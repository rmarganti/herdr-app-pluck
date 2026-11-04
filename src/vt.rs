//! A tokenizer for the byte stream `hc attach --observe` paints: just enough to find
//! escape sequences and pass everything else through untouched.

/// One piece of the stream. `Text` is printable bytes and C0 controls; every other
/// variant is one complete escape sequence. Concatenating every token's `raw` gives back
/// the input, minus an incomplete sequence at the very end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token<'a> {
    Text(&'a [u8]),
    /// `ESC [ params intermediates final`.
    Csi {
        raw: &'a [u8],
        params: &'a [u8],
        intermediates: &'a [u8],
        final_byte: u8,
    },
    /// `ESC ] body (BEL | ESC \)`.
    Osc {
        raw: &'a [u8],
        body: &'a [u8],
    },
    /// `ESC P|X|^|_ body ESC \` (DCS, SOS, PM, APC).
    String {
        raw: &'a [u8],
        kind: u8,
        body: &'a [u8],
    },
    /// `ESC intermediates final`, e.g. `ESC ( B`, `ESC 7`, `ESC =`.
    Esc {
        raw: &'a [u8],
        intermediates: &'a [u8],
        final_byte: u8,
    },
}

impl<'a> Token<'a> {
    pub fn raw(&self) -> &'a [u8] {
        match self {
            Token::Text(raw) => raw,
            Token::Csi { raw, .. }
            | Token::Osc { raw, .. }
            | Token::String { raw, .. }
            | Token::Esc { raw, .. } => raw,
        }
    }
}

const ESC: u8 = 0x1b;
const BEL: u8 = 0x07;

/// Splits `input` into tokens. An escape sequence cut off by the end of the input is
/// dropped, so bytes written after the tokens can never be swallowed by it. A UTF-8
/// character cut off at the end is dropped for the same reason.
pub fn tokenize(input: &[u8]) -> Vec<Token<'_>> {
    let mut tokens = Vec::new();
    let mut at = 0;
    let mut text_start = 0;
    while at < input.len() {
        if input[at] != ESC {
            at += 1;
            continue;
        }
        if text_start < at {
            tokens.push(Token::Text(&input[text_start..at]));
        }
        match parse_escape(input, at) {
            Some((token, end)) => {
                tokens.push(token);
                at = end;
                text_start = end;
            }
            None => {
                // Incomplete: drop the rest.
                return tokens;
            }
        }
    }
    if text_start < input.len() {
        let text = trim_partial_utf8(&input[text_start..]);
        if !text.is_empty() {
            tokens.push(Token::Text(text));
        }
    }
    tokens
}

fn trim_partial_utf8(text: &[u8]) -> &[u8] {
    // Look back at most 3 bytes for the start of a multi-byte character.
    for back in 1..=text.len().min(4) {
        let byte = text[text.len() - back];
        if byte & 0b1100_0000 == 0b1000_0000 {
            continue; // continuation byte
        }
        let need = if byte & 0b1000_0000 == 0 {
            1
        } else if byte & 0b1110_0000 == 0b1100_0000 {
            2
        } else if byte & 0b1111_0000 == 0b1110_0000 {
            3
        } else if byte & 0b1111_1000 == 0b1111_0000 {
            4
        } else {
            1
        };
        return if back < need {
            &text[..text.len() - back]
        } else {
            text
        };
    }
    text
}

fn parse_escape(input: &[u8], start: usize) -> Option<(Token<'_>, usize)> {
    let kind = *input.get(start + 1)?;
    match kind {
        b'[' => {
            let mut at = start + 2;
            let params_start = at;
            while at < input.len() && (0x30..=0x3f).contains(&input[at]) {
                at += 1;
            }
            let params_end = at;
            while at < input.len() && (0x20..=0x2f).contains(&input[at]) {
                at += 1;
            }
            let inter_end = at;
            let final_byte = *input.get(at)?;
            if !(0x40..=0x7e).contains(&final_byte) {
                // Malformed: pass the ESC [ through as text so nothing is lost.
                return Some((Token::Text(&input[start..start + 2]), start + 2));
            }
            Some((
                Token::Csi {
                    raw: &input[start..=at],
                    params: &input[params_start..params_end],
                    intermediates: &input[params_end..inter_end],
                    final_byte,
                },
                at + 1,
            ))
        }
        b']' => {
            let body_start = start + 2;
            let mut at = body_start;
            loop {
                let byte = *input.get(at)?;
                if byte == BEL {
                    return Some((
                        Token::Osc {
                            raw: &input[start..=at],
                            body: &input[body_start..at],
                        },
                        at + 1,
                    ));
                }
                if byte == ESC && *input.get(at + 1)? == b'\\' {
                    return Some((
                        Token::Osc {
                            raw: &input[start..at + 2],
                            body: &input[body_start..at],
                        },
                        at + 2,
                    ));
                }
                at += 1;
            }
        }
        b'P' | b'X' | b'^' | b'_' => {
            let body_start = start + 2;
            let mut at = body_start;
            loop {
                let byte = *input.get(at)?;
                if byte == ESC && *input.get(at + 1)? == b'\\' {
                    return Some((
                        Token::String {
                            raw: &input[start..at + 2],
                            kind,
                            body: &input[body_start..at],
                        },
                        at + 2,
                    ));
                }
                at += 1;
            }
        }
        _ => {
            let mut at = start + 1;
            while at < input.len() && (0x20..=0x2f).contains(&input[at]) {
                at += 1;
            }
            let final_byte = *input.get(at)?;
            Some((
                Token::Esc {
                    raw: &input[start..=at],
                    intermediates: &input[start + 1..at],
                    final_byte,
                },
                at + 1,
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raws(input: &[u8]) -> Vec<Vec<u8>> {
        tokenize(input).iter().map(|t| t.raw().to_vec()).collect()
    }

    #[test]
    fn splits_text_and_sequences_losslessly() {
        let input = b"a\x1b[1;2Hb\x1b]8;;http://x\x1b\\c\x1b(Bd\x1b[?25l\x1bP$q\x1b\\";
        let tokens = tokenize(input);
        let joined: Vec<u8> = tokens.iter().flat_map(|t| t.raw().to_vec()).collect();
        assert_eq!(joined, input.to_vec());
        assert!(matches!(
            tokens[1],
            Token::Csi {
                final_byte: b'H',
                params: b"1;2",
                ..
            }
        ));
        assert!(matches!(
            tokens[3],
            Token::Osc {
                body: b"8;;http://x",
                ..
            }
        ));
        assert!(matches!(
            tokens[5],
            Token::Esc {
                intermediates: b"(",
                final_byte: b'B',
                ..
            }
        ));
        assert!(matches!(
            tokens[7],
            Token::Csi {
                params: b"?25",
                final_byte: b'l',
                ..
            }
        ));
        assert!(matches!(tokens[8], Token::String { kind: b'P', .. }));
    }

    #[test]
    fn osc_ends_on_bel_too() {
        assert_eq!(
            raws(b"\x1b]0;t\x07x"),
            vec![b"\x1b]0;t\x07".to_vec(), b"x".to_vec()]
        );
    }

    #[test]
    fn csi_intermediates_are_separate() {
        let tokens = tokenize(b"\x1b[!p\x1b[2 q");
        assert!(matches!(
            tokens[0],
            Token::Csi {
                intermediates: b"!",
                final_byte: b'p',
                ..
            }
        ));
        assert!(matches!(
            tokens[1],
            Token::Csi {
                params: b"2",
                intermediates: b" ",
                final_byte: b'q',
                ..
            }
        ));
    }

    #[test]
    fn incomplete_trailing_sequences_are_dropped() {
        assert_eq!(raws(b"ab\x1b[38;5"), vec![b"ab".to_vec()]);
        assert_eq!(raws(b"ab\x1b]8;;http"), vec![b"ab".to_vec()]);
        assert_eq!(raws(b"ab\x1b"), vec![b"ab".to_vec()]);
    }

    #[test]
    fn incomplete_trailing_utf8_is_dropped() {
        assert_eq!(raws("a日".as_bytes()), vec!["a日".as_bytes().to_vec()]);
        assert_eq!(raws(&"a日".as_bytes()[..3]), vec![b"a".to_vec()]);
        assert_eq!(raws(&"🔥".as_bytes()[..2]), Vec::<Vec<u8>>::new());
    }
}
