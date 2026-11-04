//! Keys while the picker is up.

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Backspace,
    Cancel,
    Resize(u16, u16),
    Other,
}

pub fn key_from_event(event: Event) -> Key {
    match event {
        Event::Key(KeyEvent {
            kind: KeyEventKind::Release,
            ..
        }) => Key::Other,
        Event::Key(KeyEvent {
            code: KeyCode::Char('c'),
            modifiers,
            ..
        }) if modifiers.contains(KeyModifiers::CONTROL) => Key::Cancel,
        Event::Key(KeyEvent {
            code: KeyCode::Char(ch),
            modifiers,
            ..
        }) if !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
            // Some terminals report Shift+a as 'a' with SHIFT rather than 'A'.
            if modifiers.contains(KeyModifiers::SHIFT) {
                Key::Char(ch.to_ascii_uppercase())
            } else {
                Key::Char(ch)
            }
        }
        Event::Key(KeyEvent {
            code: KeyCode::Backspace,
            ..
        }) => Key::Backspace,
        Event::Key(KeyEvent {
            code: KeyCode::Esc, ..
        }) => Key::Cancel,
        Event::Resize(cols, rows) => Key::Resize(cols, rows),
        _ => Key::Other,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Nothing to do.
    Ignore,
    /// The typed prefix changed: redraw.
    Redraw,
    Cancel,
    Pick {
        hint: String,
        shift: bool,
    },
}

/// Typed hint letters so far.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputState {
    width: usize,
    typed: String,
}

impl InputState {
    pub fn new(width: usize) -> Self {
        Self {
            width,
            typed: String::new(),
        }
    }

    pub fn typed(&self) -> &str {
        &self.typed
    }

    pub fn push(&mut self, key: Key, valid_hints: &[&str]) -> Decision {
        match key {
            Key::Cancel => Decision::Cancel,
            Key::Backspace => {
                if self.typed.pop().is_some() {
                    Decision::Redraw
                } else {
                    Decision::Ignore
                }
            }
            Key::Char(ch) if ch.is_ascii_alphabetic() => {
                let candidate = format!("{}{}", self.typed, ch.to_ascii_lowercase());
                if !valid_hints.iter().any(|hint| hint.starts_with(&candidate)) {
                    return Decision::Ignore;
                }
                if candidate.len() >= self.width {
                    self.typed.clear();
                    Decision::Pick {
                        hint: candidate,
                        shift: ch.is_ascii_uppercase(),
                    }
                } else {
                    self.typed = candidate;
                    Decision::Redraw
                }
            }
            Key::Char(_) | Key::Resize(..) | Key::Other => Decision::Ignore,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pick(hint: &str, shift: bool) -> Decision {
        Decision::Pick {
            hint: hint.into(),
            shift,
        }
    }

    #[test]
    fn one_letter_hint_picks_and_shift_is_reported() {
        let mut state = InputState::new(1);
        assert_eq!(state.push(Key::Char('a'), &["a", "s"]), pick("a", false));
        assert_eq!(state.push(Key::Char('S'), &["a", "s"]), pick("s", true));
    }

    #[test]
    fn two_letter_hint_narrows_then_picks() {
        let mut state = InputState::new(2);
        assert_eq!(state.push(Key::Char('s'), &["aa", "sa"]), Decision::Redraw);
        assert_eq!(state.typed(), "s");
        assert_eq!(state.push(Key::Char('A'), &["aa", "sa"]), pick("sa", true));
    }

    #[test]
    fn letters_that_start_no_hint_are_ignored() {
        let mut state = InputState::new(2);
        assert_eq!(state.push(Key::Char('x'), &["aa", "sa"]), Decision::Ignore);
        assert_eq!(state.push(Key::Char('a'), &["aa", "sa"]), Decision::Redraw);
        assert_eq!(state.push(Key::Char('d'), &["aa", "sa"]), Decision::Ignore);
        assert_eq!(state.push(Key::Char('1'), &["aa", "sa"]), Decision::Ignore);
        assert_eq!(state.typed(), "a");
    }

    #[test]
    fn backspace_removes_the_last_letter() {
        let mut state = InputState::new(2);
        assert_eq!(state.push(Key::Backspace, &["aa"]), Decision::Ignore);
        state.push(Key::Char('a'), &["aa"]);
        assert_eq!(state.push(Key::Backspace, &["aa"]), Decision::Redraw);
        assert_eq!(state.typed(), "");
    }

    #[test]
    fn escape_and_ctrl_c_cancel() {
        let mut state = InputState::new(1);
        assert_eq!(state.push(Key::Cancel, &["a"]), Decision::Cancel);
        assert_eq!(
            key_from_event(Event::Key(KeyEvent::new(
                KeyCode::Char('c'),
                KeyModifiers::CONTROL
            ))),
            Key::Cancel
        );
        assert_eq!(
            key_from_event(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))),
            Key::Cancel
        );
    }

    #[test]
    fn shifted_letters_arrive_uppercase_either_way() {
        assert_eq!(
            key_from_event(Event::Key(KeyEvent::new(
                KeyCode::Char('A'),
                KeyModifiers::SHIFT
            ))),
            Key::Char('A')
        );
        assert_eq!(
            key_from_event(Event::Key(KeyEvent::new(
                KeyCode::Char('a'),
                KeyModifiers::SHIFT
            ))),
            Key::Char('A')
        );
    }
}
