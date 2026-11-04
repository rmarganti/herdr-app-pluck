//! Captures what a terminal shows with `hc attach --observe`: herdr-core answers with a
//! full repaint of the live screen (reset, modes, cursor moves, SGR, text).

use anyhow::{bail, Context, Result};
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy)]
pub struct CaptureLimits {
    /// Give up if the first byte takes longer than this.
    pub first_byte: Duration,
    /// After the first byte, stop at a gap this long.
    pub quiet: Duration,
    /// Stop this long after the first byte even if output never pauses.
    pub total: Duration,
    pub max_bytes: usize,
}

pub const LIMITS: CaptureLimits = CaptureLimits {
    first_byte: Duration::from_secs(5),
    quiet: Duration::from_millis(120),
    total: Duration::from_millis(400),
    max_bytes: 4 * 1024 * 1024,
};

pub fn capture(terminal: &str, limits: CaptureLimits) -> Result<Vec<u8>> {
    // stdin stays piped and is never written: closing it may end the session.
    // --observe can neither type nor resize.
    let mut child = Command::new(crate::hc::program())
        .args(["attach", "--observe", "--history", "0", terminal])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("failed to run hc attach; is hc on PATH?")?;
    let mut stdout = child.stdout.take().context("hc attach has no stdout")?;

    let (sender, receiver) = mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || {
        let mut buffer = vec![0u8; 65536];
        loop {
            match stdout.read(&mut buffer) {
                Ok(0) | Err(_) => return,
                Ok(n) => {
                    if sender.send(buffer[..n].to_vec()).is_err() {
                        return;
                    }
                }
            }
        }
    });

    let result = collect(&receiver, limits);
    // Only our observer client: never the terminal or the core.
    let _ = child.kill();
    let _ = child.wait();
    result
}

fn collect(receiver: &mpsc::Receiver<Vec<u8>>, limits: CaptureLimits) -> Result<Vec<u8>> {
    let started = Instant::now();
    let mut captured = Vec::new();
    let mut first: Option<Instant> = None;
    loop {
        let wait = match first {
            None => limits.first_byte.saturating_sub(started.elapsed()),
            Some(first) => limits
                .quiet
                .min(limits.total.saturating_sub(first.elapsed())),
        };
        match receiver.recv_timeout(wait) {
            Ok(chunk) => {
                captured.extend_from_slice(&chunk);
                first.get_or_insert_with(Instant::now);
                if captured.len() > limits.max_bytes {
                    bail!("the terminal's repaint exceeded {} bytes", limits.max_bytes);
                }
            }
            Err(RecvTimeoutError::Timeout) | Err(RecvTimeoutError::Disconnected) => break,
        }
        if first.is_some_and(|first| first.elapsed() >= limits.total) {
            break;
        }
    }
    if captured.is_empty() {
        bail!("no repaint received from the terminal");
    }
    Ok(captured)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FAST: CaptureLimits = CaptureLimits {
        first_byte: Duration::from_millis(200),
        quiet: Duration::from_millis(30),
        total: Duration::from_millis(150),
        max_bytes: 10,
    };

    #[test]
    fn stops_at_a_quiet_gap() {
        let (sender, receiver) = mpsc::channel();
        sender.send(b"abc".to_vec()).unwrap();
        sender.send(b"de".to_vec()).unwrap();
        let keep = sender; // never disconnects: the quiet gap must end it
        assert_eq!(collect(&receiver, FAST).unwrap(), b"abcde");
        drop(keep);
    }

    #[test]
    fn no_first_byte_is_an_error() {
        let (_sender, receiver) = mpsc::channel::<Vec<u8>>();
        assert!(collect(&receiver, FAST).is_err());
    }

    #[test]
    fn too_much_output_is_an_error() {
        let (sender, receiver) = mpsc::channel();
        sender.send(vec![b'x'; 11]).unwrap();
        assert!(collect(&receiver, FAST).is_err());
    }

    #[test]
    fn constant_output_is_cut_after_the_total() {
        let (sender, receiver) = mpsc::channel();
        let feeder = std::thread::spawn(move || {
            for _ in 0..100 {
                if sender.send(b"x".to_vec()).is_err() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        });
        let limits = CaptureLimits {
            max_bytes: 1000,
            ..FAST
        };
        let started = Instant::now();
        let captured = collect(&receiver, limits).unwrap();
        assert!(started.elapsed() < Duration::from_millis(400));
        assert!(captured.len() < 30);
        drop(receiver);
        feeder.join().unwrap();
    }
}
