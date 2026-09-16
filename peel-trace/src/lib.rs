//! The Peel trace file format.
//!
//! A trace is a header followed by one record per packet frame. Records hold
//! raw uncompressed frames, never decoded fields, so names and fields are
//! computed on read and decoder fixes apply to old traces too. Shared with
//! the Pomme client's built-in recorder, so the API stays small and stable.
//!
//! Layout (format version 1, integers little-endian, varints as in the
//! Minecraft protocol, strings as a varint byte length plus UTF-8):
//!
//! ```text
//! file   = "PEEL" u16:format_version header record*
//! header = u64:started_at_unix_ms string:recorder
//! record = varint:len body[len]
//! body   = varint:timestamp_us varint:protocol u8:direction u8:phase varint:packet_id frame[rest]
//! ```

mod read;
mod varint;
mod write;

use std::fmt;

pub use read::TraceReader;
use serde::{Deserialize, Serialize};
pub use write::TraceWriter;

const MAGIC: &[u8; 4] = b"PEEL";

/// The format version writers produce. Readers open this and every older
/// version.
pub const FORMAT_VERSION: u16 = 1;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Serverbound = 0,
    Clientbound = 1,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Handshake = 0,
    Status = 1,
    Login = 2,
    Configuration = 3,
    Game = 4,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Header {
    pub started_at_unix_ms: u64,
    /// What produced the trace, e.g. `peel 0.1.0`.
    pub recorder: String,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Record {
    /// Microseconds since [`Header::started_at_unix_ms`].
    pub timestamp_us: u64,
    /// The protocol number from the connection's handshake.
    pub protocol: i32,
    pub direction: Direction,
    /// The phase the receiver decoded this frame in.
    pub phase: Phase,
    /// The packet id, as encoded at the start of `frame`.
    pub packet_id: u32,
    /// The whole uncompressed packet: id varint then fields, without the
    /// frame length prefix or compression header.
    pub frame: Vec<u8>,
}

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    NotATrace,
    UnsupportedVersion {
        found: u16,
        newest: u16,
    },
    /// The file ends partway through a record, e.g. a capture that was
    /// killed. Records before it are intact.
    Truncated,
    Malformed(&'static str),
}

pub type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(err) => write!(f, "{err}"),
            Error::NotATrace => f.write_str("not a Peel trace (bad magic)"),
            Error::UnsupportedVersion { found, newest } => write!(
                f,
                "trace format version {found} is newer than this build supports \
                 (up to {newest}); update Peel to open it"
            ),
            Error::Truncated => f.write_str("trace ends partway through a record"),
            Error::Malformed(what) => write!(f, "malformed trace: {what}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Error::Io(err)
    }
}

impl Direction {
    fn from_byte(b: u8) -> Result<Self> {
        match b {
            0 => Ok(Direction::Serverbound),
            1 => Ok(Direction::Clientbound),
            _ => Err(Error::Malformed("unknown direction")),
        }
    }
}

impl Phase {
    fn from_byte(b: u8) -> Result<Self> {
        match b {
            0 => Ok(Phase::Handshake),
            1 => Ok(Phase::Status),
            2 => Ok(Phase::Login),
            3 => Ok(Phase::Configuration),
            4 => Ok(Phase::Game),
            _ => Err(Error::Malformed("unknown phase")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header() -> Header {
        Header {
            started_at_unix_ms: 1_789_000_000_000,
            recorder: "peel test".into(),
        }
    }

    fn records() -> Vec<Record> {
        vec![
            Record {
                timestamp_us: 0,
                protocol: 776,
                direction: Direction::Serverbound,
                phase: Phase::Handshake,
                packet_id: 0,
                frame: vec![0, 1, 2, 3],
            },
            Record {
                timestamp_us: 12_345_678,
                // A snapshot protocol: a 5-byte varint.
                protocol: 0x4000_0145,
                direction: Direction::Clientbound,
                phase: Phase::Game,
                packet_id: 300,
                frame: vec![0xac, 0x02],
            },
            Record {
                timestamp_us: u64::MAX,
                protocol: 763,
                direction: Direction::Clientbound,
                phase: Phase::Configuration,
                packet_id: 0,
                frame: vec![],
            },
        ]
    }

    fn encode(header: &Header, records: &[Record]) -> Vec<u8> {
        let mut writer = TraceWriter::new(Vec::new(), header).unwrap();
        for record in records {
            writer.write(record).unwrap();
        }
        writer.into_inner().unwrap()
    }

    #[test]
    fn round_trip() {
        let bytes = encode(&header(), &records());
        assert_eq!(&bytes[..6], b"PEEL\x01\x00");
        let reader = TraceReader::new(bytes.as_slice()).unwrap();
        assert_eq!(reader.format_version(), FORMAT_VERSION);
        assert_eq!(reader.header(), &header());
        let read: Vec<Record> = reader.collect::<Result<_>>().unwrap();
        assert_eq!(read, records());
    }

    #[test]
    fn rejects_future_version() {
        let mut bytes = encode(&header(), &[]);
        bytes[4..6].copy_from_slice(&(FORMAT_VERSION + 1).to_le_bytes());
        let err = TraceReader::new(bytes.as_slice()).err().unwrap();
        assert!(
            matches!(err, Error::UnsupportedVersion { found, newest }
                if found == FORMAT_VERSION + 1 && newest == FORMAT_VERSION),
            "{err:?}"
        );
        assert!(err.to_string().contains("update Peel"));
    }

    #[test]
    fn rejects_bad_magic() {
        let mut bytes = encode(&header(), &[]);
        bytes[0] = b'X';
        assert!(matches!(
            TraceReader::new(bytes.as_slice()),
            Err(Error::NotATrace)
        ));
        assert!(matches!(
            TraceReader::new(&b"PE"[..]),
            Err(Error::NotATrace)
        ));
    }

    #[test]
    fn truncated_tail_keeps_earlier_records() {
        let mut bytes = encode(&header(), &records());
        bytes.pop();
        let mut reader = TraceReader::new(bytes.as_slice()).unwrap();
        assert_eq!(reader.next().unwrap().unwrap(), records()[0]);
        assert_eq!(reader.next().unwrap().unwrap(), records()[1]);
        assert!(matches!(reader.next(), Some(Err(Error::Truncated))));
        assert!(reader.next().is_none());
    }

    #[test]
    fn rejects_unknown_enum_bytes() {
        let bytes = encode(&header(), &records()[..1]);
        // Header, then record length, timestamp and a 2-byte protocol varint.
        let direction_at = encode(&header(), &[]).len() + 1 + 1 + 2;
        for (offset, what) in [(0, "unknown direction"), (1, "unknown phase")] {
            let mut bad = bytes.clone();
            bad[direction_at + offset] = 9;
            let mut reader = TraceReader::new(bad.as_slice()).unwrap();
            assert!(
                matches!(reader.next(), Some(Err(Error::Malformed(m))) if m == what),
                "{what}"
            );
        }
    }
}
