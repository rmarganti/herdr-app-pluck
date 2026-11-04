//! Pixel diff for the exactness harness:
//!   pngdiff a.png b.png [--glyphs] [--crop x0,y0,x1,y1]
//!   pngdiff --find rrggbb a.png        bounding box of pixels exactly that color
//!
//! Prints JSON: size, differing pixels and their bounding box. With `--glyphs`, compares
//! binarized "ink" masks instead (pixel far from the image's most common color), which
//! is what must match when only colors changed (dimming).

use std::collections::HashMap;
use std::fs::File;

struct Image {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

fn load(path: &str) -> Image {
    let mut decoder = png::Decoder::new(File::open(path).expect("open png"));
    decoder.set_transformations(
        png::Transformations::normalize_to_color8() | png::Transformations::ALPHA,
    );
    let mut reader = decoder.read_info().expect("png header");
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).expect("png frame");
    let channels = info.color_type.samples();
    let mut rgba = Vec::with_capacity((info.width * info.height * 4) as usize);
    for px in buf[..info.buffer_size()].chunks(channels) {
        match channels {
            4 => rgba.extend_from_slice(px),
            3 => rgba.extend_from_slice(&[px[0], px[1], px[2], 255]),
            2 => rgba.extend_from_slice(&[px[0], px[0], px[0], px[1]]),
            _ => rgba.extend_from_slice(&[px[0], px[0], px[0], 255]),
        }
    }
    Image {
        width: info.width,
        height: info.height,
        rgba,
    }
}

fn ink_mask(image: &Image) -> Vec<bool> {
    let mut counts: HashMap<[u8; 3], usize> = HashMap::new();
    for px in image.rgba.chunks(4) {
        *counts.entry([px[0], px[1], px[2]]).or_default() += 1;
    }
    let bg = counts
        .into_iter()
        .max_by_key(|(_, n)| *n)
        .map(|(c, _)| c)
        .unwrap_or([0; 3]);
    image
        .rgba
        .chunks(4)
        .map(|px| {
            let d: i32 = (0..3).map(|i| (px[i] as i32 - bg[i] as i32).abs()).sum();
            d > 60
        })
        .collect()
}

fn value_of<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let glyphs = args.iter().any(|a| a == "--glyphs");
    let find = value_of(&args, "--find");
    let crop: Option<Vec<u32>> =
        value_of(&args, "--crop").map(|c| c.split(',').map(|n| n.parse().expect("crop")).collect());
    let skip: Vec<&str> = [find, value_of(&args, "--crop")]
        .into_iter()
        .flatten()
        .collect();
    let files: Vec<&String> = args[1..]
        .iter()
        .filter(|a| !a.starts_with("--") && !skip.contains(&a.as_str()))
        .collect();
    if let Some(hex) = find {
        let c: Vec<u8> = (0..3)
            .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap())
            .collect();
        let image = load(files[0]);
        let (mut n, mut x0, mut y0, mut x1, mut y1) = (0usize, u32::MAX, u32::MAX, 0u32, 0u32);
        for (i, px) in image.rgba.chunks(4).enumerate() {
            if px[..3] == c[..] {
                let (x, y) = (i as u32 % image.width, i as u32 / image.width);
                n += 1;
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
        if n == 0 {
            println!(r#"{{"pixels":0,"bbox":null}}"#);
        } else {
            println!(r#"{{"pixels":{n},"bbox":[{x0},{y0},{x1},{y1}]}}"#);
        }
        return;
    }
    let (a, b) = (load(files[0]), load(files[1]));
    if (a.width, a.height) != (b.width, b.height) {
        println!(
            r#"{{"error":"size differs","a":[{},{}],"b":[{},{}]}}"#,
            a.width, a.height, b.width, b.height
        );
        std::process::exit(2);
    }
    let differs: Vec<bool> = if glyphs {
        // Ink in one image with no ink within 1 px in the other. Antialiased edges of the
        // same glyph in a new color stay within 1 px; a glyph that moved by even one cell
        // (~20 px) does not.
        let (ma, mb) = (ink_mask(&a), ink_mask(&b));
        let (w, h) = (a.width as i64, a.height as i64);
        let near = |m: &[bool], i: usize| {
            let (x, y) = (i as i64 % w, i as i64 / w);
            (-1..=1).any(|dy| {
                (-1..=1).any(|dx| {
                    let (nx, ny) = (x + dx, y + dy);
                    nx >= 0 && ny >= 0 && nx < w && ny < h && m[(ny * w + nx) as usize]
                })
            })
        };
        (0..ma.len())
            .map(|i| (ma[i] && !near(&mb, i)) || (mb[i] && !near(&ma, i)))
            .collect()
    } else {
        a.rgba
            .chunks(4)
            .zip(b.rgba.chunks(4))
            .map(|(x, y)| x != y)
            .collect()
    };
    let (mut count, mut x0, mut y0, mut x1, mut y1) = (0usize, u32::MAX, u32::MAX, 0u32, 0u32);
    let inside = |x: u32, y: u32| match &crop {
        Some(c) => x >= c[0] && y >= c[1] && x <= c[2] && y <= c[3],
        None => true,
    };
    for (i, d) in differs.iter().enumerate() {
        let (x, y) = (i as u32 % a.width, i as u32 / a.width);
        if *d && inside(x, y) {
            count += 1;
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
    }
    let bbox = if count == 0 {
        "null".to_string()
    } else {
        format!("[{x0},{y0},{x1},{y1}]")
    };
    println!(
        r#"{{"width":{},"height":{},"mode":"{}","crop":{},"diff_pixels":{count},"bbox":{bbox}}}"#,
        a.width,
        a.height,
        if glyphs { "glyphs" } else { "exact" },
        crop.map(|c| format!("{c:?}"))
            .unwrap_or_else(|| "null".into())
    );
}
