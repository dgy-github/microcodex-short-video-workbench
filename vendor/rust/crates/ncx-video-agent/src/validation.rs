use rusqlite::params;
use serde_json::Value;

use crate::{Result, VideoAgentError};

#[derive(Debug, Clone)]
pub struct ValidationInput {
    pub id: String,
    pub artifact_id: String,
    pub stage: String,
    pub gate_version: String,
    pub verdict: String,
    pub confidence: Option<f64>,
    pub aesthetic_score: Option<f64>,
    pub layers_json: Value,
    pub escalate_reason: Option<String>,
}

pub fn record_validation(conn: &rusqlite::Connection, input: &ValidationInput) -> Result<()> {
    validate_input(input)?;
    conn.execute(
        "INSERT INTO validation_records(
            id, artifact_id, stage, gate_version, verdict, confidence,
            aesthetic_score, layers_json, escalate_reason
         )
         VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            input.id,
            input.artifact_id,
            input.stage,
            input.gate_version,
            input.verdict,
            input.confidence,
            input.aesthetic_score,
            input.layers_json.to_string(),
            input.escalate_reason,
        ],
    )?;
    Ok(())
}

fn validate_input(input: &ValidationInput) -> Result<()> {
    require_nonempty("id", &input.id)?;
    require_nonempty("artifact_id", &input.artifact_id)?;
    require_nonempty("stage", &input.stage)?;
    require_nonempty("gate_version", &input.gate_version)?;

    match input.verdict.as_str() {
        "pass" | "repair" | "escalate" => {}
        verdict => {
            return Err(VideoAgentError::ValidationRecord(format!(
                "verdict must be pass, repair, or escalate; got {verdict}"
            )));
        }
    }

    let confidence = input
        .confidence
        .ok_or_else(|| VideoAgentError::ValidationRecord("confidence is required".to_string()))?;
    require_unit_interval("confidence", confidence)?;
    if let Some(aesthetic_score) = input.aesthetic_score {
        require_unit_interval("aesthetic_score", aesthetic_score)?;
    }
    if input.layers_json.is_null() {
        return Err(VideoAgentError::ValidationRecord(
            "layers_json must not be null".to_string(),
        ));
    }
    if input.verdict == "escalate" {
        let reason = input.escalate_reason.as_deref().unwrap_or_default();
        require_nonempty("escalate_reason", reason)?;
    }
    Ok(())
}

fn require_nonempty(label: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(VideoAgentError::ValidationRecord(format!(
            "{label} must not be empty"
        )));
    }
    Ok(())
}

fn require_unit_interval(label: &str, value: f64) -> Result<()> {
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return Err(VideoAgentError::ValidationRecord(format!(
            "{label} must be finite and in [0, 1], got {value}"
        )));
    }
    Ok(())
}

pub fn assert_artifacts_passed(conn: &rusqlite::Connection, artifact_ids: &[&str]) -> Result<()> {
    for artifact_id in artifact_ids {
        let pass_count: i64 = conn.query_row(
            "SELECT COUNT(*)
             FROM validation_records
             WHERE artifact_id=?1 AND verdict='pass'",
            params![artifact_id],
            |row| row.get(0),
        )?;
        if pass_count == 0 {
            return Err(VideoAgentError::MissingPassingValidation {
                artifact_id: (*artifact_id).to_string(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::*;
    use crate::db::Database;
    use crate::test_support::temp_db_path;

    fn db_with_artifact(name: &str) -> (std::path::PathBuf, Database) {
        let path = temp_db_path(name);
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
        db.create_artifact("a", Some("shot"), "brief", "tos://a", "hash", "{}")
            .unwrap();
        (path, db)
    }

    #[test]
    fn downstream_contract_rejects_missing_and_non_pass_records() {
        let (path, db) = db_with_artifact("contract");

        assert!(matches!(
            assert_artifacts_passed(db.connection(), &["a"]),
            Err(VideoAgentError::MissingPassingValidation { .. })
        ));

        record_validation(
            db.connection(),
            &ValidationInput {
                id: "v1".to_string(),
                artifact_id: "a".to_string(),
                stage: "l0".to_string(),
                gate_version: "v1".to_string(),
                verdict: "repair".to_string(),
                confidence: Some(1.0),
                aesthetic_score: None,
                layers_json: json!({"reason": "bad_reference"}),
                escalate_reason: None,
            },
        )
        .unwrap();
        assert!(matches!(
            assert_artifacts_passed(db.connection(), &["a"]),
            Err(VideoAgentError::MissingPassingValidation { .. })
        ));

        record_validation(
            db.connection(),
            &ValidationInput {
                id: "v2".to_string(),
                artifact_id: "a".to_string(),
                stage: "l0".to_string(),
                gate_version: "v1".to_string(),
                verdict: "pass".to_string(),
                confidence: Some(1.0),
                aesthetic_score: None,
                layers_json: json!({}),
                escalate_reason: None,
            },
        )
        .unwrap();
        assert_artifacts_passed(db.connection(), &["a"]).unwrap();

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn record_validation_rejects_invalid_contract_inputs_before_insert() {
        let (path, db) = db_with_artifact("validation-input-contract");

        let valid = ValidationInput {
            id: "v-valid".to_string(),
            artifact_id: "a".to_string(),
            stage: "l0".to_string(),
            gate_version: "v1".to_string(),
            verdict: "pass".to_string(),
            confidence: Some(1.0),
            aesthetic_score: None,
            layers_json: json!({}),
            escalate_reason: None,
        };

        for (label, input) in [
            (
                "empty id",
                ValidationInput {
                    id: " ".to_string(),
                    ..valid.clone()
                },
            ),
            (
                "unknown verdict",
                ValidationInput {
                    id: "v-bad-verdict".to_string(),
                    verdict: "skip".to_string(),
                    ..valid.clone()
                },
            ),
            (
                "missing confidence",
                ValidationInput {
                    id: "v-missing-confidence".to_string(),
                    confidence: None,
                    ..valid.clone()
                },
            ),
            (
                "nan confidence",
                ValidationInput {
                    id: "v-nan-confidence".to_string(),
                    confidence: Some(f64::NAN),
                    ..valid.clone()
                },
            ),
            (
                "invalid aesthetic score",
                ValidationInput {
                    id: "v-bad-aesthetic".to_string(),
                    aesthetic_score: Some(1.5),
                    ..valid.clone()
                },
            ),
            (
                "null layers",
                ValidationInput {
                    id: "v-null-layers".to_string(),
                    layers_json: Value::Null,
                    ..valid.clone()
                },
            ),
            (
                "empty escalate reason",
                ValidationInput {
                    id: "v-empty-escalate".to_string(),
                    verdict: "escalate".to_string(),
                    escalate_reason: Some("  ".to_string()),
                    ..valid.clone()
                },
            ),
        ] {
            let err = match record_validation(db.connection(), &input) {
                Ok(()) => panic!("{label} should be rejected"),
                Err(err) => err,
            };
            assert!(
                matches!(err, VideoAgentError::ValidationRecord(_)),
                "{label} returned unexpected error: {err}"
            );
        }

        let count: i64 = db
            .connection()
            .query_row("SELECT COUNT(*) FROM validation_records", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0);

        let _ = std::fs::remove_file(path);
    }
}
