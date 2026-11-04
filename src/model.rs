use serde::{Deserialize, Serialize};

/// A copied/highlighted occurrence on one logical line (soft-wrapped rows joined).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchSpan {
    /// Zero-based logical line index.
    pub line: usize,
    /// UTF-8 byte offset where the highlighted substring starts.
    pub start: usize,
    /// UTF-8 byte offset immediately after the highlighted substring.
    pub end: usize,
    /// Value picked: the `match` capture, or an OSC 8 target.
    pub text: String,
    /// Pattern name that produced this occurrence.
    pub pattern: String,
    /// Match precedence where lower numbers are higher priority.
    pub priority: u16,
}

impl MatchSpan {
    pub fn len_bytes(&self) -> usize {
        self.end.saturating_sub(self.start)
    }

    pub fn overlaps(&self, other: &Self) -> bool {
        self.line == other.line && self.start < other.end && other.start < self.end
    }
}

/// A unique picked value and every visible occurrence of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HintAssignment {
    pub hint: String,
    pub text: String,
    pub occurrences: Vec<MatchSpan>,
}

/// A regex pattern from user or project configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatternSpec {
    pub name: String,
    pub regex: String,
    pub priority: u16,
}

/// What the key that started pluck asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Copy,
    Open,
}

impl Action {
    pub fn flipped(self) -> Self {
        match self {
            Action::Copy => Action::Open,
            Action::Open => Action::Copy,
        }
    }
}
