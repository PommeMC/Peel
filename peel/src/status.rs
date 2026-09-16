//! Peel's stderr output: log lines, plus capture's status line, redrawn in
//! place when stderr is a terminal.

use std::fmt::Display;
use std::io::{self, IsTerminal};
use std::sync::Mutex;

/// Length of the status line currently drawn, 0 when there is none.
static STATUS_LEN: Mutex<usize> = Mutex::new(0);

/// Prints a `peel: ` log line with `format!` arguments, clearing the status
/// line first so they don't mix.
macro_rules! note {
    ($($arg:tt)*) => {
        $crate::status::note_line(format_args!($($arg)*))
    };
}
pub(crate) use note;

pub fn note_line(msg: impl Display) {
    let mut status_len = STATUS_LEN.lock().unwrap();
    if *status_len > 0 {
        eprint!("\r{:width$}\r", "", width = *status_len);
        *status_len = 0;
    }
    eprintln!("peel: {msg}");
}

/// Redraws the status line. Skipped when stderr is not a terminal, so
/// redirected logs stay free of carriage returns.
pub fn show(text: &str) {
    if !io::stderr().is_terminal() {
        return;
    }
    let mut status_len = STATUS_LEN.lock().unwrap();
    eprint!("\r{text:width$}", width = *status_len);
    *status_len = text.len();
}

/// A recording's progress, e.g. `12,345 packets, 402 ticks, 2.1 MB`.
/// `ticks` is `None` for protocols without a tick marker.
pub fn progress(packets: u64, ticks: Option<u64>, compressed_bytes: u64) -> String {
    let ticks = ticks.map_or_else(String::new, |ticks| format!(", {} ticks", thousands(ticks)));
    format!(
        "{} packets{ticks}, {}",
        thousands(packets),
        size(compressed_bytes)
    )
}

fn size(bytes: u64) -> String {
    if bytes < 1_000_000 {
        format!("{} KB", bytes.div_ceil(1000))
    } else {
        format!("{:.1} MB", bytes as f64 / 1e6)
    }
}

fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_text() {
        assert_eq!(
            progress(12_345, Some(402), 2_130_000),
            "12,345 packets, 402 ticks, 2.1 MB"
        );
        assert_eq!(progress(7, None, 1702), "7 packets, 2 KB");
        assert_eq!(
            progress(1_000_000, Some(999), 0),
            "1,000,000 packets, 999 ticks, 0 KB"
        );
    }
}
