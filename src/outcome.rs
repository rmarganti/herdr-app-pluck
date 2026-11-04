//! What the picker tells the mod, as files in the run directory.

use crate::model::Action;
use anyhow::{Context, Result};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Outcome {
    /// `action` is final: Shift already applied.
    Picked {
        action: Action,
        text: String,
        is_url: bool,
    },
    Cancelled {
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    NoMatches,
    Error {
        message: String,
    },
}

#[derive(Debug, Serialize)]
struct ResultFile<'a> {
    #[serde(flatten)]
    outcome: &'a Outcome,
    #[serde(skip_serializing_if = "<[String]>::is_empty")]
    warnings: &'a [String],
}

#[derive(Debug, Serialize)]
struct ReadyFile<'a> {
    hints: usize,
    #[serde(skip_serializing_if = "<[String]>::is_empty")]
    warnings: &'a [String],
}

/// The JSON written to `result.json`.
pub fn result_json(outcome: &Outcome, warnings: &[String]) -> String {
    serde_json::to_string(&ResultFile { outcome, warnings }).expect("result serializes")
}

pub fn write_result(run_dir: &Path, outcome: &Outcome, warnings: &[String]) -> Result<()> {
    write_atomic(
        &run_dir.join("result.json"),
        &result_json(outcome, warnings),
    )
}

/// Captured and matched: the mod may show the picker now.
pub fn write_ready(run_dir: &Path, hints: usize, warnings: &[String]) -> Result<()> {
    let json = serde_json::to_string(&ReadyFile { hints, warnings }).expect("ready serializes");
    write_atomic(&run_dir.join("ready"), &json)
}

/// Writes then renames, so the mod never reads half a file.
fn write_atomic(path: &Path, text: &str) -> Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, text).with_context(|| format!("failed to write {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("failed to write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn result_shapes_match_the_mod_contract() {
        let picked = Outcome::Picked {
            action: Action::Copy,
            text: "src/app/mod.rs".into(),
            is_url: false,
        };
        assert_eq!(
            result_json(&picked, &[]),
            r#"{"outcome":"picked","action":"copy","text":"src/app/mod.rs","is_url":false}"#
        );
        assert_eq!(
            result_json(&Outcome::Cancelled { message: None }, &[]),
            r#"{"outcome":"cancelled"}"#
        );
        assert_eq!(
            result_json(&Outcome::NoMatches, &["config.toml: bad".into()]),
            r#"{"outcome":"no_matches","warnings":["config.toml: bad"]}"#
        );
        assert_eq!(
            result_json(
                &Outcome::Error {
                    message: "x".into()
                },
                &[]
            ),
            r#"{"outcome":"error","message":"x"}"#
        );
    }

    #[test]
    fn files_are_written_whole() {
        let dir = std::env::temp_dir().join(format!("hap-outcome-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        write_ready(&dir, 3, &[]).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join("ready")).unwrap(),
            r#"{"hints":3}"#
        );
        write_result(&dir, &Outcome::NoMatches, &[]).unwrap();
        assert!(!dir.join("result.tmp").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
