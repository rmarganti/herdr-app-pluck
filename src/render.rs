//! Draws hints on top of the replayed paint.
//!
//! Only cells the decoder calls redrawable (one narrow, non-combining character) are
//! ever printed over, and each is reprinted with its own character, so positions can't
//! move. Hint letters replace a token's first cells, tmux-fingers style.

use crate::color::Rgb;
use crate::grid::Grid;
use crate::targets::Targets;
use std::fmt::Write as _;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HintStyle {
    pub hint_fg: Rgb,
    pub hint_bg: Rgb,
    pub match_fg: Rgb,
}

/// Puts the terminal into a state where `CUP` + printing ASCII lands exactly where asked
/// whatever the replayed program left behind: no origin mode, no margins, no insert
/// mode, no autowrap, ASCII in G0, no open hyperlink. None of it changes any cell.
const DRAW_PREAMBLE: &str = "\x1b[?6l\x1b[?69l\x1b[r\x1b[4l\x1b[?7l\x1b(B\x0f\x1b]8;;\x1b\\";

/// Bytes that draw every hint starting with `typed` and highlight its token.
pub fn draw_hints(grid: &Grid, targets: &Targets, typed: &str, style: HintStyle) -> String {
    let mut out = String::from(DRAW_PREAMBLE);
    let hint_sgr = format!("\x1b[0;1;{};{}m", style.hint_fg.fg(), style.hint_bg.bg());
    let typed_sgr = format!("\x1b[0;1;4;{}m", style.match_fg.fg());
    let match_sgr = format!("\x1b[0;{}m", style.match_fg.fg());

    for assignment in targets.assignments.assignments() {
        if !assignment.hint.starts_with(typed) {
            continue;
        }
        for span in &assignment.occurrences {
            let cells = targets.cells(span);
            let all_redrawable = cells
                .iter()
                .all(|cell| grid.cell(cell.row, cell.col).redrawable());
            let mut next: Option<(u16, u16)> = None;
            let mut current_sgr = "";
            for (index, cell) in cells.iter().enumerate() {
                let hint_char = assignment.hint.chars().nth(index);
                if hint_char.is_none() && !all_redrawable {
                    break;
                }
                if next != Some((cell.row, cell.col)) {
                    let _ = write!(out, "\x1b[{};{}H", cell.row + 1, cell.col + 1);
                }
                let (sgr, text) = match hint_char {
                    Some(letter) if index < typed.len() => (typed_sgr.as_str(), letter.to_string()),
                    Some(letter) => (hint_sgr.as_str(), letter.to_string()),
                    None => (
                        match_sgr.as_str(),
                        grid.cell(cell.row, cell.col).text.clone(),
                    ),
                };
                if sgr != current_sgr {
                    out.push_str(sgr);
                    current_sgr = sgr;
                }
                out.push_str(&text);
                next = Some((cell.row, cell.col + 1));
            }
        }
    }
    out.push_str("\x1b[0m");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::decode;
    use crate::model::Action;
    use crate::targets::find_targets;

    const STYLE: HintStyle = HintStyle {
        hint_fg: Rgb::new(0, 0, 0),
        hint_bg: Rgb::new(255, 255, 0),
        match_fg: Rgb::new(255, 255, 255),
    };

    /// Replays the paint plus the hints into a fresh emulator and returns row text.
    fn screen(paint: &[u8], cols: u16, rows: u16, typed: &str) -> (Vec<String>, String) {
        let grid = decode(paint, cols, rows).unwrap();
        let targets = find_targets(&grid, Action::Copy, &[]);
        let hints = draw_hints(&grid, &targets, typed, STYLE);
        let mut all = paint.to_vec();
        all.extend_from_slice(hints.as_bytes());
        let after = decode(&all, cols, rows).unwrap();
        let rows = after
            .cells
            .iter()
            .map(|row| row.iter().map(|c| c.text.as_str()).collect::<String>())
            .collect();
        (rows, hints)
    }

    #[test]
    fn hint_replaces_first_cell_and_nothing_moves() {
        let (rows, _) = screen("日本 /tmp/x 🔥 1234\r\nend".as_bytes(), 30, 2, "");
        assert_eq!(rows[0].trim_end(), "日本 atmp/x 🔥 s234");
        assert_eq!(rows[1].trim_end(), "end");
    }

    #[test]
    fn typed_prefix_filters_two_letter_hints() {
        let text: String = (0..30).map(|i| format!("10.0.0.{i}\r\n")).collect();
        let (rows, _) = screen(text.as_bytes(), 20, 31, "s");
        // aa..am belong to the first 26; "s?" hints start at the 27th value.
        assert_eq!(rows[0].trim_end(), "10.0.0.0");
        assert_eq!(rows[26].trim_end(), "sa.0.0.26");
    }

    #[test]
    fn hints_survive_origin_mode_margins_insert_mode_and_line_drawing_charset() {
        let paint = b"/tmp/x\x1b[2;3r\x1b[?6h\x1b[4h\x1b(0\x1b[1;1H";
        let (rows, _) = screen(paint, 10, 4, "");
        assert_eq!(rows[0].trim_end(), "atmp/x");
    }

    #[test]
    fn wrapped_token_is_highlighted_on_both_rows() {
        let (rows, hints) = screen(b"https://ex.test/abc", 10, 2, "");
        assert_eq!(rows[0], "attps://ex");
        assert_eq!(rows[1].trim_end(), ".test/abc");
        assert!(hints.contains("\x1b[2;1H"));
    }
}
