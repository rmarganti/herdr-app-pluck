//! Makes a captured paint safe to replay into the picker's own terminal.
//!
//! Everything that changes what the screen *shows* is kept byte for byte. What goes is
//! anything that changes how the picker's terminal *reads input* (mouse, kitty keyboard,
//! application cursor keys, bracketed paste, ...) and anything that would make the
//! terminal answer into the picker's stdin (device status, attribute and mode queries).
//! Titles, cwd and agent-status reports go too, so the picker never poses as the source.

use crate::vt::{tokenize, Token};

/// DEC private modes that only change input or reporting, never the display.
const INPUT_PRIVATE_MODES: &[u32] = &[
    1,    // application cursor keys
    9,    // X10 mouse
    66,   // application keypad (DECNKM)
    1000, // mouse
    1001, 1002, 1003, 1004, // mouse, focus events
    1005, 1006, 1007, 1015, 1016, // mouse encodings, alternate scroll
    1035, 1036, 1039, // alt/meta key handling
    2004, // bracketed paste
    2026, // synchronized output: an unbalanced set would freeze the picker's screen
    2031, // color scheme reports
    2048, // in-band resize reports
];

/// OSC numbers that set display colors; kept unless they are queries.
const DISPLAY_OSCS: &[&[u8]] = &[b"4", b"10", b"11", b"12", b"104", b"110", b"111", b"112"];

pub fn sanitize(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len());
    for token in tokenize(input) {
        match token {
            Token::Text(text) => {
                // ENQ makes the terminal send its answerback.
                out.extend(text.iter().filter(|&&byte| byte != 0x05));
            }
            Token::Csi {
                raw,
                params,
                intermediates,
                final_byte,
            } => sanitize_csi(&mut out, raw, params, intermediates, final_byte),
            Token::Osc { raw, body } => {
                let number = body.split(|&b| b == b';').next().unwrap_or_default();
                let keep =
                    number == b"8" || (DISPLAY_OSCS.contains(&number) && !body.contains(&b'?'));
                if keep {
                    out.extend_from_slice(raw);
                }
            }
            Token::String { raw, kind, body } => {
                let query = match kind {
                    b'P' => body.starts_with(b"$q") || body.starts_with(b"+q"),
                    b'_' => body.starts_with(b"G") && contains(body, b"a=q"),
                    _ => true, // SOS/PM: nothing a terminal displays
                };
                if !query {
                    out.extend_from_slice(raw);
                }
            }
            Token::Esc {
                raw,
                intermediates,
                final_byte,
            } => {
                // ESC = (application keypad) changes keys only.
                if !(intermediates.is_empty() && final_byte == b'=') {
                    out.extend_from_slice(raw);
                }
            }
        }
    }
    out
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn sanitize_csi(
    out: &mut Vec<u8>,
    raw: &[u8],
    params: &[u8],
    intermediates: &[u8],
    final_byte: u8,
) {
    let marker = params
        .first()
        .copied()
        .filter(|b| matches!(b, b'?' | b'>' | b'=' | b'<'));

    // Queries and reports: device status, device attributes, mode requests, window ops,
    // XTVERSION, XTSMGRAPHICS, DECREQTPARM.
    let plain = intermediates.is_empty();
    let is_query = (plain && matches!(final_byte, b'n' | b'c' | b't' | b'x'))
        || (intermediates == b"$" && final_byte == b'p')
        || (plain && marker == Some(b'>') && final_byte == b'q')
        || (plain && marker == Some(b'?') && final_byte == b'S');
    // Kitty keyboard (CSI > u, CSI = u, CSI < u, CSI ? u) and modifyOtherKeys (CSI > m).
    let is_keyboard = plain
        && ((final_byte == b'u' && marker.is_some())
            || (marker == Some(b'>') && matches!(final_byte, b'm' | b'n')));
    if is_query || is_keyboard {
        return;
    }

    if marker == Some(b'?') && intermediates.is_empty() && matches!(final_byte, b'h' | b'l') {
        let kept: Vec<&[u8]> = params[1..]
            .split(|&b| b == b';')
            .filter(|param| {
                let mode = std::str::from_utf8(param)
                    .ok()
                    .and_then(|text| text.parse::<u32>().ok());
                !matches!(mode, Some(mode) if INPUT_PRIVATE_MODES.contains(&mode))
            })
            .collect();
        if kept.is_empty() {
            return;
        }
        out.extend_from_slice(b"\x1b[?");
        out.extend_from_slice(&kept.join(&b';'));
        out.push(final_byte);
        return;
    }

    out.extend_from_slice(raw);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(input: &[u8]) -> Vec<u8> {
        sanitize(input)
    }

    #[test]
    fn keeps_display_bytes_verbatim() {
        let input = "\x18\x1b[!p\x1b[?1049h\x1b[?7l\x1b[H\x1b[2J\x1b[3;20r\x1b[?6h\x1b[38;5;1mred\x1b[0m 日本 🔥\r\n\x1b]8;;https://x\x1b\\x\x1b]8;;\x1b\\\x1b(0q\x1b(B\x1b[5;1H\x1b[?25l\x1b[2 q"
            .as_bytes();
        assert_eq!(s(input), input.to_vec());
    }

    #[test]
    fn drops_mouse_focus_paste_and_cursor_key_modes() {
        assert_eq!(
            s(b"\x1b[?1000h\x1b[?1006h\x1b[?1004h\x1b[?2004h\x1b[?1h"),
            b""
        );
        assert_eq!(
            s(b"\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1005l\x1b[?1015l\x1b[?1016l"),
            b""
        );
    }

    #[test]
    fn keeps_display_modes_out_of_a_mixed_mode_set() {
        assert_eq!(s(b"\x1b[?1049;1000;25h"), b"\x1b[?1049;25h".to_vec());
        assert_eq!(s(b"\x1b[?1;7l"), b"\x1b[?7l".to_vec());
    }

    #[test]
    fn drops_kitty_keyboard_and_modify_other_keys() {
        assert_eq!(
            s(b"\x1b[>1u\x1b[=0;1u\x1b[<u\x1b[?u\x1b[>4;2m\x1b[>4n"),
            b""
        );
    }

    #[test]
    fn drops_queries_that_would_answer_into_stdin() {
        assert_eq!(
            s(b"\x1b[6n\x1b[5n\x1b[?6n\x1b[c\x1b[>c\x1b[=c\x1b[?2027$p\x1b[14t\x1b[>q\x1bP$qm\x1b\\\x1bP+q544e\x1b\\\x05"),
            b""
        );
        assert_eq!(s(b"\x1b]11;?\x07\x1b]4;1;?\x1b\\\x1b]52;c;?\x07"), b"");
    }

    #[test]
    fn drops_titles_cwd_clipboard_and_agent_status() {
        assert_eq!(
            s(b"\x1b]0;title\x07\x1b]2;t\x07\x1b]7;file:///x\x07\x1b]52;c;aGk=\x07\x1b]7501;working\x07\x1b]133;A\x07"),
            b""
        );
    }

    #[test]
    fn keeps_color_sets_and_drops_keypad_mode() {
        assert_eq!(s(b"\x1b]11;#000000\x07"), b"\x1b]11;#000000\x07".to_vec());
        assert_eq!(s(b"\x1b=\x1b>"), b"\x1b>".to_vec());
    }

    #[test]
    fn soft_reset_is_not_a_mode_request() {
        assert_eq!(s(b"\x1b[!p"), b"\x1b[!p".to_vec());
    }

    #[test]
    fn real_observer_preamble_loses_only_input_modes() {
        let input = b"\x18\x1b[!p\x1b[?2026l\x1b[?9l\x1b[?1000l\x1b[?1001l\x1b[?1002l\x1b[?1003l\x1b[?1004l\x1b[?1005l\x1b[?1006l\x1b[?1015l\x1b[?1016l\x1b[?2004l\x1b[?1l\x1b[?66l\x1b[?2031l\x1b[?2048l\x1b[=0;1u\x1b[?1049l\x1b[?1047l\x1b[?47l\x1b[H\x1b[2Jhi\x1b[5;1H\x1b[0m\x1b[?1004h";
        assert_eq!(
            s(input),
            b"\x18\x1b[!p\x1b[?1049l\x1b[?1047l\x1b[?47l\x1b[H\x1b[2Jhi\x1b[5;1H\x1b[0m".to_vec()
        );
    }
}
