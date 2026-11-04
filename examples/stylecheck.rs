//! Debug: print the fg color the emulator gives a cell after replaying bytes.
//!   stylecheck <file> <cols> <rows> <x> <y> [--dim rrggbb]
use ghostty_vt::terminal::{Point, PointCoordinate};
use ghostty_vt::{Terminal, TerminalOptions};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mut bytes = std::fs::read(&a[1]).unwrap();
    let (cols, rows, x, y): (u16, u16, u16, u32) = (
        a[2].parse().unwrap(),
        a[3].parse().unwrap(),
        a[4].parse().unwrap(),
        a[5].parse().unwrap(),
    );
    bytes = herdr_app_pluck::sanitize::sanitize(&bytes);
    if a.iter().any(|s| s == "--dim") {
        bytes = herdr_app_pluck::dim::dim(&bytes, "#77817d".parse().unwrap());
    }
    let mut t = Terminal::new(TerminalOptions {
        cols,
        rows,
        max_scrollback: 0,
    })
    .unwrap();
    t.vt_write(&bytes);
    let r = t.grid_ref(Point::Active(PointCoordinate { x, y })).unwrap();
    println!("{:?}", r.style().unwrap().fg_color);
}
