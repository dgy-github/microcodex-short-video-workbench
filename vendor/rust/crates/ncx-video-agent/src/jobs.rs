use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{Result, VideoAgentError};

#[derive(Debug, Clone, PartialEq)]
pub struct JobRecord {
    pub id: String,
    pub shot_id: String,
    pub attempt: i64,
    pub idempotency_key: String,
    pub provider: String,
    pub model: String,
    pub status: String,
    pub provider_job_id: Option<String>,
    pub params_json: Value,
    pub latency_ms: Option<i64>,
    pub failure_reason: Option<String>,
    pub cost: f64,
    pub budget_reserved: f64,
    pub budget_settled: bool,
    pub candidate_set: Option<String>,
    pub is_chosen: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct JobSubmitOutcome {
    pub record: JobRecord,
    pub submitted_to_provider: bool,
}

pub fn idempotency_key(shot_id: &str, attempt: i64, params: &Value) -> String {
    let mut hasher = Sha256::new();
    hasher.update(shot_id.as_bytes());
    hasher.update([0]);
    hasher.update(attempt.to_string().as_bytes());
    hasher.update([0]);
    hasher.update(canonical_json(params).as_bytes());
    let digest = hasher.finalize();
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn submit_job_once<F>(
    conn: &mut rusqlite::Connection,
    project_id: &str,
    shot_id: &str,
    attempt: i64,
    params: &Value,
    provider: &str,
    model: &str,
    reserve_cost: f64,
    mut submit_to_provider: F,
) -> Result<JobSubmitOutcome>
where
    F: FnMut() -> std::result::Result<String, String>,
{
    validate_finite_nonnegative("reserve cost", reserve_cost)?;
    validate_nonempty_reason("provider", provider)?;
    validate_nonempty_reason("model", model)?;
    let key = idempotency_key(shot_id, attempt, params);
    if let Some(existing) = load_job_by_key(conn, &key)? {
        if existing.provider_job_id.is_none() {
            return Err(VideoAgentError::JobSubmission(format!(
                "idempotent job {} already exists with status '{}' but no provider_job_id; refusing to resubmit because provider submission state is ambiguous",
                existing.id, existing.status
            )));
        }
        return Ok(JobSubmitOutcome {
            record: existing,
            submitted_to_provider: false,
        });
    }

    let job_id = format!("job_{key}");
    reserve_and_insert_job(
        conn,
        project_id,
        shot_id,
        attempt,
        &key,
        &job_id,
        provider,
        model,
        params,
        reserve_cost,
    )?;

    match submit_to_provider() {
        Ok(provider_job_id) => {
            let provider_job_id = provider_job_id.trim().to_string();
            if provider_job_id.is_empty() {
                let reason = "provider returned empty job id".to_string();
                release_failed_reservation(conn, project_id, &job_id, &reason)?;
                return Err(VideoAgentError::JobSubmission(reason));
            }
            conn.execute(
                "UPDATE jobs SET status='submitted', provider_job_id=?1 WHERE id=?2",
                params![provider_job_id, job_id],
            )?;
            Ok(JobSubmitOutcome {
                record: load_job(conn, &job_id)?.expect("inserted job can be loaded"),
                submitted_to_provider: true,
            })
        }
        Err(err) => {
            let reason = if err.trim().is_empty() {
                "provider submission failed without reason".to_string()
            } else {
                err
            };
            release_failed_reservation(conn, project_id, &job_id, &reason)?;
            Err(VideoAgentError::JobSubmission(reason))
        }
    }
}

pub fn settle_budget(
    conn: &mut rusqlite::Connection,
    project_id: &str,
    job_id: &str,
    actual_cost: f64,
    token_used: i64,
) -> Result<()> {
    validate_finite_nonnegative("actual cost", actual_cost)?;
    if token_used < 0 {
        return Err(VideoAgentError::JobSubmission(format!(
            "token_used must be non-negative, got {token_used}"
        )));
    }
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    ensure_job_belongs_to_project(&tx, project_id, job_id)?;
    let (reserved, settled): (f64, i64) = tx.query_row(
        "SELECT budget_reserved, budget_settled FROM jobs WHERE id=?1",
        params![job_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if settled == 1 {
        tx.commit()?;
        return Ok(());
    }
    tx.execute(
        "UPDATE projects
         SET budget_reserved = budget_reserved - ?1,
             budget_spent = budget_spent + ?2
         WHERE id=?3",
        params![reserved, actual_cost, project_id],
    )?;
    tx.execute(
        "UPDATE jobs
         SET token_used=?1, cost=?2, budget_settled=1, status='settled'
         WHERE id=?3",
        params![token_used, actual_cost, job_id],
    )?;
    tx.commit()?;
    Ok(())
}

pub fn mark_job_status(conn: &rusqlite::Connection, job_id: &str, status: &str) -> Result<()> {
    let status = status.trim();
    match status {
        "provider_running" | "provider_succeeded" => {}
        "failed" | "submit_failed" => {
            return Err(VideoAgentError::JobSubmission(format!(
                "job status {status} must be recorded through failure handling with a reason"
            )));
        }
        "settled" => {
            return Err(VideoAgentError::JobSubmission(
                "job status settled must be recorded through settle_budget".to_string(),
            ));
        }
        "reserved" | "submitted" | "" => {
            return Err(VideoAgentError::JobSubmission(format!(
                "job status {status:?} is not a provider poll status"
            )));
        }
        _ => {
            return Err(VideoAgentError::JobSubmission(format!(
                "invalid provider poll job status: {status}"
            )));
        }
    }
    let changed = conn.execute(
        "UPDATE jobs SET status=?1, failure_reason=NULL WHERE id=?2",
        params![status, job_id],
    )?;
    if changed == 0 {
        return Err(VideoAgentError::JobSubmission(format!(
            "job {job_id} was not found"
        )));
    }
    Ok(())
}

pub fn record_job_latency_ms(
    conn: &rusqlite::Connection,
    job_id: &str,
    latency_ms: i64,
) -> Result<()> {
    if latency_ms < 0 {
        return Err(VideoAgentError::JobSubmission(format!(
            "job latency must be non-negative, got {latency_ms}"
        )));
    }
    let changed = conn.execute(
        "UPDATE jobs SET latency_ms=?1 WHERE id=?2",
        params![latency_ms, job_id],
    )?;
    if changed == 0 {
        return Err(VideoAgentError::JobSubmission(format!(
            "job {job_id} was not found"
        )));
    }
    Ok(())
}

pub fn fail_job_and_release_budget(
    conn: &mut rusqlite::Connection,
    project_id: &str,
    job_id: &str,
    failure_reason: &str,
) -> Result<()> {
    validate_nonempty_reason("failure_reason", failure_reason)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    ensure_job_belongs_to_project(&tx, project_id, job_id)?;
    let (reserved, settled): (f64, i64) = tx.query_row(
        "SELECT budget_reserved, budget_settled FROM jobs WHERE id=?1",
        params![job_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if settled == 1 {
        tx.commit()?;
        return Ok(());
    }
    tx.execute(
        "UPDATE projects
         SET budget_reserved = budget_reserved - ?1
         WHERE id=?2",
        params![reserved, project_id],
    )?;
    tx.execute(
        "UPDATE jobs
         SET status='failed',
             failure_reason=?1,
             budget_settled=1,
             cost=0
         WHERE id=?2",
        params![failure_reason, job_id],
    )?;
    tx.commit()?;
    Ok(())
}

fn reserve_and_insert_job(
    conn: &mut rusqlite::Connection,
    project_id: &str,
    shot_id: &str,
    attempt: i64,
    key: &str,
    job_id: &str,
    provider: &str,
    model: &str,
    params_json: &Value,
    reserve_cost: f64,
) -> Result<()> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    ensure_shot_belongs_to_project(&tx, project_id, shot_id)?;
    let (total, reserved, spent): (f64, f64, f64) = tx.query_row(
        "SELECT budget_total, budget_reserved, budget_spent FROM projects WHERE id=?1",
        params![project_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    let available = total - reserved - spent;
    if reserve_cost > available + f64::EPSILON {
        return Err(VideoAgentError::BudgetExhausted {
            project_id: project_id.to_string(),
            requested: reserve_cost,
            available,
        });
    }

    tx.execute(
        "UPDATE projects
         SET budget_reserved = budget_reserved + ?1
         WHERE id=?2",
        params![reserve_cost, project_id],
    )?;
    tx.execute(
        "INSERT INTO jobs(
            id, shot_id, attempt, idempotency_key, provider, model, status,
            params_json, budget_reserved
         )
         VALUES(?1, ?2, ?3, ?4, ?5, ?6, 'reserved', ?7, ?8)",
        params![
            job_id,
            shot_id,
            attempt,
            key,
            provider,
            model,
            canonical_json(params_json),
            reserve_cost
        ],
    )?;
    tx.commit()?;
    Ok(())
}

fn release_failed_reservation(
    conn: &mut rusqlite::Connection,
    project_id: &str,
    job_id: &str,
    failure_reason: &str,
) -> Result<()> {
    validate_nonempty_reason("failure_reason", failure_reason)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    ensure_job_belongs_to_project(&tx, project_id, job_id)?;
    let reserved: f64 = tx.query_row(
        "SELECT budget_reserved FROM jobs WHERE id=?1",
        params![job_id],
        |row| row.get(0),
    )?;
    tx.execute(
        "UPDATE projects
         SET budget_reserved = budget_reserved - ?1
         WHERE id=?2",
        params![reserved, project_id],
    )?;
    tx.execute(
        "UPDATE jobs
         SET status='submit_failed',
             failure_reason=?1,
             budget_settled=1,
             cost=0
         WHERE id=?2",
        params![failure_reason, job_id],
    )?;
    tx.commit()?;
    Ok(())
}

fn ensure_shot_belongs_to_project(
    tx: &rusqlite::Transaction<'_>,
    project_id: &str,
    shot_id: &str,
) -> Result<()> {
    let actual_project_id: String = tx.query_row(
        "SELECT ch.project_id
         FROM shots sh
         JOIN scenes sc ON sc.id = sh.scene_id
         JOIN chapters ch ON ch.id = sc.chapter_id
         WHERE sh.id=?1",
        params![shot_id],
        |row| row.get(0),
    )?;
    if actual_project_id != project_id {
        return Err(VideoAgentError::JobSubmission(format!(
            "shot {shot_id} belongs to project {actual_project_id}, not {project_id}"
        )));
    }
    Ok(())
}

fn ensure_job_belongs_to_project(
    tx: &rusqlite::Transaction<'_>,
    project_id: &str,
    job_id: &str,
) -> Result<()> {
    let actual_project_id: String = tx.query_row(
        "SELECT ch.project_id
         FROM jobs j
         JOIN shots sh ON sh.id = j.shot_id
         JOIN scenes sc ON sc.id = sh.scene_id
         JOIN chapters ch ON ch.id = sc.chapter_id
         WHERE j.id=?1",
        params![job_id],
        |row| row.get(0),
    )?;
    if actual_project_id != project_id {
        return Err(VideoAgentError::JobSubmission(format!(
            "job {job_id} belongs to project {actual_project_id}, not {project_id}"
        )));
    }
    Ok(())
}

fn validate_finite_nonnegative(label: &str, value: f64) -> Result<()> {
    if !value.is_finite() || value < 0.0 {
        return Err(VideoAgentError::JobSubmission(format!(
            "{label} must be finite and non-negative, got {value}"
        )));
    }
    Ok(())
}

fn validate_nonempty_reason(label: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(VideoAgentError::JobSubmission(format!(
            "{label} must not be empty"
        )));
    }
    Ok(())
}

fn load_job_by_key(conn: &rusqlite::Connection, key: &str) -> Result<Option<JobRecord>> {
    conn.query_row(
        "SELECT id, shot_id, attempt, idempotency_key, provider, model, status,
                provider_job_id, params_json, latency_ms, failure_reason,
                cost, budget_reserved, budget_settled,
                candidate_set, is_chosen
         FROM jobs WHERE idempotency_key=?1",
        params![key],
        row_to_job,
    )
    .optional()
    .map_err(Into::into)
}

fn load_job(conn: &rusqlite::Connection, id: &str) -> Result<Option<JobRecord>> {
    conn.query_row(
        "SELECT id, shot_id, attempt, idempotency_key, provider, model, status,
                provider_job_id, params_json, latency_ms, failure_reason,
                cost, budget_reserved, budget_settled,
                candidate_set, is_chosen
         FROM jobs WHERE id=?1",
        params![id],
        row_to_job,
    )
    .optional()
    .map_err(Into::into)
}

fn row_to_job(row: &rusqlite::Row<'_>) -> rusqlite::Result<JobRecord> {
    let params_raw: String = row.get(8)?;
    let params_json = serde_json::from_str(&params_raw).map_err(|err| {
        rusqlite::Error::FromSqlConversionFailure(8, rusqlite::types::Type::Text, Box::new(err))
    })?;
    let budget_settled: i64 = row.get(13)?;
    let is_chosen: i64 = row.get(15)?;
    Ok(JobRecord {
        id: row.get(0)?,
        shot_id: row.get(1)?,
        attempt: row.get(2)?,
        idempotency_key: row.get(3)?,
        provider: row.get(4)?,
        model: row.get(5)?,
        status: row.get(6)?,
        provider_job_id: row.get(7)?,
        params_json,
        latency_ms: row.get(9)?,
        failure_reason: row.get(10)?,
        cost: row.get(11)?,
        budget_reserved: row.get(12)?,
        budget_settled: budget_settled == 1,
        candidate_set: row.get(14)?,
        is_chosen: is_chosen == 1,
    })
}

fn canonical_json(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(v) => v.to_string(),
        Value::Number(v) => v.to_string(),
        Value::String(v) => serde_json::to_string(v).expect("string serialization cannot fail"),
        Value::Array(items) => {
            let body = items
                .iter()
                .map(canonical_json)
                .collect::<Vec<_>>()
                .join(",");
            format!("[{body}]")
        }
        Value::Object(map) => {
            let mut keys = map.keys().collect::<Vec<_>>();
            keys.sort();
            let body = keys
                .into_iter()
                .map(|key| {
                    let k = serde_json::to_string(key).expect("key serialization cannot fail");
                    let v = canonical_json(&map[key]);
                    format!("{k}:{v}")
                })
                .collect::<Vec<_>>()
                .join(",");
            format!("{{{body}}}")
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Barrier};
    use std::thread;

    use serde_json::json;

    use super::*;
    use crate::db::Database;
    use crate::test_support::temp_db_path;

    fn seeded_db(name: &str, budget: f64) -> (std::path::PathBuf, Database) {
        let path = temp_db_path(name);
        let db = Database::open(&path).expect("open db");
        db.create_project("p", budget).unwrap();
        db.create_chapter("c", "p", "{}").unwrap();
        db.create_scene("s", "c", "{}").unwrap();
        db.create_shot(
            "shot",
            "s",
            "{\"duration_s\":1}",
            None,
            None,
            false,
            "standard",
        )
        .unwrap();
        (path, db)
    }

    #[test]
    fn idempotency_key_canonicalizes_json_object_order() {
        let a = json!({"duration": 2, "params": {"b": true, "a": 1}});
        let b = json!({"params": {"a": 1, "b": true}, "duration": 2});
        assert_eq!(
            idempotency_key("shot", 0, &a),
            idempotency_key("shot", 0, &b)
        );
        assert_ne!(
            idempotency_key("shot", 0, &a),
            idempotency_key("shot", 1, &a)
        );
    }

    #[test]
    fn submit_job_is_idempotent_and_reserves_once() {
        let (path, mut db) = seeded_db("submit", 100.0);
        let mut api_calls = 0;
        let params = json!({"prompt": "no text overlays", "duration_s": 1});

        let first = submit_job_once(
            db.connection_mut(),
            "p",
            "shot",
            0,
            &params,
            "ark",
            "seedance",
            20.0,
            || {
                api_calls += 1;
                Ok("ark-job-1".to_string())
            },
        )
        .unwrap();
        assert!(first.submitted_to_provider);
        assert_eq!(api_calls, 1);

        let second = submit_job_once(
            db.connection_mut(),
            "p",
            "shot",
            0,
            &params,
            "ark",
            "seedance",
            20.0,
            || {
                api_calls += 1;
                Ok("ark-job-duplicate".to_string())
            },
        )
        .unwrap();
        assert!(!second.submitted_to_provider);
        assert_eq!(api_calls, 1, "provider should not be called twice");
        assert_eq!(first.record.id, second.record.id);

        let reserved: f64 = db
            .connection()
            .query_row(
                "SELECT budget_reserved FROM projects WHERE id='p'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(reserved, 20.0);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn submit_job_rejects_invalid_reserve_cost_before_provider_call() {
        let (path, mut db) = seeded_db("invalid-reserve-cost", 100.0);
        let mut api_calls = 0;

        let err = submit_job_once(
            db.connection_mut(),
            "p",
            "shot",
            0,
            &json!({"prompt": "no text overlays", "duration_s": 1}),
            "ark",
            "seedance",
            -1.0,
            || {
                api_calls += 1;
                Ok("ark-job-should-not-submit".to_string())
            },
        )
        .expect_err("negative reserve cost should be rejected");

        assert_eq!(api_calls, 0, "provider must not be called");
        assert!(err.to_string().contains("reserve cost"));
        let (reserved, spent, jobs): (f64, f64, i64) = db
            .connection()
            .query_row(
                "SELECT p.budget_reserved, p.budget_spent, COUNT(j.id)
                 FROM projects p LEFT JOIN jobs j ON 1=1
                 WHERE p.id='p'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(reserved, 0.0);
        assert_eq!(spent, 0.0);
        assert_eq!(jobs, 0);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn submit_job_rejects_empty_provider_or_model_before_provider_call() {
        let (path, mut db) = seeded_db("empty-provider-model", 100.0);
        let mut api_calls = 0;

        let err = submit_job_once(
            db.connection_mut(),
            "p",
            "shot",
            0,
            &json!({"duration_s": 1}),
            "   ",
            "seedance",
            25.0,
            || {
                api_calls += 1;
                Ok("ark-job-should-not-submit".to_string())
            },
        )
        .expect_err("empty provider should be rejected");
        assert_eq!(api_calls, 0, "provider must not be called");
        assert!(err.to_string().contains("provider"));

        let err = submit_job_once(
            db.connection_mut(),
            "p",
            "shot",
            1,
            &json!({"duration_s": 1}),
            "ark",
            "   ",
            25.0,
            || {
                api_calls += 1;
                Ok("ark-job-should-not-submit".to_string())
            },
        )
        .expect_err("empty model should be rejected");
        assert_eq!(api_calls, 0, "provider must not be called");
        assert!(err.to_string().contains("model"));

        let (reserved, spent, jobs): (f64, f64, i64) = db
            .connection()
            .query_row(
                "SELECT p.budget_reserved, p.budget_spent, COUNT(j.id)
                 FROM projects p LEFT JOIN jobs j ON 1=1
                 WHERE p.id='p'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(reserved, 0.0);
        assert_eq!(spent, 0.0);
        assert_eq!(jobs, 0);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn submit_job_releases_budget_when_provider_returns_empty_job_id() {
        let (path, mut db) = seeded_db("empty-provider-job-id", 100.0);
        let mut api_calls = 0;

        let err = submit_job_once(
            db.connection_mut(),
            "p",
            "shot",
            0,
            &json!({"duration_s": 1}),
            "ark",
            "seedance",
            25.0,
            || {
                api_calls += 1;
                Ok("   ".to_string())
            },
        )
        .expect_err("empty provider job id should fail safely");

        assert_eq!(api_calls, 1, "provider was called once");
        assert!(err.to_string().contains("empty job id"));
        let (reserved, spent, status, reason, settled, provider_job_id): (
            f64,
            f64,
            String,
            Option<String>,
            i64,
            Option<String>,
        ) = db
            .connection()
            .query_row(
                "SELECT p.budget_reserved, p.budget_spent, j.status, j.failure_reason,
                        j.budget_settled, j.provider_job_id
                 FROM projects p JOIN jobs j ON j.shot_id='shot'
                 WHERE p.id='p'",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(reserved, 0.0);
        assert_eq!(spent, 0.0);
        assert_eq!(status, "submit_failed");
        assert_eq!(reason.as_deref(), Some("provider returned empty job id"));
        assert_eq!(settled, 1);
        assert_eq!(provider_job_id, None);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn submit_job_rejects_cross_project_shot_without_mutating_ledger() {
        let (path, mut db) = seeded_db("submit-cross-project-shot", 100.0);
        db.create_project("p2", 100.0).unwrap();
        let mut api_calls = 0;

        let err = submit_job_once(
            db.connection_mut(),
            "p2",
            "shot",
            0,
            &json!({"duration_s": 1}),
            "ark",
            "seedance",
            25.0,
            || {
                api_calls += 1;
                Ok("ark-job-should-not-submit".to_string())
            },
        )
        .expect_err("shot from p must not reserve budget from p2");

        assert_eq!(api_calls, 0, "provider must not be called");
        assert!(err.to_string().contains("belongs to project p, not p2"));
        let (p_reserved, p_spent, p2_reserved, p2_spent, jobs): (f64, f64, f64, f64, i64) = db
            .connection()
            .query_row(
                "SELECT
                    (SELECT budget_reserved FROM projects WHERE id='p'),
                    (SELECT budget_spent FROM projects WHERE id='p'),
                    (SELECT budget_reserved FROM projects WHERE id='p2'),
                    (SELECT budget_spent FROM projects WHERE id='p2'),
                    (SELECT COUNT(*) FROM jobs)",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(p_reserved, 0.0);
        assert_eq!(p_spent, 0.0);
        assert_eq!(p2_reserved, 0.0);
        assert_eq!(p2_spent, 0.0);
        assert_eq!(jobs, 0);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn submit_job_refuses_to_resubmit_ambiguous_existing_job_without_provider_id() {
        let (path, mut db) = seeded_db("ambiguous-submit", 100.0);
        let params = json!({"prompt": "no text overlays", "duration_s": 1});
        let key = idempotency_key("shot", 0, &params);
        db.connection()
            .execute("UPDATE projects SET budget_reserved=20 WHERE id='p'", [])
            .unwrap();
        db.connection()
            .execute(
                "INSERT INTO jobs(
                id, shot_id, attempt, idempotency_key, provider, model, status,
                params_json, budget_reserved
             )
             VALUES(?1, 'shot', 0, ?2, 'ark', 'seedance', 'reserved', ?3, 20)",
                params![format!("job_{key}"), key, canonical_json(&params)],
            )
            .unwrap();

        let mut api_calls = 0;
        let err = submit_job_once(
            db.connection_mut(),
            "p",
            "shot",
            0,
            &params,
            "ark",
            "seedance",
            20.0,
            || {
                api_calls += 1;
                Ok("ark-job-duplicate".to_string())
            },
        )
        .expect_err("ambiguous existing submit must require reconciliation");

        assert_eq!(api_calls, 0, "provider must not be called again");
        assert!(err.to_string().contains("no provider_job_id"));
        assert!(err.to_string().contains("refusing to resubmit"));

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn submit_job_refuses_to_retry_submit_failed_job_without_provider_id() {
        let (path, mut db) = seeded_db("submit-failed-retry", 100.0);
        let params = json!({"prompt": "no text overlays", "duration_s": 1});
        let first = submit_job_once(
            db.connection_mut(),
            "p",
            "shot",
            0,
            &params,
            "ark",
            "seedance",
            20.0,
            || Err("transport timed out after request body was sent".to_string()),
        )
        .expect_err("first submit should surface transport error");
        assert!(first.to_string().contains("transport timed out"));

        let mut api_calls = 0;
        let second = submit_job_once(
            db.connection_mut(),
            "p",
            "shot",
            0,
            &params,
            "ark",
            "seedance",
            20.0,
            || {
                api_calls += 1;
                Ok("ark-job-duplicate".to_string())
            },
        )
        .expect_err("submit_failed retry must not silently resubmit");

        assert_eq!(api_calls, 0, "provider must not be retried ambiguously");
        assert!(second.to_string().contains("submit_failed"));
        assert!(second.to_string().contains("no provider_job_id"));

        let reason: Option<String> = db
            .connection()
            .query_row(
                "SELECT failure_reason FROM jobs WHERE shot_id='shot'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            reason.as_deref(),
            Some("transport timed out after request body was sent")
        );

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn settle_budget_reconciles_project_and_job_once() {
        let (path, mut db) = seeded_db("settle", 100.0);
        let outcome = submit_job_once(
            db.connection_mut(),
            "p",
            "shot",
            0,
            &json!({"duration_s": 1}),
            "ark",
            "seedance",
            25.0,
            || Ok("ark-job-1".to_string()),
        )
        .unwrap();

        settle_budget(db.connection_mut(), "p", &outcome.record.id, 18.5, 0).unwrap();
        settle_budget(db.connection_mut(), "p", &outcome.record.id, 18.5, 0).unwrap();

        let (reserved, spent): (f64, f64) = db
            .connection()
            .query_row(
                "SELECT budget_reserved, budget_spent FROM projects WHERE id='p'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(reserved, 0.0);
        assert_eq!(spent, 18.5);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn settle_budget_rejects_invalid_cost_or_tokens_without_mutating_ledger() {
        let (path, mut db) = seeded_db("settle-invalid", 100.0);
        let outcome = submit_job_once(
            db.connection_mut(),
            "p",
            "shot",
            0,
            &json!({"duration_s": 1}),
            "ark",
            "seedance",
            25.0,
            || Ok("ark-job-1".to_string()),
        )
        .unwrap();

        let err = settle_budget(db.connection_mut(), "p", &outcome.record.id, -0.01, 0)
            .expect_err("negative actual cost should be rejected");
        assert!(err.to_string().contains("actual cost"));

        let err = settle_budget(db.connection_mut(), "p", &outcome.record.id, 1.0, -1)
            .expect_err("negative token usage should be rejected");
        assert!(err.to_string().contains("token_used"));

        let (reserved, spent, job_cost, settled, status): (f64, f64, f64, i64, String) = db
            .connection()
            .query_row(
                "SELECT p.budget_reserved, p.budget_spent, j.cost, j.budget_settled, j.status
                 FROM projects p JOIN jobs j ON j.id=?1
                 WHERE p.id='p'",
                params![outcome.record.id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(reserved, 25.0);
        assert_eq!(spent, 0.0);
        assert_eq!(job_cost, 0.0);
        assert_eq!(settled, 0);
        assert_eq!(status, "submitted");

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn settle_and_fail_reject_cross_project_job_without_mutating_ledgers() {
        let (path, mut db) = seeded_db("settle-fail-cross-project", 100.0);
        db.create_project("p2", 100.0).unwrap();
        let outcome = submit_job_once(
            db.connection_mut(),
            "p",
            "shot",
            0,
            &json!({"duration_s": 1}),
            "ark",
            "seedance",
            25.0,
            || Ok("ark-job-1".to_string()),
        )
        .unwrap();

        let err = settle_budget(db.connection_mut(), "p2", &outcome.record.id, 18.5, 0)
            .expect_err("job from p must not settle against p2");
        assert!(err.to_string().contains("belongs to project p, not p2"));

        let err = fail_job_and_release_budget(
            db.connection_mut(),
            "p2",
            &outcome.record.id,
            "wrong project failure",
        )
        .expect_err("job from p must not release budget from p2");
        assert!(err.to_string().contains("belongs to project p, not p2"));

        let (p_reserved, p_spent, p2_reserved, p2_spent, job_cost, settled, status): (
            f64,
            f64,
            f64,
            f64,
            f64,
            i64,
            String,
        ) = db
            .connection()
            .query_row(
                "SELECT
                    (SELECT budget_reserved FROM projects WHERE id='p'),
                    (SELECT budget_spent FROM projects WHERE id='p'),
                    (SELECT budget_reserved FROM projects WHERE id='p2'),
                    (SELECT budget_spent FROM projects WHERE id='p2'),
                    j.cost,
                    j.budget_settled,
                    j.status
                 FROM jobs j WHERE j.id=?1",
                params![outcome.record.id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(p_reserved, 25.0);
        assert_eq!(p_spent, 0.0);
        assert_eq!(p2_reserved, 0.0);
        assert_eq!(p2_spent, 0.0);
        assert_eq!(job_cost, 0.0);
        assert_eq!(settled, 0);
        assert_eq!(status, "submitted");

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn mark_job_status_only_allows_provider_poll_statuses() {
        let (path, mut db) = seeded_db("mark-status-contract", 100.0);
        let outcome = submit_job_once(
            db.connection_mut(),
            "p",
            "shot",
            0,
            &json!({"duration_s": 1}),
            "ark",
            "seedance",
            25.0,
            || Ok("ark-job-1".to_string()),
        )
        .unwrap();

        mark_job_status(db.connection(), &outcome.record.id, "provider_running").unwrap();
        mark_job_status(db.connection(), &outcome.record.id, "provider_succeeded").unwrap();

        for status in [
            "failed",
            "submit_failed",
            "settled",
            "reserved",
            "submitted",
            "done",
            " ",
        ] {
            let err = mark_job_status(db.connection(), &outcome.record.id, status)
                .expect_err("invalid status path should be rejected");
            assert!(
                err.to_string().contains("job status")
                    || err.to_string().contains("invalid provider poll job status"),
                "unexpected error for status {status:?}: {err}"
            );
        }

        let (reserved, spent, status, reason): (f64, f64, String, Option<String>) = db
            .connection()
            .query_row(
                "SELECT p.budget_reserved, p.budget_spent, j.status, j.failure_reason
                 FROM projects p JOIN jobs j ON j.id=?1
                 WHERE p.id='p'",
                params![outcome.record.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(reserved, 25.0);
        assert_eq!(spent, 0.0);
        assert_eq!(status, "provider_succeeded");
        assert_eq!(reason, None);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn status_and_latency_updates_reject_missing_jobs() {
        let (path, db) = seeded_db("missing-job-updates", 100.0);

        let err = mark_job_status(db.connection(), "missing-job", "provider_running")
            .expect_err("missing status update should fail");
        assert!(err.to_string().contains("missing-job"));

        let err = record_job_latency_ms(db.connection(), "missing-job", 25)
            .expect_err("missing latency update should fail");
        assert!(err.to_string().contains("missing-job"));

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn concurrent_reservations_never_exceed_project_budget() {
        let (path, db) = seeded_db("concurrent", 100.0);
        drop(db);

        let barrier = Arc::new(Barrier::new(8));
        let mut handles = Vec::new();
        for idx in 0..8 {
            let path = path.clone();
            let barrier = barrier.clone();
            handles.push(thread::spawn(move || {
                let mut db = Database::open(&path).expect("open db in thread");
                barrier.wait();
                submit_job_once(
                    db.connection_mut(),
                    "p",
                    "shot",
                    idx,
                    &json!({"duration_s": 1, "seed": idx}),
                    "ark",
                    "seedance",
                    15.0,
                    || Ok(format!("ark-job-{idx}")),
                )
            }));
        }

        let mut accepted = 0;
        let mut exhausted = 0;
        for handle in handles {
            match handle.join().unwrap() {
                Ok(_) => accepted += 1,
                Err(VideoAgentError::BudgetExhausted { .. }) => exhausted += 1,
                Err(err) => panic!("unexpected error: {err}"),
            }
        }
        assert_eq!(accepted, 6);
        assert_eq!(exhausted, 2);

        let db = Database::open(&path).unwrap();
        let reserved: f64 = db
            .connection()
            .query_row(
                "SELECT budget_reserved FROM projects WHERE id='p'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(reserved, 90.0);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn failed_provider_job_releases_reserved_budget_once() {
        let (path, mut db) = seeded_db("fail-release", 100.0);
        let outcome = submit_job_once(
            db.connection_mut(),
            "p",
            "shot",
            0,
            &json!({"duration_s": 1}),
            "ark",
            "seedance",
            25.0,
            || Ok("ark-job-1".to_string()),
        )
        .unwrap();

        fail_job_and_release_budget(db.connection_mut(), "p", &outcome.record.id, "task failed")
            .unwrap();
        fail_job_and_release_budget(
            db.connection_mut(),
            "p",
            &outcome.record.id,
            "task failed again",
        )
        .unwrap();

        let (reserved, spent): (f64, f64) = db
            .connection()
            .query_row(
                "SELECT budget_reserved, budget_spent FROM projects WHERE id='p'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        let (status, reason, settled): (String, Option<String>, i64) = db
            .connection()
            .query_row(
                "SELECT status, failure_reason, budget_settled FROM jobs WHERE id=?1",
                params![outcome.record.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(reserved, 0.0);
        assert_eq!(spent, 0.0);
        assert_eq!(status, "failed");
        assert_eq!(reason.as_deref(), Some("task failed"));
        assert_eq!(settled, 1);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn failed_provider_job_requires_failure_reason() {
        let (path, mut db) = seeded_db("fail-empty-reason", 100.0);
        let outcome = submit_job_once(
            db.connection_mut(),
            "p",
            "shot",
            0,
            &json!({"duration_s": 1}),
            "ark",
            "seedance",
            25.0,
            || Ok("ark-job-1".to_string()),
        )
        .unwrap();

        let err = fail_job_and_release_budget(db.connection_mut(), "p", &outcome.record.id, "  ")
            .expect_err("empty failure reason should be rejected");
        assert!(err.to_string().contains("failure_reason"));

        let (reserved, status, reason): (f64, String, Option<String>) = db
            .connection()
            .query_row(
                "SELECT p.budget_reserved, j.status, j.failure_reason
                 FROM projects p JOIN jobs j ON j.id=?1
                 WHERE p.id='p'",
                params![outcome.record.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(reserved, 25.0);
        assert_eq!(status, "submitted");
        assert_eq!(reason, None);

        let _ = std::fs::remove_file(path);
    }
}
