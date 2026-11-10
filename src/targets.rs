//! Finds what to hint on a decoded grid and assigns hints.

use crate::grid::{CellRef, Grid, LogicalLine};
use crate::hints::{assign_hints, HintAssignments};
use crate::model::{Action, MatchSpan};
use crate::patterns::{
    find_matches, find_openable_urls, is_openable_url, merge_links, CustomPatternDefinition,
    OSC8_PRIORITY,
};

/// Hints plus the screen lines they were found on.
#[derive(Debug, Clone)]
pub struct Targets {
    pub lines: Vec<LogicalLine>,
    pub assignments: HintAssignments,
}

impl Targets {
    pub fn cells(&self, span: &MatchSpan) -> Vec<CellRef> {
        self.lines[span.line].cells_for(span.start, span.end)
    }
}

pub fn find_targets(grid: &Grid, action: Action, custom: &[CustomPatternDefinition]) -> Targets {
    let lines = grid.logical_lines();
    let texts: Vec<String> = lines.iter().map(|line| line.text.clone()).collect();
    let links = link_spans(grid, &lines);
    let spans = match action {
        Action::Copy => merge_links(find_matches(&texts, custom), links),
        Action::Open => merge_links(
            find_openable_urls(&texts),
            links
                .into_iter()
                .filter(|link| is_openable_url(&link.text))
                .collect(),
        ),
    };

    // Hint letters are printed over a token's first cells, so those cells must be safe
    // to print over. Which hint width applies depends on how many survive, so filter for
    // one letter first and again for two if that's what the count calls for.
    let one = filter_drawable(grid, &lines, spans.clone(), 1);
    let mut assignments = assign_hints(one);
    if assignments.width() == Some(2) {
        assignments = assign_hints(filter_drawable(grid, &lines, spans, 2));
    }
    Targets { lines, assignments }
}

fn filter_drawable(
    grid: &Grid,
    lines: &[LogicalLine],
    spans: Vec<MatchSpan>,
    hint_width: usize,
) -> Vec<MatchSpan> {
    spans
        .into_iter()
        .filter(|span| {
            let cells = lines[span.line].cells_for(span.start, span.end);
            cells.len() >= hint_width
                && cells[..hint_width]
                    .iter()
                    .all(|cell| grid.cell(cell.row, cell.col).redrawable())
        })
        .collect()
}

/// OSC 8 hyperlinks: each run of cells with the same target is one span on its line.
fn link_spans(grid: &Grid, lines: &[LogicalLine]) -> Vec<MatchSpan> {
    let mut spans = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let mut run: Option<(usize, usize, &str)> = None;
        for cell_ref in &line.cells {
            let link = grid.cell(cell_ref.row, cell_ref.col).link.as_deref();
            match (&mut run, link) {
                (Some((_, end, target)), Some(link)) if *target == link => *end = cell_ref.byte_end,
                _ => {
                    if let Some((start, end, target)) = run.take() {
                        spans.push(link_span(index, start, end, target));
                    }
                    run = link.map(|link| (cell_ref.byte_start, cell_ref.byte_end, link));
                }
            }
        }
        if let Some((start, end, target)) = run {
            spans.push(link_span(index, start, end, target));
        }
    }
    spans
}

fn link_span(line: usize, start: usize, end: usize, target: &str) -> MatchSpan {
    MatchSpan {
        line,
        start,
        end,
        text: target.to_string(),
        pattern: "osc8".to_string(),
        priority: OSC8_PRIORITY,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::decode;

    fn texts(targets: &Targets) -> Vec<(String, String)> {
        targets
            .assignments
            .assignments()
            .iter()
            .map(|a| (a.hint.clone(), a.text.clone()))
            .collect()
    }

    #[test]
    fn copy_finds_tokens_in_visible_order() {
        let grid = decode(
            b"see src/main.rs and https://x.test/a\r\nsha 1a63023f",
            60,
            3,
        )
        .unwrap();
        let targets = find_targets(&grid, Action::Copy, &[]);
        assert_eq!(
            texts(&targets),
            vec![
                ("a".into(), "src/main.rs".into()),
                ("s".into(), "https://x.test/a".into()),
                ("d".into(), "1a63023f".into()),
            ]
        );
    }

    #[test]
    fn git_paths_exclude_terminal_padding() {
        for (prefix, pattern) in [
            ("modified:   ", "git-status"),
            ("new file:   ", "git-status"),
            ("deleted by us: ", "git-status"),
            ("--- a/", "diff"),
            ("+++ b/", "diff"),
        ] {
            for path in ["dots/.bash_aliases", "docs/my notes.md"] {
                let paint = format!("{prefix}{path}\t  ");
                let grid = decode(paint.as_bytes(), 100, 1).unwrap();
                let targets = find_targets(&grid, Action::Copy, &[]);
                assert_eq!(texts(&targets), vec![("a".into(), path.into())]);
                let span = &targets.assignments.assignments()[0].occurrences[0];
                assert_eq!(span.pattern, pattern);
                assert_eq!(span.start, prefix.len());
                assert_eq!(span.end, prefix.len() + path.len());
                assert_eq!(targets.cells(span).len(), path.len());
            }
        }
    }

    #[test]
    fn open_only_hints_openable_urls() {
        let grid = decode(b"src/main.rs https://x.test ssh://h/p file:///tmp/a", 60, 1).unwrap();
        let targets = find_targets(&grid, Action::Open, &[]);
        let values: Vec<String> = texts(&targets).into_iter().map(|t| t.1).collect();
        assert_eq!(values, vec!["https://x.test", "file:///tmp/a"]);
    }

    #[test]
    fn osc8_link_beats_patterns_and_keeps_its_hidden_target() {
        let grid = decode(
            b"\x1b]8;;https://github.com/x\x1b\\src/main.rs\x1b]8;;\x1b\\ 1234",
            40,
            1,
        )
        .unwrap();
        let targets = find_targets(&grid, Action::Copy, &[]);
        assert_eq!(
            texts(&targets),
            vec![
                ("a".into(), "https://github.com/x".into()),
                ("s".into(), "1234".into())
            ]
        );
        let span = &targets.assignments.assignments()[0].occurrences[0];
        assert_eq!(targets.cells(span).len(), "src/main.rs".len());
    }

    #[test]
    fn token_across_a_soft_wrap_is_one_match() {
        let grid = decode(b"go https://example.com/very/long", 12, 3).unwrap();
        let targets = find_targets(&grid, Action::Copy, &[]);
        assert_eq!(texts(&targets)[0].1, "https://example.com/very/long");
        let span = &targets.assignments.assignments()[0].occurrences[0];
        let rows: Vec<u16> = targets.cells(span).iter().map(|c| c.row).collect();
        assert_eq!(rows.first(), Some(&0));
        assert_eq!(rows.last(), Some(&2));
    }

    #[test]
    fn tokens_starting_on_unsafe_cells_get_no_hint() {
        // The match starts on a combining-mark cell: never print over it.
        let grid = decode("e\u{301}/tmp/x and /tmp/y".as_bytes(), 40, 1).unwrap();
        let targets = find_targets(&grid, Action::Copy, &[]);
        let values: Vec<String> = texts(&targets).into_iter().map(|t| t.1).collect();
        assert_eq!(values, vec!["/tmp/y"]);
    }

    #[test]
    fn positions_after_wide_clusters_follow_the_grid() {
        let grid = decode("👨\u{200d}👩\u{200d}👧 /tmp/x".as_bytes(), 20, 1).unwrap();
        let targets = find_targets(&grid, Action::Copy, &[]);
        let span = &targets.assignments.assignments()[0].occurrences[0];
        assert_eq!(targets.cells(span)[0].col, 3);
    }

    #[test]
    fn two_letter_hints_when_more_than_26_values() {
        let text: String = (0..30).map(|i| format!("10.0.0.{i}\r\n")).collect();
        let grid = decode(text.as_bytes(), 20, 31).unwrap();
        let targets = find_targets(&grid, Action::Copy, &[]);
        assert_eq!(targets.assignments.width(), Some(2));
        assert_eq!(targets.assignments.len(), 30);
    }
}
