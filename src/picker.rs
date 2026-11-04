//! The picker: capture, match, replay, draw hints, read keys, report.

use crate::capture::{capture, LIMITS};
use crate::color::Rgb;
use crate::config::{default_config_path, load_patterns};
use crate::dim::dim;
use crate::grid::{decode, Grid};
use crate::hc::{read_screen_lines, terminal_facts};
use crate::input::{key_from_event, Decision, InputState, Key};
use crate::model::Action;
use crate::outcome::{write_ready, write_result, Outcome};
use crate::patterns::is_openable_url;
use crate::render::{draw_hints, HintStyle};
use crate::sanitize::sanitize;
use crate::targets::{find_targets, Targets};
use anyhow::{bail, Context, Result};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Longer than the mod's 3 s of re-sending `terminal.show`.
const SIZE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
pub struct PickOptions {
    pub source: String,
    pub action: Action,
    pub run_dir: PathBuf,
    pub config: Option<PathBuf>,
    pub dim_color: Rgb,
    pub style: HintStyle,
    pub no_dim: bool,
    pub no_hints: bool,
    pub capture_out: Option<PathBuf>,
}

/// Runs the picker and always leaves a `result.json` behind. Returns the process exit code.
pub fn run(options: &PickOptions) -> i32 {
    let mut warnings = Vec::new();
    let result = pick(options, &mut warnings);
    let (outcome, code) = match result {
        Ok(outcome) => (outcome, 0),
        Err(error) => (
            Outcome::Error {
                message: format!("{error:#}"),
            },
            1,
        ),
    };
    if let Err(error) = write_result(&options.run_dir, &outcome, &warnings) {
        eprintln!("herdr-app-pluck: {error:#}");
        return 1;
    }
    code
}

fn pick(options: &PickOptions, warnings: &mut Vec<String>) -> Result<Outcome> {
    let facts = terminal_facts(&options.source)?;
    let (cols, rows) = (facts.cols, facts.rows);
    if cols == 0 || rows == 0 {
        bail!("terminal {} has no visible area", options.source);
    }

    let raw = capture(&options.source, LIMITS)?;
    if let Some(path) = &options.capture_out {
        std::fs::write(path, &raw)
            .with_context(|| format!("failed to write {}", path.display()))?;
    }
    // Right after the capture, while the screen still matches it: the repaint doesn't
    // say which rows soft-wrap, `hc read` does. Without it tokens match per row.
    let logical_lines = read_screen_lines(&options.source).ok();
    let sanitized = sanitize(&raw);
    let mut grid = decode(&sanitized, cols, rows)?;
    if let Some(lines) = &logical_lines {
        grid.apply_soft_wraps(lines);
    }

    let custom = match options.action {
        // Opening only ever uses the built-in URL detector and OSC 8 links.
        Action::Open => Vec::new(),
        Action::Copy => {
            let config = options.config.clone().or_else(default_config_path);
            let loaded = load_patterns(config.as_deref(), facts.live_cwd().map(Path::new));
            warnings.extend(loaded.warnings);
            loaded.patterns
        }
    };
    let targets = find_targets(&grid, options.action, &custom);
    if targets.assignments.is_empty() && !options.no_hints {
        return Ok(Outcome::NoMatches);
    }

    let paint = if options.no_dim {
        sanitized
    } else {
        dim(&sanitized, options.dim_color)
    };

    write_ready(&options.run_dir, targets.assignments.len(), warnings)?;
    wait_for_size(cols, rows, SIZE_TIMEOUT)?;
    let _terminal = RawTerminal::enter()?;
    interact(options, &grid, &targets, &paint, (cols, rows))
}

fn interact(
    options: &PickOptions,
    grid: &Grid,
    targets: &Targets,
    paint: &[u8],
    size: (u16, u16),
) -> Result<Outcome> {
    let mut stdout = std::io::stdout();
    let mut state = InputState::new(targets.assignments.width().unwrap_or(1));
    let valid: Vec<&str> = targets.assignments.valid_hints().collect();
    let draw = |out: &mut std::io::Stdout, typed: &str| -> Result<()> {
        // Restoring always comes from the original bytes, never from the decoder.
        let mut frame = b"\x1b[?2026h".to_vec();
        frame.extend_from_slice(paint);
        if !options.no_hints {
            frame.extend_from_slice(draw_hints(grid, targets, typed, options.style).as_bytes());
        }
        frame.extend_from_slice(b"\x1b[?25l\x1b[?2026l");
        out.write_all(&frame)?;
        out.flush()?;
        Ok(())
    };
    draw(&mut stdout, "")?;

    loop {
        let key = key_from_event(crossterm::event::read().context("failed to read a key")?);
        if let Key::Resize(cols, rows) = key {
            if (cols, rows) != size {
                return Ok(Outcome::Cancelled {
                    message: Some("The pane changed size, so pluck closed.".into()),
                });
            }
            draw(&mut stdout, state.typed())?;
            continue;
        }
        if options.no_hints {
            if key == Key::Cancel {
                return Ok(Outcome::Cancelled { message: None });
            }
            continue;
        }
        match state.push(key, &valid) {
            Decision::Ignore => {}
            Decision::Redraw => draw(&mut stdout, state.typed())?,
            Decision::Cancel => return Ok(Outcome::Cancelled { message: None }),
            Decision::Pick { hint, shift } => {
                let text = targets
                    .assignments
                    .copied_text_for_hint(&hint)
                    .context("picked an unknown hint")?
                    .to_string();
                let is_url = is_openable_url(&text);
                return Ok(Outcome::Picked {
                    action: final_action(options.action, shift, is_url),
                    text,
                    is_url,
                });
            }
        }
    }
}

/// Shift flips the action: copy+Shift opens (URLs only), open+Shift copies.
pub fn final_action(action: Action, shift: bool, is_url: bool) -> Action {
    match (action, shift) {
        (Action::Copy, true) if is_url => Action::Open,
        (Action::Copy, _) => Action::Copy,
        (Action::Open, shift) => {
            if shift || !is_url {
                Action::Copy
            } else {
                Action::Open
            }
        }
    }
}

/// The app resizes the picker's terminal to the pane a moment after showing it.
fn wait_for_size(cols: u16, rows: u16, timeout: Duration) -> Result<()> {
    let started = Instant::now();
    loop {
        let size = crossterm::terminal::size().context("failed to read the picker's size")?;
        if size == (cols, rows) {
            return Ok(());
        }
        if started.elapsed() >= timeout {
            bail!(
                "the app never gave the picker the pane's size ({cols}x{rows}; it is {}x{})",
                size.0,
                size.1
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Raw mode for the picker's keys; restored, cursor shown, on drop.
struct RawTerminal;

impl RawTerminal {
    fn enter() -> Result<Self> {
        crossterm::terminal::enable_raw_mode().context("failed to enable raw mode")?;
        Ok(Self)
    }
}

impl Drop for RawTerminal {
    fn drop(&mut self) {
        let mut stdout = std::io::stdout();
        let _ = stdout.write_all(b"\x1b[0m\x1b[?25h");
        let _ = stdout.flush();
        let _ = crossterm::terminal::disable_raw_mode();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shift_flips_the_action_per_the_key_table() {
        assert_eq!(final_action(Action::Copy, false, true), Action::Copy);
        assert_eq!(final_action(Action::Copy, true, true), Action::Open);
        assert_eq!(final_action(Action::Copy, true, false), Action::Copy);
        assert_eq!(final_action(Action::Open, false, true), Action::Open);
        assert_eq!(final_action(Action::Open, true, true), Action::Copy);
    }
}
