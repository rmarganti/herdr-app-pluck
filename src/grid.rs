//! Decodes a captured paint into a cell grid with libghostty-vt, the emulator herdr-core
//! itself uses, so cell widths (CJK, emoji, ZWJ and VS16 clusters) agree with the pane.
//!
//! The grid only decides *where* hints go. What the user sees is always the replayed
//! capture, never something rebuilt from this grid.

use anyhow::{anyhow, Result};
use ghostty_vt::screen::CellWide;
use ghostty_vt::terminal::{Point, PointCoordinate};
use ghostty_vt::{Terminal, TerminalOptions};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GridCell {
    /// The cell's grapheme cluster; a space for an empty cell, "" for a wide char's spacer.
    pub text: String,
    /// Columns this cell starts: 1 or 2, or 0 for a spacer.
    pub width: u8,
    /// OSC 8 target, if the cell is part of a hyperlink.
    pub link: Option<String>,
}

impl GridCell {
    /// Whether pluck may print over this cell: one narrow, non-combining character.
    pub fn redrawable(&self) -> bool {
        let mut chars = self.text.chars();
        self.width == 1
            && matches!((chars.next(), chars.next()), (Some(c), None) if !c.is_control())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grid {
    pub cols: u16,
    pub rows: u16,
    pub cells: Vec<Vec<GridCell>>,
    /// Row `y` continues on row `y + 1` (soft wrap).
    pub wrapped: Vec<bool>,
}

/// Feeds `paint` to a fresh `cols` x `rows` emulator and reads back every visible cell.
pub fn decode(paint: &[u8], cols: u16, rows: u16) -> Result<Grid> {
    let vt_err = |error: ghostty_vt::Error| anyhow!("terminal decoder: {error}");
    let mut terminal = Terminal::new(TerminalOptions {
        cols,
        rows,
        max_scrollback: 0,
    })
    .map_err(vt_err)?;
    terminal.vt_write(paint);

    let mut cells = Vec::with_capacity(rows as usize);
    let mut wrapped = Vec::with_capacity(rows as usize);
    let mut graphemes = vec!['\0'; 32];
    let mut link = vec![0u8; 4096];
    for y in 0..rows {
        let mut row = Vec::with_capacity(cols as usize);
        let mut row_wrapped = false;
        for x in 0..cols {
            let grid_ref = terminal
                .grid_ref(Point::Active(PointCoordinate { x, y: y as u32 }))
                .map_err(vt_err)?;
            if x == 0 {
                row_wrapped = grid_ref
                    .row()
                    .and_then(|r| r.is_wrapped())
                    .map_err(vt_err)?;
            }
            let cell = grid_ref.cell().map_err(vt_err)?;
            let width = match cell.wide().map_err(vt_err)? {
                CellWide::Narrow => 1,
                CellWide::Wide => 2,
                CellWide::SpacerTail | CellWide::SpacerHead => 0,
            };
            let text = if width == 0 {
                String::new()
            } else if cell.has_text().map_err(vt_err)? {
                let count = loop {
                    match grid_ref.graphemes(&mut graphemes) {
                        Ok(count) => break count,
                        Err(ghostty_vt::Error::OutOfSpace { required }) => {
                            graphemes.resize(required, '\0')
                        }
                        Err(error) => return Err(vt_err(error)),
                    }
                };
                graphemes[..count].iter().collect()
            } else {
                " ".to_string()
            };
            let link_target = if cell.has_hyperlink().map_err(vt_err)? {
                let len = loop {
                    match grid_ref.hyperlink_uri(&mut link) {
                        Ok(len) => break len,
                        Err(ghostty_vt::Error::OutOfSpace { required }) => link.resize(required, 0),
                        Err(error) => return Err(vt_err(error)),
                    }
                };
                Some(String::from_utf8_lossy(&link[..len]).into_owned()).filter(|s| !s.is_empty())
            } else {
                None
            };
            row.push(GridCell {
                text,
                width,
                link: link_target,
            });
        }
        cells.push(row);
        wrapped.push(row_wrapped);
    }
    Ok(Grid {
        cols,
        rows,
        cells,
        wrapped,
    })
}

/// Where one character of a logical line sits on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellRef {
    pub row: u16,
    pub col: u16,
    pub byte_start: usize,
    pub byte_end: usize,
}

/// One logical line: soft-wrapped rows joined, with a map from text bytes to cells.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogicalLine {
    pub text: String,
    pub cells: Vec<CellRef>,
}

impl LogicalLine {
    /// Cells covering the byte range `start..end`, in order.
    pub fn cells_for(&self, start: usize, end: usize) -> Vec<CellRef> {
        self.cells
            .iter()
            .filter(|cell| cell.byte_start < end && start < cell.byte_end)
            .copied()
            .collect()
    }
}

impl Grid {
    pub fn cell(&self, row: u16, col: u16) -> &GridCell {
        &self.cells[row as usize][col as usize]
    }

    fn row_text(&self, row: usize) -> String {
        self.cells[row]
            .iter()
            .filter(|cell| cell.width != 0)
            .map(|cell| cell.text.as_str())
            .collect()
    }

    /// Marks soft wraps from `hc read`'s logical lines.
    ///
    /// herdr-core's observer repaint ends every row with CR LF, wrapped or not, so the
    /// replayed grid never learns which rows soft-wrapped. `hc read` reports logical
    /// lines. Row `r` is marked wrapped only when it is an exact prefix of its logical
    /// line and that line goes on into row `r + 1`. On any mismatch (the screen changed
    /// between capture and read, say) nothing more is marked, which only means tokens
    /// are matched per row.
    pub fn apply_soft_wraps(&mut self, logical_lines: &[String]) {
        let rows = self.cells.len();
        let mut row = 0;
        for line in logical_lines {
            if row >= rows {
                return;
            }
            let mut remaining = line.as_str();
            loop {
                let text = self.row_text(row);
                let visible = text.trim_end();
                if !remaining.starts_with(visible) {
                    return; // out of step with the grid: stop guessing
                }
                if remaining.len() > text.len() && remaining.starts_with(text.as_str()) {
                    remaining = &remaining[text.len()..];
                    self.wrapped[row] = true;
                    row += 1;
                    if row >= rows {
                        return;
                    }
                    continue;
                }
                if remaining.trim_end() != visible {
                    return;
                }
                row += 1;
                break;
            }
        }
    }

    /// Joins soft-wrapped rows into logical lines (spacers skipped).
    pub fn logical_lines(&self) -> Vec<LogicalLine> {
        let mut lines = Vec::new();
        let mut current = LogicalLine {
            text: String::new(),
            cells: Vec::new(),
        };
        for (y, row) in self.cells.iter().enumerate() {
            for (x, cell) in row.iter().enumerate() {
                if cell.width == 0 {
                    continue;
                }
                let start = current.text.len();
                current.text.push_str(&cell.text);
                current.cells.push(CellRef {
                    row: y as u16,
                    col: x as u16,
                    byte_start: start,
                    byte_end: current.text.len(),
                });
            }
            if !self.wrapped[y] {
                lines.push(std::mem::replace(
                    &mut current,
                    LogicalLine {
                        text: String::new(),
                        cells: Vec::new(),
                    },
                ));
            }
        }
        if !current.cells.is_empty() {
            lines.push(current);
        }
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row_text(grid: &Grid, y: usize) -> String {
        grid.cells[y].iter().map(|c| c.text.as_str()).collect()
    }

    #[test]
    fn decodes_text_and_wide_cells() {
        let grid = decode("ab日🔥x\r\nnext".as_bytes(), 10, 3).unwrap();
        assert_eq!(row_text(&grid, 0), "ab日🔥x   ");
        let widths: Vec<u8> = grid.cells[0].iter().map(|c| c.width).collect();
        assert_eq!(widths, vec![1, 1, 2, 0, 2, 0, 1, 1, 1, 1]);
        assert_eq!(row_text(&grid, 1).trim_end(), "next");
    }

    #[test]
    fn grapheme_clusters_take_two_cells_like_herdr_core() {
        // herdr-core runs with mode 2027: ZWJ, VS16, flags and skin tones are one wide cell.
        let grid = decode("👨\u{200d}👩\u{200d}👧X❤\u{fe0f}Y🇯🇵Z".as_bytes(), 12, 1).unwrap();
        let x = grid.cells[0].iter().position(|c| c.text == "X").unwrap();
        let y = grid.cells[0].iter().position(|c| c.text == "Y").unwrap();
        let z = grid.cells[0].iter().position(|c| c.text == "Z").unwrap();
        assert_eq!((x, y, z), (2, 5, 8));
    }

    #[test]
    fn combining_marks_join_their_base_cell() {
        let grid = decode("e\u{301}x".as_bytes(), 4, 1).unwrap();
        assert_eq!(grid.cells[0][0].text, "e\u{301}");
        assert!(!grid.cells[0][0].redrawable());
        assert!(grid.cells[0][1].redrawable());
    }

    #[test]
    fn soft_wraps_join_into_one_logical_line() {
        let grid = decode(b"abcdefgh\r\nxy", 5, 3).unwrap();
        assert_eq!(grid.wrapped, vec![true, false, false]);
        let lines = grid.logical_lines();
        assert_eq!(lines[0].text, "abcdefgh  ");
        assert_eq!(lines[1].text.trim_end(), "xy");
        let cells = lines[0].cells_for(3, 7);
        let at: Vec<(u16, u16)> = cells.iter().map(|c| (c.row, c.col)).collect();
        assert_eq!(at, vec![(0, 3), (0, 4), (1, 0), (1, 1)]);
    }

    #[test]
    fn soft_wraps_come_from_hc_read_when_the_repaint_lost_them() {
        // The observer writes each row then CR LF, even for a wrapped row.
        let mut grid = decode(b"abcde\r\nfgh\r\nnext\r\nxy", 5, 4).unwrap();
        assert_eq!(grid.wrapped, vec![false; 4]);
        grid.apply_soft_wraps(&["abcdefgh".into(), "next".into(), "xy".into()]);
        assert_eq!(grid.wrapped, vec![true, false, false, false]);
        assert_eq!(grid.logical_lines()[0].text, "abcdefgh  ");
    }

    #[test]
    fn soft_wrap_marks_stop_at_the_first_disagreement() {
        let mut grid = decode(b"abcde\r\nfgh\r\nzzzzz\r\nq", 5, 4).unwrap();
        // The screen changed after capture: the read no longer lines up from row 2 on.
        grid.apply_soft_wraps(&["abcdefgh".into(), "other".into(), "q".into()]);
        assert_eq!(grid.wrapped, vec![true, false, false, false]);
    }

    #[test]
    fn a_full_row_followed_by_a_new_line_is_not_a_wrap() {
        let mut grid = decode(b"abcde\r\nfgh", 5, 2).unwrap();
        grid.apply_soft_wraps(&["abcde".into(), "fgh".into()]);
        assert_eq!(grid.wrapped, vec![false, false]);
    }

    #[test]
    fn wide_char_pushed_to_the_next_row_still_joins() {
        // 5 columns: "abcd" + a wide char that doesn't fit wraps it to row 2.
        let mut grid = decode("abcd日x".as_bytes(), 5, 2).unwrap();
        grid.wrapped = vec![false, false];
        grid.apply_soft_wraps(&["abcd日x".into()]);
        assert_eq!(grid.wrapped, vec![true, false]);
    }

    #[test]
    fn cursor_moves_and_erases_are_followed() {
        let grid = decode(b"\x1b[2;3Hhi\x1b[1;1Hzz\x1b[K", 6, 2).unwrap();
        assert_eq!(row_text(&grid, 0), "zz    ");
        assert_eq!(row_text(&grid, 1), "  hi  ");
    }

    #[test]
    fn hyperlinks_are_recorded_per_cell() {
        let grid = decode(b"a \x1b]8;;https://x.test\x1b\\link\x1b]8;;\x1b\\ b", 12, 1).unwrap();
        let links: Vec<Option<&str>> = grid.cells[0].iter().map(|c| c.link.as_deref()).collect();
        assert_eq!(links[1], None);
        assert_eq!(links[2..6], [Some("https://x.test"); 4]);
        assert_eq!(links[6], None);
    }

    #[test]
    fn alternate_screen_content_is_what_is_read() {
        let grid = decode(b"main\x1b[?1049h\x1b[Halt", 6, 1).unwrap();
        assert_eq!(row_text(&grid, 0), "alt   ");
    }
}
