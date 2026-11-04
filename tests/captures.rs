//! Real `hc attach --observe` captures (fixtures/captures/), through the picker pipeline.

use herdr_app_pluck::color::Rgb;
use herdr_app_pluck::dim::dim;
use herdr_app_pluck::grid::decode;
use herdr_app_pluck::model::Action;
use herdr_app_pluck::render::{draw_hints, HintStyle};
use herdr_app_pluck::sanitize::sanitize;
use herdr_app_pluck::targets::find_targets;

const STYLE: HintStyle = HintStyle {
    hint_fg: Rgb::new(0, 0, 0),
    hint_bg: Rgb::new(255, 255, 0),
    match_fg: Rgb::new(255, 255, 255),
};

fn rows(bytes: &[u8], cols: u16, rows: u16) -> Vec<String> {
    decode(bytes, cols, rows)
        .unwrap()
        .cells
        .iter()
        .map(|row| {
            row.iter()
                .map(|c| c.text.as_str())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect()
}

#[test]
fn tokens_capture_sanitizes_dims_and_hints_without_moving_text() {
    let raw = std::fs::read("fixtures/captures/tokens-120x40.bin").unwrap();
    let clean = sanitize(&raw);
    // Input modes the observer sets at the end (focus events) don't survive.
    assert!(!clean.windows(8).any(|w| w == b"\x1b[?1004h"));

    let original = rows(&raw, 120, 40);
    assert_eq!(rows(&clean, 120, 40), original);
    let dimmed = dim(&clean, Rgb::new(90, 90, 90));
    assert_eq!(rows(&dimmed, 120, 40), original);

    let grid = decode(&clean, 120, 40).unwrap();
    let targets = find_targets(&grid, Action::Copy, &[]);
    let values: Vec<&str> = targets
        .assignments
        .assignments()
        .iter()
        .map(|a| a.text.as_str())
        .collect();
    assert_eq!(
        values,
        vec![
            "https://example.com/docs/getting-started",
            "file:///tmp/herdr-pluck/report.html",
            "src/picker/session.rs",
            "1a63023f",
            "123e4567-e89b-12d3-a456-426614174000",
            "192.168.40.12",
            "0xdeadBEEF",
            "deployment.apps/frontend",
            "9876543210123",
            "https://example.com/trailing",
        ]
    );
    // The duplicate path shares hint "d" at both occurrences.
    assert_eq!(targets.assignments.assignments()[2].occurrences.len(), 2);

    let mut drawn = dimmed.clone();
    drawn.extend_from_slice(draw_hints(&grid, &targets, "", STYLE).as_bytes());
    let after = rows(&drawn, 120, 40);
    assert_eq!(
        after[0],
        "url      attps://example.com/docs/getting-started"
    );
    assert_eq!(after[2], "path     drc/picker/session.rs");
    assert_eq!(after[9], "dup      drc/picker/session.rs");
    for (before, after) in original.iter().zip(&after) {
        assert_eq!(before.chars().count(), after.chars().count());
    }
}

#[test]
fn dimmed_capture_gives_every_text_cell_the_dim_color() {
    use ghostty_vt::style::StyleColor;
    use ghostty_vt::terminal::{Point, PointCoordinate};
    use ghostty_vt::{Terminal, TerminalOptions};

    let raw = std::fs::read("fixtures/captures/tokens-120x40.bin").unwrap();
    let dimmed = dim(&sanitize(&raw), Rgb::new(0x77, 0x81, 0x7d));
    let mut terminal = Terminal::new(TerminalOptions {
        cols: 120,
        rows: 40,
        max_scrollback: 0,
    })
    .unwrap();
    terminal.vt_write(&dimmed);
    for (x, y) in [(0u16, 0u32), (9, 0), (0, 11), (5, 11)] {
        let cell = terminal
            .grid_ref(Point::Active(PointCoordinate { x, y }))
            .unwrap();
        match cell.style().unwrap().fg_color {
            StyleColor::Rgb(rgb) => {
                assert_eq!((rgb.r, rgb.g, rgb.b), (0x77, 0x81, 0x7d), "cell {x},{y}")
            }
            other => panic!("cell {x},{y} fg is {other:?}, not the dim color"),
        }
    }
}
