//! `#rrggbb` colors passed from the mod's theme.

use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// SGR parameters selecting this color as foreground (`38;2;r;g;b`).
    pub fn fg(self) -> String {
        format!("38;2;{};{};{}", self.r, self.g, self.b)
    }

    /// SGR parameters selecting this color as background (`48;2;r;g;b`).
    pub fn bg(self) -> String {
        format!("48;2;{};{};{}", self.r, self.g, self.b)
    }
}

impl FromStr for Rgb {
    type Err = String;

    /// `#rrggbb` or `#rrggbbaa` (alpha ignored), `#` optional.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let hex = text.trim().trim_start_matches('#');
        if !(hex.len() == 6 || hex.len() == 8) || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(format!("expected #rrggbb, got {text:?}"));
        }
        let byte = |at: usize| u8::from_str_radix(&hex[at..at + 2], 16).unwrap();
        Ok(Self::new(byte(0), byte(2), byte(4)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_with_and_without_alpha() {
        assert_eq!("#0a1B2c".parse(), Ok(Rgb::new(10, 27, 44)));
        assert_eq!("0a1b2cff".parse(), Ok(Rgb::new(10, 27, 44)));
        assert!("#123".parse::<Rgb>().is_err());
        assert!("#zzzzzz".parse::<Rgb>().is_err());
    }
}
