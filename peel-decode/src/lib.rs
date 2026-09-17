//! Turns trace records into what users see: packet names, phases, the tick
//! timeline and decoded fields. Shared by `peel dump` and the browser viewer,
//! so both always agree.

pub mod diff;
pub mod fields;
pub mod names;
#[cfg(any(test, feature = "test-util"))]
pub mod test_util;
mod timeline;

use std::io::Read;

use peel_trace::{Header, Record, TraceReader};
pub use timeline::{Group, GroupKey, GroupKind, Grouping, Packet, Timeline, WINDOW_US};

/// A whole trace read into memory.
pub struct Loaded {
    pub format_version: u16,
    pub header: Header,
    pub records: Vec<Record>,
    /// Why reading stopped early, e.g. a trace cut off mid-capture. The
    /// records before it are intact.
    pub error: Option<peel_trace::Error>,
}

pub fn load(trace: impl Read) -> peel_trace::Result<Loaded> {
    let mut reader = TraceReader::new(trace)?;
    let format_version = reader.format_version();
    let header = reader.header().clone();
    let mut records = Vec::new();
    let mut error = None;
    for record in &mut reader {
        match record {
            Ok(record) => records.push(record),
            Err(err) => error = Some(err),
        }
    }
    Ok(Loaded {
        format_version,
        header,
        records,
        error,
    })
}

impl Loaded {
    pub fn timeline(&self) -> Timeline<'_> {
        Timeline::build(self.format_version, &self.header, &self.records)
    }

    /// The fields of the record at `index`.
    pub fn fields(&self, index: usize) -> Option<fields::Decoded> {
        self.records.get(index).map(fields::decode)
    }

    /// Why reading stopped before the end of the trace, if it did.
    pub fn warning(&self) -> Option<String> {
        self.error
            .as_ref()
            .map(|err| format!("stopped after {} records: {err}", self.records.len()))
    }
}
