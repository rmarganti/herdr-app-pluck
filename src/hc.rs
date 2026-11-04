//! The bits of herdr-core (`hc`) the picker uses.

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

static HC: OnceLock<PathBuf> = OnceLock::new();

/// Uses this `hc` binary (the mod passes the one it found).
pub fn set_program(path: PathBuf) {
    let _ = HC.set(path);
}

/// The `hc` to run. A terminal started by herdr-core with an explicit command gets the
/// core's environment, whose PATH may lack `~/.local/bin`, so look in the usual places.
pub fn program() -> PathBuf {
    HC.get_or_init(|| {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        find_program(std::env::var_os("PATH").as_deref(), home.as_deref())
    })
    .clone()
}

fn find_program(path: Option<&std::ffi::OsStr>, home: Option<&Path>) -> PathBuf {
    let on_path = path
        .into_iter()
        .flat_map(std::env::split_paths)
        .map(|dir| dir.join("hc"));
    let usual = home
        .map(|home| home.join(".local/bin/hc"))
        .into_iter()
        .chain(["/opt/homebrew/bin/hc", "/usr/local/bin/hc"].map(PathBuf::from));
    on_path
        .chain(usual)
        .find(|candidate| candidate.is_file())
        .unwrap_or_else(|| PathBuf::from("hc"))
}

/// The facts about a herdr-core terminal the picker needs (`hc get --json`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TerminalFacts {
    pub cols: u16,
    pub rows: u16,
    pub cwd: Option<String>,
    #[serde(default)]
    pub foreground: Option<Foreground>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Foreground {
    pub cwd: Option<String>,
}

impl TerminalFacts {
    /// Where the program in the terminal is now, else where the terminal started.
    pub fn live_cwd(&self) -> Option<&str> {
        self.foreground
            .as_ref()
            .and_then(|foreground| foreground.cwd.as_deref())
            .or(self.cwd.as_deref())
    }
}

pub fn terminal_facts(terminal: &str) -> Result<TerminalFacts> {
    let output = Command::new(program())
        .args(["get", "--json", terminal])
        .output()
        .context("failed to run hc; is it on PATH?")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("hc get {terminal} failed: {}", stderr.trim());
    }
    serde_json::from_slice(&output.stdout)
        .with_context(|| format!("unexpected hc get output for {terminal}"))
}

/// The terminal's visible screen as logical lines (soft wraps joined), from `hc read`.
pub fn read_screen_lines(terminal: &str) -> Result<Vec<String>> {
    #[derive(Deserialize)]
    struct Read {
        text: String,
    }
    let output = Command::new(program())
        .args(["read", "--json", "--lines", "0", terminal])
        .output()
        .context("failed to run hc read")?;
    if !output.status.success() {
        bail!(
            "hc read {terminal} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let read: Read = serde_json::from_slice(&output.stdout).context("unexpected hc read output")?;
    Ok(read.text.split('\n').map(str::to_string).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_facts_prefer_foreground_cwd() {
        let facts: TerminalFacts = serde_json::from_str(
            r#"{"address":"t1","cols":95,"rows":18,"cwd":"/start",
                "foreground":{"argv":["nvim"],"cwd":"/now","name":"nvim","pid":1}}"#,
        )
        .unwrap();
        assert_eq!((facts.cols, facts.rows), (95, 18));
        assert_eq!(facts.live_cwd(), Some("/now"));
    }

    #[test]
    fn finds_hc_outside_path_in_local_bin() {
        let home = std::env::temp_dir().join(format!("hap-hc-{}", std::process::id()));
        std::fs::create_dir_all(home.join(".local/bin")).unwrap();
        std::fs::write(home.join(".local/bin/hc"), "").unwrap();
        assert_eq!(
            find_program(Some(std::ffi::OsStr::new("/nonexistent")), Some(&home)),
            home.join(".local/bin/hc")
        );
        std::fs::remove_file(home.join(".local/bin/hc")).unwrap();
    }

    #[test]
    fn terminal_facts_fall_back_to_start_cwd() {
        let facts: TerminalFacts =
            serde_json::from_str(r#"{"cols":80,"rows":24,"cwd":"/start","foreground":null}"#)
                .unwrap();
        assert_eq!(facts.live_cwd(), Some("/start"));
    }
}
