use std::path::Path;
use std::time::Duration;

use rusqlite::{params, Connection};
use serde_json::json;

use crate::Result;

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = open_db(path)?;
        Ok(Self { conn })
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    pub fn connection_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }

    pub fn create_project(&self, id: &str, budget_total: f64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO projects(id, brief_json, status, budget_total)
             VALUES(?1, ?2, 'new', ?3)",
            params![id, json!({}).to_string(), budget_total],
        )?;
        Ok(())
    }

    pub fn create_chapter(&self, id: &str, project_id: &str, plan_json: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO chapters(id, project_id, plan_json, status)
             VALUES(?1, ?2, ?3, 'new')",
            params![id, project_id, plan_json],
        )?;
        Ok(())
    }

    pub fn create_scene(&self, id: &str, chapter_id: &str, plan_json: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO scenes(id, chapter_id, plan_json, status)
             VALUES(?1, ?2, ?3, 'new')",
            params![id, chapter_id, plan_json],
        )?;
        Ok(())
    }

    pub fn create_shot(
        &self,
        id: &str,
        scene_id: &str,
        plan_json: &str,
        continuity_in: Option<&str>,
        continuity_out: Option<&str>,
        is_hero: bool,
        tier: &str,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO shots(
                id, scene_id, plan_json, status, continuity_in, continuity_out,
                risk_level, is_hero, tier
             )
             VALUES(?1, ?2, ?3, 'new', ?4, ?5, 'normal', ?6, ?7)",
            params![
                id,
                scene_id,
                plan_json,
                continuity_in,
                continuity_out,
                i64::from(is_hero),
                tier
            ],
        )?;
        Ok(())
    }

    pub fn create_artifact(
        &self,
        id: &str,
        shot_id: Option<&str>,
        kind: &str,
        tos_key: &str,
        content_hash: &str,
        params_json: &str,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO artifacts(id, shot_id, kind, tos_key, content_hash, params_json)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, shot_id, kind, tos_key, content_hash, params_json],
        )?;
        Ok(())
    }

    pub fn create_project_artifact(
        &self,
        id: &str,
        project_id: &str,
        kind: &str,
        tos_key: &str,
        content_hash: &str,
        params_json: &str,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO artifacts(id, project_id, shot_id, kind, tos_key, content_hash, params_json)
             VALUES(?1, ?2, NULL, ?3, ?4, ?5, ?6)",
            params![id, project_id, kind, tos_key, content_hash, params_json],
        )?;
        Ok(())
    }
}

pub fn open_db(path: impl AsRef<Path>) -> Result<Connection> {
    let conn = Connection::open(path)?;
    conn.busy_timeout(Duration::from_secs(5))?;
    let _: String = conn.query_row("PRAGMA journal_mode=WAL", [], |row| row.get(0))?;
    conn.execute_batch("PRAGMA foreign_keys=ON;")?;
    init_schema(&conn)?;
    require_json1(&conn)?;
    Ok(conn)
}

pub fn require_json1(conn: &Connection) -> Result<()> {
    let value: i64 = conn.query_row("SELECT json_extract('{\"x\":7}', '$.x')", [], |row| {
        row.get(0)
    })?;
    debug_assert_eq!(value, 7);
    Ok(())
}

pub fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS projects(
            id TEXT PRIMARY KEY,
            brief_json TEXT NOT NULL CHECK(json_valid(brief_json)),
            status TEXT NOT NULL,
            budget_total REAL NOT NULL CHECK(budget_total >= 0),
            budget_reserved REAL NOT NULL DEFAULT 0 CHECK(budget_reserved >= 0),
            budget_spent REAL NOT NULL DEFAULT 0 CHECK(budget_spent >= 0),
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS chapters(
            id TEXT PRIMARY KEY,
            project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            plan_json TEXT NOT NULL CHECK(json_valid(plan_json)),
            status TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS scenes(
            id TEXT PRIMARY KEY,
            chapter_id TEXT NOT NULL REFERENCES chapters(id) ON DELETE CASCADE,
            plan_json TEXT NOT NULL CHECK(json_valid(plan_json)),
            status TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS shots(
            id TEXT PRIMARY KEY,
            scene_id TEXT NOT NULL REFERENCES scenes(id) ON DELETE CASCADE,
            plan_json TEXT NOT NULL CHECK(json_valid(plan_json)),
            status TEXT NOT NULL,
            continuity_in TEXT,
            continuity_out TEXT,
            risk_level TEXT NOT NULL DEFAULT 'normal',
            is_hero INTEGER NOT NULL DEFAULT 0 CHECK(is_hero IN (0, 1)),
            tier TEXT NOT NULL DEFAULT 'standard' CHECK(tier IN ('hero', 'standard', 'filler'))
        );

        CREATE TABLE IF NOT EXISTS artifacts(
            id TEXT PRIMARY KEY,
            project_id TEXT REFERENCES projects(id) ON DELETE CASCADE,
            shot_id TEXT REFERENCES shots(id) ON DELETE SET NULL,
            kind TEXT NOT NULL CHECK(length(trim(kind)) > 0),
            tos_key TEXT NOT NULL CHECK(length(trim(tos_key)) > 0),
            content_hash TEXT NOT NULL CHECK(length(trim(content_hash)) > 0),
            params_json TEXT NOT NULL CHECK(json_valid(params_json)),
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            CHECK(
                (project_id IS NOT NULL AND shot_id IS NULL)
                OR (project_id IS NULL AND shot_id IS NOT NULL)
            )
        );

        CREATE TABLE IF NOT EXISTS jobs(
            id TEXT PRIMARY KEY,
            shot_id TEXT NOT NULL REFERENCES shots(id) ON DELETE CASCADE,
            attempt INTEGER NOT NULL CHECK(attempt >= 0),
            idempotency_key TEXT NOT NULL UNIQUE CHECK(length(trim(idempotency_key)) > 0),
            provider TEXT NOT NULL CHECK(length(trim(provider)) > 0),
            model TEXT NOT NULL CHECK(length(trim(model)) > 0),
            status TEXT NOT NULL CHECK(status IN (
                'reserved', 'submitted', 'provider_running', 'provider_succeeded',
                'settled', 'failed', 'submit_failed'
            )),
            provider_job_id TEXT,
            params_json TEXT NOT NULL DEFAULT '{}' CHECK(json_valid(params_json)),
            token_used INTEGER NOT NULL DEFAULT 0 CHECK(token_used >= 0),
            cost REAL NOT NULL DEFAULT 0 CHECK(cost >= 0),
            latency_ms INTEGER CHECK(latency_ms IS NULL OR latency_ms >= 0),
            failure_reason TEXT,
            budget_reserved REAL NOT NULL DEFAULT 0 CHECK(budget_reserved >= 0),
            budget_settled INTEGER NOT NULL DEFAULT 0 CHECK(budget_settled IN (0, 1)),
            candidate_set TEXT,
            is_chosen INTEGER NOT NULL DEFAULT 0 CHECK(is_chosen IN (0, 1)),
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            CHECK(status NOT IN (
                'submitted', 'provider_running', 'provider_succeeded', 'settled'
            ) OR (
                provider_job_id IS NOT NULL AND length(trim(provider_job_id)) > 0
            )),
            CHECK(status NOT IN ('failed', 'submit_failed') OR (
                failure_reason IS NOT NULL AND length(trim(failure_reason)) > 0
            )),
            CHECK(status NOT IN ('failed', 'submit_failed', 'settled') OR budget_settled = 1)
        );

        CREATE TABLE IF NOT EXISTS validation_records(
            id TEXT PRIMARY KEY CHECK(length(trim(id)) > 0),
            artifact_id TEXT NOT NULL REFERENCES artifacts(id) ON DELETE CASCADE CHECK(length(trim(artifact_id)) > 0),
            stage TEXT NOT NULL CHECK(length(trim(stage)) > 0),
            gate_version TEXT NOT NULL CHECK(length(trim(gate_version)) > 0),
            verdict TEXT NOT NULL CHECK(verdict IN ('pass', 'repair', 'escalate')),
            confidence REAL NOT NULL CHECK(confidence >= 0 AND confidence <= 1),
            aesthetic_score REAL CHECK(aesthetic_score IS NULL OR (aesthetic_score >= 0 AND aesthetic_score <= 1)),
            layers_json TEXT NOT NULL CHECK(json_valid(layers_json) AND json_type(layers_json) <> 'null'),
            escalate_reason TEXT,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            CHECK(verdict <> 'escalate' OR (
                escalate_reason IS NOT NULL AND length(trim(escalate_reason)) > 0
            ))
        );

        CREATE UNIQUE INDEX IF NOT EXISTS validation_pass_once
        ON validation_records(artifact_id, stage)
        WHERE verdict='pass';

        CREATE TABLE IF NOT EXISTS golden_cases(
            id TEXT PRIMARY KEY,
            stage TEXT NOT NULL,
            failure_type TEXT NOT NULL,
            tos_key TEXT NOT NULL,
            human_verdict TEXT NOT NULL,
            human_score REAL,
            is_exemplar INTEGER NOT NULL DEFAULT 0 CHECK(is_exemplar IN (0, 1)),
            source TEXT NOT NULL,
            added_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS gate_metrics(
            stage TEXT NOT NULL,
            gate_version TEXT NOT NULL,
            pass_precision REAL CHECK(pass_precision IS NULL OR (pass_precision >= 0 AND pass_precision <= 1)),
            escalate_rate REAL CHECK(escalate_rate IS NULL OR (escalate_rate >= 0 AND escalate_rate <= 1)),
            judge_cost REAL CHECK(judge_cost IS NULL OR judge_cost >= 0),
            human_agreement REAL CHECK(human_agreement IS NULL OR (human_agreement >= 0 AND human_agreement <= 1)),
            PRIMARY KEY(stage, gate_version)
        );

        CREATE TABLE IF NOT EXISTS model_metrics(
            provider TEXT NOT NULL,
            model TEXT NOT NULL,
            task TEXT NOT NULL,
            pass_rate REAL CHECK(pass_rate IS NULL OR (pass_rate >= 0 AND pass_rate <= 1)),
            avg_cost REAL CHECK(avg_cost IS NULL OR avg_cost >= 0),
            avg_latency REAL CHECK(avg_latency IS NULL OR avg_latency >= 0),
            PRIMARY KEY(provider, model, task)
        );
        "#,
    )?;
    ensure_artifact_project_id(conn)?;
    ensure_artifact_owner_triggers(conn)?;
    ensure_artifact_trace_field_triggers(conn)?;
    ensure_job_trace_field_triggers(conn)?;
    ensure_job_contract_triggers(conn)?;
    ensure_validation_record_contract_triggers(conn)?;
    Ok(())
}

fn ensure_artifact_project_id(conn: &Connection) -> Result<()> {
    let mut stmt = conn.prepare("PRAGMA table_info(artifacts)")?;
    let has_project_id = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<std::result::Result<Vec<_>, _>>()?
        .iter()
        .any(|name| name == "project_id");
    if !has_project_id {
        conn.execute(
            "ALTER TABLE artifacts ADD COLUMN project_id TEXT REFERENCES projects(id) ON DELETE CASCADE",
            [],
        )?;
    }
    Ok(())
}

fn ensure_artifact_owner_triggers(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TRIGGER IF NOT EXISTS artifacts_exactly_one_owner_insert
        BEFORE INSERT ON artifacts
        WHEN NOT (
            (NEW.project_id IS NOT NULL AND NEW.shot_id IS NULL)
            OR (NEW.project_id IS NULL AND NEW.shot_id IS NOT NULL)
        )
        BEGIN
            SELECT RAISE(ABORT, 'artifact must have exactly one owner');
        END;

        CREATE TRIGGER IF NOT EXISTS artifacts_exactly_one_owner_update
        BEFORE UPDATE ON artifacts
        WHEN NOT (
            (NEW.project_id IS NOT NULL AND NEW.shot_id IS NULL)
            OR (NEW.project_id IS NULL AND NEW.shot_id IS NOT NULL)
        )
        BEGIN
            SELECT RAISE(ABORT, 'artifact must have exactly one owner');
        END;
        "#,
    )?;
    Ok(())
}

fn ensure_artifact_trace_field_triggers(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TRIGGER IF NOT EXISTS artifacts_nonempty_trace_fields_insert
        BEFORE INSERT ON artifacts
        WHEN length(trim(NEW.kind)) = 0
          OR length(trim(NEW.tos_key)) = 0
          OR length(trim(NEW.content_hash)) = 0
        BEGIN
            SELECT RAISE(ABORT, 'artifact kind, tos_key, and content_hash must be non-empty');
        END;

        CREATE TRIGGER IF NOT EXISTS artifacts_nonempty_trace_fields_update
        BEFORE UPDATE ON artifacts
        WHEN length(trim(NEW.kind)) = 0
          OR length(trim(NEW.tos_key)) = 0
          OR length(trim(NEW.content_hash)) = 0
        BEGIN
            SELECT RAISE(ABORT, 'artifact kind, tos_key, and content_hash must be non-empty');
        END;
        "#,
    )?;
    Ok(())
}

fn ensure_job_trace_field_triggers(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TRIGGER IF NOT EXISTS jobs_nonempty_trace_fields_insert
        BEFORE INSERT ON jobs
        WHEN length(trim(NEW.idempotency_key)) = 0
          OR length(trim(NEW.provider)) = 0
          OR length(trim(NEW.model)) = 0
        BEGIN
            SELECT RAISE(ABORT, 'job idempotency_key, provider, and model must be non-empty');
        END;

        CREATE TRIGGER IF NOT EXISTS jobs_nonempty_trace_fields_update
        BEFORE UPDATE ON jobs
        WHEN length(trim(NEW.idempotency_key)) = 0
          OR length(trim(NEW.provider)) = 0
          OR length(trim(NEW.model)) = 0
        BEGIN
            SELECT RAISE(ABORT, 'job idempotency_key, provider, and model must be non-empty');
        END;
        "#,
    )?;
    Ok(())
}

fn ensure_job_contract_triggers(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TRIGGER IF NOT EXISTS jobs_provider_id_for_submitted_insert
        BEFORE INSERT ON jobs
        WHEN NEW.status IN (
            'submitted', 'provider_running', 'provider_succeeded', 'settled'
        ) AND (NEW.provider_job_id IS NULL OR length(trim(NEW.provider_job_id)) = 0)
        BEGIN
            SELECT RAISE(ABORT, 'provider_job_id required for submitted provider job status');
        END;

        CREATE TRIGGER IF NOT EXISTS jobs_provider_id_for_submitted_update
        BEFORE UPDATE ON jobs
        WHEN NEW.status IN (
            'submitted', 'provider_running', 'provider_succeeded', 'settled'
        ) AND (NEW.provider_job_id IS NULL OR length(trim(NEW.provider_job_id)) = 0)
        BEGIN
            SELECT RAISE(ABORT, 'provider_job_id required for submitted provider job status');
        END;

        CREATE TRIGGER IF NOT EXISTS jobs_failure_reason_for_failed_insert
        BEFORE INSERT ON jobs
        WHEN NEW.status IN ('failed', 'submit_failed')
             AND (NEW.failure_reason IS NULL OR length(trim(NEW.failure_reason)) = 0)
        BEGIN
            SELECT RAISE(ABORT, 'failure_reason required for failed job status');
        END;

        CREATE TRIGGER IF NOT EXISTS jobs_failure_reason_for_failed_update
        BEFORE UPDATE ON jobs
        WHEN NEW.status IN ('failed', 'submit_failed')
             AND (NEW.failure_reason IS NULL OR length(trim(NEW.failure_reason)) = 0)
        BEGIN
            SELECT RAISE(ABORT, 'failure_reason required for failed job status');
        END;

        CREATE TRIGGER IF NOT EXISTS jobs_budget_settled_for_terminal_insert
        BEFORE INSERT ON jobs
        WHEN NEW.status IN ('failed', 'submit_failed', 'settled') AND NEW.budget_settled <> 1
        BEGIN
            SELECT RAISE(ABORT, 'budget_settled required for terminal job status');
        END;

        CREATE TRIGGER IF NOT EXISTS jobs_budget_settled_for_terminal_update
        BEFORE UPDATE ON jobs
        WHEN NEW.status IN ('failed', 'submit_failed', 'settled') AND NEW.budget_settled <> 1
        BEGIN
            SELECT RAISE(ABORT, 'budget_settled required for terminal job status');
        END;
        "#,
    )?;
    Ok(())
}

fn ensure_validation_record_contract_triggers(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TRIGGER IF NOT EXISTS validation_records_contract_insert
        BEFORE INSERT ON validation_records
        WHEN length(trim(NEW.id)) = 0
          OR length(trim(NEW.artifact_id)) = 0
          OR length(trim(NEW.stage)) = 0
          OR length(trim(NEW.gate_version)) = 0
          OR NEW.confidence IS NULL
          OR NEW.layers_json IS NULL
          OR (json_valid(NEW.layers_json) AND json_type(NEW.layers_json) = 'null')
        BEGIN
            SELECT RAISE(ABORT, 'validation record id, artifact_id, stage, gate_version, confidence, and layers_json are required');
        END;

        CREATE TRIGGER IF NOT EXISTS validation_records_contract_update
        BEFORE UPDATE ON validation_records
        WHEN length(trim(NEW.id)) = 0
          OR length(trim(NEW.artifact_id)) = 0
          OR length(trim(NEW.stage)) = 0
          OR length(trim(NEW.gate_version)) = 0
          OR NEW.confidence IS NULL
          OR NEW.layers_json IS NULL
          OR (json_valid(NEW.layers_json) AND json_type(NEW.layers_json) = 'null')
        BEGIN
            SELECT RAISE(ABORT, 'validation record id, artifact_id, stage, gate_version, confidence, and layers_json are required');
        END;
        "#,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::temp_db_path;

    #[test]
    fn schema_creates_tables_wal_and_json1() {
        let path = temp_db_path("schema");
        let conn = open_db(&path).expect("open db");

        let mode: String = conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .expect("journal mode");
        assert_eq!(mode.to_lowercase(), "wal");

        require_json1(&conn).expect("JSON1 is available");

        let table_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type='table'
                   AND name IN (
                    'projects','chapters','scenes','shots','artifacts','jobs',
                    'validation_records','golden_cases','gate_metrics','model_metrics'
                   )",
                [],
                |row| row.get(0),
            )
            .expect("count tables");
        assert_eq!(table_count, 10);

        let _: i64 = conn
            .query_row(
                "SELECT json_extract('{\"plan\":{\"seconds\":3}}', '$.plan.seconds')",
                [],
                |row| row.get(0),
            )
            .expect("json_extract works");

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn schema_rejects_invalid_json_columns() {
        let path = temp_db_path("schema-json-valid");
        let db = Database::open(&path).expect("open db");
        db.create_project("p", 100.0).unwrap();
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
        db.create_artifact("a", Some("shot"), "video", "tos://a", "hash", "{}")
            .unwrap();

        for (label, sql) in [
            (
                "projects.brief_json",
                "INSERT INTO projects(id, brief_json, status, budget_total)
                 VALUES('bad-project', '{bad', 'new', 1.0)",
            ),
            (
                "chapters.plan_json",
                "INSERT INTO chapters(id, project_id, plan_json, status)
                 VALUES('bad-chapter', 'p', '{bad', 'new')",
            ),
            (
                "scenes.plan_json",
                "INSERT INTO scenes(id, chapter_id, plan_json, status)
                 VALUES('bad-scene', 'c', '{bad', 'new')",
            ),
            (
                "shots.plan_json",
                "INSERT INTO shots(id, scene_id, plan_json, status)
                 VALUES('bad-shot', 's', '{bad', 'new')",
            ),
            (
                "artifacts.params_json",
                "INSERT INTO artifacts(id, shot_id, kind, tos_key, content_hash, params_json)
                 VALUES('bad-artifact', 'shot', 'video', 'tos://bad', 'hash', '{bad')",
            ),
            (
                "jobs.params_json",
                "INSERT INTO jobs(
                    id, shot_id, attempt, idempotency_key, provider, model, status,
                    provider_job_id, params_json
                 )
                 VALUES(
                    'bad-job', 'shot', 0, 'bad-key', 'ark', 'seedance',
                    'submitted', 'ark-bad-job', '{bad'
                 )",
            ),
            (
                "validation_records.layers_json",
                "INSERT INTO validation_records(
                    id, artifact_id, stage, gate_version, verdict, confidence, layers_json
                 )
                 VALUES('bad-validation', 'a', 'l0', 'v1', 'pass', 1.0, '{bad')",
            ),
        ] {
            let result = db.connection().execute(sql, []);
            assert!(result.is_err(), "{label} should reject invalid JSON");
        }

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn schema_rejects_invalid_p1_contract_values() {
        let path = temp_db_path("schema-p1-contract-values");
        let db = Database::open(&path).expect("open db");
        db.create_project("p", 100.0).unwrap();
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
        db.create_artifact("a", Some("shot"), "video", "tos://a", "hash", "{}")
            .unwrap();

        for (label, sql) in [
            (
                "shots.tier",
                "INSERT INTO shots(
                    id, scene_id, plan_json, status, is_hero, tier
                 )
                 VALUES('bad-tier', 's', '{}', 'new', 0, 'premium')",
            ),
            (
                "jobs.status",
                "INSERT INTO jobs(
                    id, shot_id, attempt, idempotency_key, provider, model, status
                 )
                 VALUES('bad-job-status', 'shot', 0, 'bad-status-key', 'ark', 'seedance', 'done')",
            ),
            (
                "validation_records.escalate_reason",
                "INSERT INTO validation_records(
                    id, artifact_id, stage, gate_version, verdict, confidence, layers_json
                 )
                 VALUES('bad-escalate', 'a', 'l0', 'v1', 'escalate', 1.0, '{}')",
            ),
            (
                "validation_records.aesthetic_score",
                "INSERT INTO validation_records(
                    id, artifact_id, stage, gate_version, verdict, confidence, aesthetic_score, layers_json
                 )
                 VALUES('bad-aesthetic', 'a', 'l0', 'v1', 'pass', 1.0, 1.5, '{}')",
            ),
            (
                "validation_records.id",
                "INSERT INTO validation_records(
                    id, artifact_id, stage, gate_version, verdict, confidence, layers_json
                 )
                 VALUES('   ', 'a', 'l0', 'v1', 'pass', 1.0, '{}')",
            ),
            (
                "validation_records.stage",
                "INSERT INTO validation_records(
                    id, artifact_id, stage, gate_version, verdict, confidence, layers_json
                 )
                 VALUES('bad-stage', 'a', '   ', 'v1', 'pass', 1.0, '{}')",
            ),
            (
                "validation_records.gate_version",
                "INSERT INTO validation_records(
                    id, artifact_id, stage, gate_version, verdict, confidence, layers_json
                 )
                 VALUES('bad-gate', 'a', 'l0', '   ', 'pass', 1.0, '{}')",
            ),
            (
                "validation_records.confidence",
                "INSERT INTO validation_records(
                    id, artifact_id, stage, gate_version, verdict, layers_json
                 )
                 VALUES('bad-confidence', 'a', 'l0', 'v1', 'pass', '{}')",
            ),
            (
                "validation_records.layers_json_null",
                "INSERT INTO validation_records(
                    id, artifact_id, stage, gate_version, verdict, confidence, layers_json
                 )
                 VALUES('bad-null-layers', 'a', 'l0', 'v1', 'pass', 1.0, 'null')",
            ),
            (
                "gate_metrics.pass_precision",
                "INSERT INTO gate_metrics(
                    stage, gate_version, pass_precision, escalate_rate, judge_cost, human_agreement
                 )
                 VALUES('l0', 'bad-precision', 1.25, 0.1, 0.0, 1.0)",
            ),
            (
                "gate_metrics.judge_cost",
                "INSERT INTO gate_metrics(
                    stage, gate_version, pass_precision, escalate_rate, judge_cost, human_agreement
                 )
                 VALUES('l0', 'bad-cost', 0.99, 0.1, -0.01, 1.0)",
            ),
            (
                "model_metrics.pass_rate",
                "INSERT INTO model_metrics(provider, model, task, pass_rate, avg_cost, avg_latency)
                 VALUES('ark', 'seedance', 'video', -0.1, 1.0, 10.0)",
            ),
            (
                "model_metrics.avg_latency",
                "INSERT INTO model_metrics(provider, model, task, pass_rate, avg_cost, avg_latency)
                 VALUES('ark', 'seedance', 'video-negative-latency', 0.9, 1.0, -1.0)",
            ),
        ] {
            let result = db.connection().execute(sql, []);
            assert!(result.is_err(), "{label} should reject invalid value");
        }

        db.connection()
            .execute(
                "INSERT INTO validation_records(
                    id, artifact_id, stage, gate_version, verdict, confidence,
                    aesthetic_score, layers_json, escalate_reason
                 )
                 VALUES(
                    'valid-escalate', 'a', 'l0', 'v1', 'escalate',
                    0.25, 0.0, '{}', 'low_confidence_high_risk'
                 )",
                [],
            )
            .unwrap();

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn schema_rejects_artifacts_without_exactly_one_owner() {
        let path = temp_db_path("schema-artifact-owner");
        let db = Database::open(&path).expect("open db");
        db.create_project("p", 100.0).unwrap();
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

        db.create_artifact(
            "shot-artifact",
            Some("shot"),
            "video",
            "tos://shot",
            "hash",
            "{}",
        )
        .unwrap();
        db.create_project_artifact(
            "project-artifact",
            "p",
            "rough_cut",
            "tos://rough",
            "hash",
            "{}",
        )
        .unwrap();

        for (label, sql) in [
            (
                "missing owner",
                "INSERT INTO artifacts(id, kind, tos_key, content_hash, params_json)
                 VALUES('orphan', 'video', 'tos://orphan', 'hash', '{}')",
            ),
            (
                "two owners",
                "INSERT INTO artifacts(
                    id, project_id, shot_id, kind, tos_key, content_hash, params_json
                 )
                 VALUES('ambiguous', 'p', 'shot', 'video', 'tos://ambiguous', 'hash', '{}')",
            ),
        ] {
            let result = db.connection().execute(sql, []);
            assert!(result.is_err(), "{label} artifact should be rejected");
        }

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn schema_rejects_empty_trace_identity_fields() {
        let path = temp_db_path("schema-empty-trace-fields");
        let db = Database::open(&path).expect("open db");
        db.create_project("p", 100.0).unwrap();
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
        db.create_artifact("a", Some("shot"), "video", "tos://a", "hash", "{}")
            .unwrap();

        for (label, sql) in [
            (
                "artifact kind",
                "INSERT INTO artifacts(id, shot_id, kind, tos_key, content_hash, params_json)
                 VALUES('bad-artifact-kind', 'shot', '   ', 'tos://bad', 'hash', '{}')",
            ),
            (
                "artifact tos_key",
                "INSERT INTO artifacts(id, shot_id, kind, tos_key, content_hash, params_json)
                 VALUES('bad-artifact-tos', 'shot', 'video', '   ', 'hash', '{}')",
            ),
            (
                "artifact content_hash",
                "INSERT INTO artifacts(id, shot_id, kind, tos_key, content_hash, params_json)
                 VALUES('bad-artifact-hash', 'shot', 'video', 'tos://bad', '   ', '{}')",
            ),
            (
                "job idempotency_key",
                "INSERT INTO jobs(id, shot_id, attempt, idempotency_key, provider, model, status)
                 VALUES('bad-job-key', 'shot', 0, '   ', 'ark', 'seedance', 'reserved')",
            ),
            (
                "job provider",
                "INSERT INTO jobs(id, shot_id, attempt, idempotency_key, provider, model, status)
                 VALUES('bad-job-provider', 'shot', 0, 'bad-provider-key', '   ', 'seedance', 'reserved')",
            ),
            (
                "job model",
                "INSERT INTO jobs(id, shot_id, attempt, idempotency_key, provider, model, status)
                 VALUES('bad-job-model', 'shot', 0, 'bad-model-key', 'ark', '   ', 'reserved')",
            ),
        ] {
            let result = db.connection().execute(sql, []);
            assert!(result.is_err(), "{label} should reject blank text");
        }

        for (label, sql) in [
            (
                "artifact update",
                "UPDATE artifacts SET tos_key='   ' WHERE id='a'",
            ),
            (
                "job update",
                "INSERT INTO jobs(id, shot_id, attempt, idempotency_key, provider, model, status)
                 VALUES('good-job', 'shot', 0, 'good-key', 'ark', 'seedance', 'reserved')",
            ),
        ] {
            if label == "job update" {
                db.connection().execute(sql, []).unwrap();
                let result = db
                    .connection()
                    .execute("UPDATE jobs SET model='   ' WHERE id='good-job'", []);
                assert!(result.is_err(), "{label} should reject blank text");
            } else {
                let result = db.connection().execute(sql, []);
                assert!(result.is_err(), "{label} should reject blank text");
            }
        }

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn schema_rejects_invalid_job_trace_contract_values() {
        let path = temp_db_path("schema-job-trace-contract");
        let db = Database::open(&path).expect("open db");
        db.create_project("p", 100.0).unwrap();
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

        db.connection()
            .execute(
                "INSERT INTO jobs(
                    id, shot_id, attempt, idempotency_key, provider, model,
                    status, provider_job_id
                 )
                 VALUES(
                    'valid-submitted', 'shot', 0, 'valid-submitted-key',
                    'ark', 'seedance', 'submitted', 'ark-job-1'
                 )",
                [],
            )
            .unwrap();
        db.connection()
            .execute(
                "INSERT INTO jobs(
                    id, shot_id, attempt, idempotency_key, provider, model,
                    status, failure_reason, budget_settled
                 )
                 VALUES(
                    'valid-failed', 'shot', 1, 'valid-failed-key',
                    'ark', 'seedance', 'failed', 'provider failed', 1
                 )",
                [],
            )
            .unwrap();
        db.connection()
            .execute(
                "INSERT INTO jobs(
                    id, shot_id, attempt, idempotency_key, provider, model,
                    status, provider_job_id, budget_settled
                 )
                 VALUES(
                    'valid-settled', 'shot', 2, 'valid-settled-key',
                    'ark', 'seedance', 'settled', 'ark-job-2', 1
                 )",
                [],
            )
            .unwrap();

        for (label, sql) in [
            (
                "submitted without provider_job_id",
                "INSERT INTO jobs(
                    id, shot_id, attempt, idempotency_key, provider, model, status
                 )
                 VALUES(
                    'bad-submitted', 'shot', 10, 'bad-submitted-key',
                    'ark', 'seedance', 'submitted'
                 )",
            ),
            (
                "provider_running with blank provider_job_id",
                "INSERT INTO jobs(
                    id, shot_id, attempt, idempotency_key, provider, model,
                    status, provider_job_id
                 )
                 VALUES(
                    'bad-running', 'shot', 11, 'bad-running-key',
                    'ark', 'seedance', 'provider_running', '   '
                 )",
            ),
            (
                "failed without failure_reason",
                "INSERT INTO jobs(
                    id, shot_id, attempt, idempotency_key, provider, model,
                    status, budget_settled
                 )
                 VALUES(
                    'bad-failed-reason', 'shot', 12, 'bad-failed-reason-key',
                    'ark', 'seedance', 'failed', 1
                 )",
            ),
            (
                "submit_failed with blank failure_reason",
                "INSERT INTO jobs(
                    id, shot_id, attempt, idempotency_key, provider, model,
                    status, failure_reason, budget_settled
                 )
                 VALUES(
                    'bad-submit-failed-reason', 'shot', 13,
                    'bad-submit-failed-reason-key',
                    'ark', 'seedance', 'submit_failed', '   ', 1
                 )",
            ),
            (
                "failed without budget_settled",
                "INSERT INTO jobs(
                    id, shot_id, attempt, idempotency_key, provider, model,
                    status, failure_reason
                 )
                 VALUES(
                    'bad-failed-settled', 'shot', 14, 'bad-failed-settled-key',
                    'ark', 'seedance', 'failed', 'provider failed'
                 )",
            ),
            (
                "settled without budget_settled",
                "INSERT INTO jobs(
                    id, shot_id, attempt, idempotency_key, provider, model,
                    status, provider_job_id
                 )
                 VALUES(
                    'bad-settled-settled', 'shot', 15, 'bad-settled-settled-key',
                    'ark', 'seedance', 'settled', 'ark-job-15'
                 )",
            ),
        ] {
            let result = db.connection().execute(sql, []);
            assert!(result.is_err(), "{label} job should be rejected");
        }

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn schema_rejects_duplicate_pass_for_same_artifact_stage() {
        let path = temp_db_path("schema-validation-pass-once");
        let db = Database::open(&path).expect("open db");
        db.create_project("p", 100.0).unwrap();
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
        db.create_artifact("a1", Some("shot"), "video", "tos://a1", "hash1", "{}")
            .unwrap();
        db.create_artifact("a2", Some("shot"), "video", "tos://a2", "hash2", "{}")
            .unwrap();

        db.connection()
            .execute(
                "INSERT INTO validation_records(
                    id, artifact_id, stage, gate_version, verdict, confidence, layers_json
                 )
                 VALUES('v-repair', 'a1', 'l0', 'v1', 'repair', 1.0, '{}')",
                [],
            )
            .unwrap();
        db.connection()
            .execute(
                "INSERT INTO validation_records(
                    id, artifact_id, stage, gate_version, verdict, confidence, layers_json
                 )
                 VALUES('v-pass', 'a1', 'l0', 'v1', 'pass', 1.0, '{}')",
                [],
            )
            .unwrap();
        db.connection()
            .execute(
                "INSERT INTO validation_records(
                    id, artifact_id, stage, gate_version, verdict, confidence, layers_json
                 )
                 VALUES('v-other-stage', 'a1', 'media_l0', 'v1', 'pass', 1.0, '{}')",
                [],
            )
            .unwrap();
        db.connection()
            .execute(
                "INSERT INTO validation_records(
                    id, artifact_id, stage, gate_version, verdict, confidence, layers_json
                 )
                 VALUES('v-other-artifact', 'a2', 'l0', 'v1', 'pass', 1.0, '{}')",
                [],
            )
            .unwrap();

        let err = db
            .connection()
            .execute(
                "INSERT INTO validation_records(
                    id, artifact_id, stage, gate_version, verdict, confidence, layers_json
                 )
                 VALUES('v-duplicate-pass', 'a1', 'l0', 'v2', 'pass', 1.0, '{}')",
                [],
            )
            .expect_err("UNIQUE should reject duplicate pass validation");
        assert!(matches!(
            err,
            rusqlite::Error::SqliteFailure(_, Some(_)) | rusqlite::Error::SqliteFailure(_, None)
        ));

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn duplicate_idempotency_key_is_rejected() {
        let path = temp_db_path("job-unique");
        let db = Database::open(&path).expect("open db");
        db.create_project("p", 100.0).unwrap();
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

        db.connection()
            .execute(
                "INSERT INTO jobs(
                    id, shot_id, attempt, idempotency_key, provider, model, status,
                    provider_job_id
                 )
                 VALUES(
                    'j1', 'shot', 0, 'same-key', 'ark', 'seedance',
                    'submitted', 'ark-job-1'
                 )",
                [],
            )
            .unwrap();

        let err = db
            .connection()
            .execute(
                "INSERT INTO jobs(
                    id, shot_id, attempt, idempotency_key, provider, model, status,
                    provider_job_id
                 )
                 VALUES(
                    'j2', 'shot', 1, 'same-key', 'ark', 'seedance',
                    'submitted', 'ark-job-2'
                 )",
                [],
            )
            .expect_err("UNIQUE should reject duplicate idempotency key");
        assert!(matches!(
            err,
            rusqlite::Error::SqliteFailure(_, Some(_)) | rusqlite::Error::SqliteFailure(_, None)
        ));

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn schema_supports_excellence_fields_from_p1() {
        let path = temp_db_path("schema-excellence-fields");
        let db = Database::open(&path).expect("open db");
        db.create_project("p", 100.0).unwrap();
        db.create_chapter("c", "p", "{}").unwrap();
        db.create_scene("s", "c", "{}").unwrap();
        db.create_shot("shot", "s", "{\"duration_s\":1}", None, None, true, "hero")
            .unwrap();
        db.create_artifact("a", Some("shot"), "video", "tos://a", "hash", "{}")
            .unwrap();

        db.connection()
            .execute(
                "INSERT INTO jobs(
                    id, shot_id, attempt, idempotency_key, provider, model, status,
                    provider_job_id, budget_settled, candidate_set, is_chosen
                 )
                 VALUES(
                    'j1', 'shot', 0, 'key-1', 'ark', 'seedance', 'settled',
                    'ark-job-1', 1, 'candidate-set-1', 1
                 )",
                [],
            )
            .unwrap();
        db.connection()
            .execute(
                "INSERT INTO validation_records(
                    id, artifact_id, stage, gate_version, verdict, confidence,
                    aesthetic_score, layers_json
                 )
                 VALUES('v1', 'a', 'l0', 'v1', 'pass', 1.0, 0.875, '{}')",
                [],
            )
            .unwrap();
        db.connection()
            .execute(
                "INSERT INTO golden_cases(
                    id, stage, failure_type, tos_key, human_verdict, human_score,
                    is_exemplar, source
                 )
                 VALUES(
                    'g1', 'l0', 'none', 'tos://golden', 'pass', 0.95, 1,
                    'schema-test'
                 )",
                [],
            )
            .unwrap();

        let (is_hero, tier): (i64, String) = db
            .connection()
            .query_row(
                "SELECT is_hero, tier FROM shots WHERE id='shot'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(is_hero, 1);
        assert_eq!(tier, "hero");

        let (candidate_set, is_chosen): (String, i64) = db
            .connection()
            .query_row(
                "SELECT candidate_set, is_chosen FROM jobs WHERE id='j1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(candidate_set, "candidate-set-1");
        assert_eq!(is_chosen, 1);

        let aesthetic_score: f64 = db
            .connection()
            .query_row(
                "SELECT aesthetic_score FROM validation_records WHERE id='v1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(aesthetic_score, 0.875);

        let is_exemplar: i64 = db
            .connection()
            .query_row(
                "SELECT is_exemplar FROM golden_cases WHERE id='g1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(is_exemplar, 1);

        let _ = std::fs::remove_file(path);
    }
}
