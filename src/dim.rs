//! Dims a captured paint by rewriting SGR parameters in place.
//!
//! Every foreground color becomes the dim color, backgrounds and reverse video go, and
//! bold/italic/underline/strike stay. SGR never moves the cursor or changes a cell's
//! width, so the dimmed paint lays out exactly like the original.

use crate::color::Rgb;
use crate::vt::{tokenize, Token};

pub fn dim(input: &[u8], color: Rgb) -> Vec<u8> {
    let dim_sgr = format!("\x1b[38;2;{};{};{};49m", color.r, color.g, color.b);
    let mut out = Vec::with_capacity(input.len() + input.len() / 8);
    out.extend_from_slice(dim_sgr.as_bytes());
    for token in tokenize(input) {
        match token {
            Token::Csi {
                params,
                intermediates,
                final_byte: b'm',
                ..
            } if intermediates.is_empty()
                && params
                    .iter()
                    .all(|b| b.is_ascii_digit() || *b == b';' || *b == b':') =>
            {
                out.extend_from_slice(b"\x1b[");
                out.extend_from_slice(rewrite_sgr_params(params, color).as_bytes());
                out.push(b'm');
            }
            // A soft or full reset puts SGR back to default: dim again right after.
            Token::Csi {
                raw,
                intermediates: b"!",
                final_byte: b'p',
                ..
            }
            | Token::Esc {
                raw,
                intermediates: b"",
                final_byte: b'c',
            } => {
                out.extend_from_slice(raw);
                out.extend_from_slice(dim_sgr.as_bytes());
            }
            // Restoring the cursor restores its SGR state too (often the default), and so do
            // the alternate-screen switches that save/restore it: re-assert the dim colors
            // (no reset, so restored bold/italic stay).
            token if restores_cursor(&token) => {
                out.extend_from_slice(token.raw());
                out.extend_from_slice(dim_sgr.as_bytes());
            }
            token => out.extend_from_slice(token.raw()),
        }
    }
    out
}

/// `ESC 8`, `CSI u` (SCORC), and the cursor-saving screen modes 47/1047/1048/1049.
fn restores_cursor(token: &Token<'_>) -> bool {
    match token {
        Token::Esc {
            intermediates: b"",
            final_byte: b'8',
            ..
        } => true,
        Token::Csi {
            params: b"",
            intermediates: b"",
            final_byte: b'u',
            ..
        } => true,
        Token::Csi {
            params,
            intermediates: b"",
            final_byte: b'h' | b'l',
            ..
        } if params.first() == Some(&b'?') => params[1..]
            .split(|&b| b == b';')
            .any(|mode| matches!(mode, b"47" | b"1047" | b"1048" | b"1049")),
        _ => false,
    }
}

/// Rewrites one SGR parameter string (the bytes between `CSI` and `m`).
pub fn rewrite_sgr_params(params: &[u8], color: Rgb) -> String {
    let text = std::str::from_utf8(params).unwrap_or("");
    let parts: Vec<&str> = text.split(';').collect();
    let mut kept: Vec<&str> = Vec::new();
    let mut at = 0;
    while at < parts.len() {
        let part = parts[at];
        at += 1;
        if part.contains(':') {
            // Colon form carries its own arguments: 38:2::r:g:b, 48:5:n, 58:2::r:g:b, 4:3.
            let head = part.split(':').next().unwrap_or("");
            if !matches!(head, "38" | "48") {
                kept.push(part);
            }
            continue;
        }
        let code: u32 = if part.is_empty() {
            0
        } else {
            part.parse().unwrap_or(u32::MAX)
        };
        match code {
            38 | 48 | 58 => {
                // Extended color: 5;n or 2;r;g;b follow as separate parameters.
                let args = match parts.get(at).copied() {
                    Some("5") => 2,
                    Some("2") => 4,
                    _ => 0,
                };
                let end = (at + args).min(parts.len());
                if code == 58 {
                    kept.push(part);
                    kept.extend(&parts[at..end]);
                }
                at = end;
            }
            30..=37 | 39 | 90..=97 | 40..=47 | 49 | 100..=107 | 7 => {}
            0 => kept.push("0"),
            _ => kept.push(part),
        }
    }
    let dim = format!("38;2;{};{};{}", color.r, color.g, color.b);
    let mut out = kept.join(";");
    if !out.is_empty() {
        out.push(';');
    }
    out.push_str(&dim);
    out.push_str(";49");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const C: Rgb = Rgb { r: 1, g: 2, b: 3 };

    fn r(params: &str) -> String {
        rewrite_sgr_params(params.as_bytes(), C)
    }

    #[test]
    fn bare_reset_becomes_reset_plus_dim() {
        assert_eq!(r(""), "0;38;2;1;2;3;49");
        assert_eq!(r("0"), "0;38;2;1;2;3;49");
    }

    #[test]
    fn removes_every_foreground_and_background_form() {
        assert_eq!(r("31"), "38;2;1;2;3;49");
        assert_eq!(r("91;42"), "38;2;1;2;3;49");
        assert_eq!(r("38;5;196"), "38;2;1;2;3;49");
        assert_eq!(r("38;2;10;20;30;48;2;1;1;1"), "38;2;1;2;3;49");
        assert_eq!(r("38:2::10:20:30"), "38;2;1;2;3;49");
        assert_eq!(r("38:5:9;48:5:1"), "38;2;1;2;3;49");
        assert_eq!(r("39;49;107;100"), "38;2;1;2;3;49");
    }

    #[test]
    fn removes_reverse_and_keeps_text_attributes() {
        assert_eq!(r("1;7;3;4;9"), "1;3;4;9;38;2;1;2;3;49");
        assert_eq!(r("0;1;31"), "0;1;38;2;1;2;3;49");
        assert_eq!(r("4:3"), "4:3;38;2;1;2;3;49");
    }

    #[test]
    fn keeps_underline_color_with_its_arguments() {
        assert_eq!(r("58;5;4;1"), "58;5;4;1;38;2;1;2;3;49");
        assert_eq!(r("58:2::1:2:3"), "58:2::1:2:3;38;2;1;2;3;49");
    }

    #[test]
    fn dims_the_start_and_after_resets_and_leaves_other_bytes_alone() {
        let out = dim(b"a\x1b[31mb\x1b[!pc\x1bcd\x1b[5;1H\x1b[?25l", C);
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "\x1b[38;2;1;2;3;49ma\x1b[38;2;1;2;3;49mb\x1b[!p\x1b[38;2;1;2;3;49mc\x1bc\x1b[38;2;1;2;3;49md\x1b[5;1H\x1b[?25l"
        );
    }

    #[test]
    fn dims_again_after_cursor_restores_which_restore_sgr_too() {
        let d = "\x1b[38;2;1;2;3;49m";
        let out = String::from_utf8(dim(b"\x1b[?1049la\x1b8b\x1b[uc\x1b[?1048ld\x1b[?1049he", C))
            .unwrap();
        assert_eq!(
            out,
            format!("{d}\x1b[?1049l{d}a\x1b8{d}b\x1b[u{d}c\x1b[?1048l{d}d\x1b[?1049h{d}e")
        );
        // Kitty keyboard pops (CSI < u) are not cursor restores; private u is left alone.
        assert_eq!(
            String::from_utf8(dim(b"\x1b[<u", C)).unwrap(),
            format!("{d}\x1b[<u")
        );
    }

    #[test]
    fn private_sgr_like_sequences_are_untouched() {
        assert_eq!(dim(b"\x1b[>4;2m", C)[16..].to_vec(), b"\x1b[>4;2m".to_vec());
    }
}
