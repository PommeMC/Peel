//! The Peel trace file format.
//!
//! A trace is a header followed by one record per packet frame. Records hold
//! raw uncompressed frames, never decoded fields, so names and fields are
//! computed on read and decoder fixes apply to old traces too. Shared with
//! the Pomme client's built-in recorder, so the API stays small and stable.
//!
//! Layout (integers little-endian, varints as in the Minecraft protocol,
//! strings as a varint byte length plus UTF-8). Version 2 compresses the
//! records into one zlib stream; version 1 stored them as is.
//!
//! ```text
//! file    = "PEEL" u16:format_version header records
//! header  = u64:started_at_unix_ms string:recorder
//! records = zlib(record*)            (version 1: record*)
//! record  = varint:len body[len]
//! body    = varint:timestamp_us varint:protocol u8:direction u8:phase varint:packet_id frame[rest]
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
pub const FORMAT_VERSION: u16 = 2;

/// The longest record a reader accepts, `CompressionDecoder`'s maximum
/// uncompressed packet length. Traces are shared files, so a record that
/// claims more than a packet can hold is refused rather than allocated.
pub const MAX_RECORD_LENGTH: u64 = 0x80_0000;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Serverbound = 0,
    Clientbound = 1,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
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
    /// A record, or the whole trace, is bigger than the reader accepts.
    TooLarge(&'static str),
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
            Error::TooLarge(what) => write!(f, "trace is too large to read: {what}"),
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
        if err
            .get_ref()
            .is_some_and(|inner| inner.is::<read::IncompleteStream>())
        {
            return Error::Truncated;
        }
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
    use std::cell::RefCell;
    use std::io::Write;
    use std::rc::Rc;

    use flate2::Compression;
    use flate2::write::ZlibEncoder;

    use super::*;

    /// A version 1 trace from the last version 1 writer, holding the first
    /// two of [`records`] under [`header`]. It must keep opening.
    const V1_TRACE: &[u8] = &[
        0x50, 0x45, 0x45, 0x4c, 0x01, 0x00, 0x00, 0xa2, 0xb5, 0x88, 0xa0, 0x01, 0x00, 0x00, 0x0a,
        0x70, 0x65, 0x65, 0x6c, 0x20, 0x30, 0x2e, 0x31, 0x2e, 0x30, 0x0a, 0x00, 0x88, 0x06, 0x00,
        0x00, 0x00, 0x00, 0x01, 0x02, 0x03, 0x0f, 0xce, 0xc2, 0xf1, 0x05, 0xc5, 0x82, 0x80, 0x80,
        0x04, 0x01, 0x04, 0xac, 0x02, 0xac, 0x02,
    ];

    fn header() -> Header {
        Header {
            started_at_unix_ms: 1_789_000_000_000,
            recorder: "peel 0.1.0".into(),
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

    /// A writer whose bytes stay readable while the trace writer holds it.
    #[derive(Clone, Default)]
    struct SharedBuf(Rc<RefCell<Vec<u8>>>);

    impl Write for SharedBuf {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.borrow_mut().write(buf)
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn encode(records: &[Record]) -> Vec<u8> {
        let mut writer = TraceWriter::new(Vec::new(), &header()).unwrap();
        for record in records {
            writer.write(record).unwrap();
        }
        writer.into_inner().unwrap()
    }

    fn read_all(bytes: &[u8]) -> Vec<Result<Record>> {
        TraceReader::new(bytes).unwrap().collect()
    }

    #[test]
    fn round_trip() {
        let bytes = encode(&records());
        assert_eq!(&bytes[..6], b"PEEL\x02\x00");
        let reader = TraceReader::new(bytes.as_slice()).unwrap();
        assert_eq!(reader.format_version(), FORMAT_VERSION);
        assert_eq!(reader.header(), &header());
        let read: Vec<Record> = reader.collect::<Result<_>>().unwrap();
        assert_eq!(read, records());
    }

    #[test]
    fn a_record_longer_than_a_packet_is_refused() {
        let header = header();
        let mut trace = Vec::from(MAGIC);
        trace.extend(FORMAT_VERSION.to_le_bytes());
        trace.extend(header.started_at_unix_ms.to_le_bytes());
        varint::write(&mut trace, header.recorder.len() as u64);
        trace.extend(header.recorder.as_bytes());
        let mut length = Vec::new();
        varint::write(&mut length, MAX_RECORD_LENGTH + 1);
        let mut stream = ZlibEncoder::new(Vec::new(), Compression::default());
        stream.write_all(&length).unwrap();
        trace.extend(stream.finish().unwrap());
        let mut reader = TraceReader::new(&trace[..]).unwrap();
        assert!(matches!(reader.next(), Some(Err(Error::TooLarge(_)))));
    }

    #[test]
    fn reads_version_1() {
        let reader = TraceReader::new(V1_TRACE).unwrap();
        assert_eq!(reader.format_version(), 1);
        assert_eq!(reader.header(), &header());
        let read: Vec<Record> = reader.collect::<Result<_>>().unwrap();
        assert_eq!(read, records()[..2]);
    }

    #[test]
    fn compresses_records() {
        let frame: Vec<u8> = (0..16_000u32).map(|i| (i % 64) as u8).collect();
        let records: Vec<Record> = (0..20)
            .map(|i| Record {
                timestamp_us: i * 50_000,
                frame: frame.clone(),
                ..records()[1].clone()
            })
            .collect();
        let raw: usize = records.iter().map(|r| r.frame.len()).sum();
        let bytes = encode(&records);
        assert!(bytes.len() * 4 < raw, "{} of {raw} bytes", bytes.len());
        let read: Vec<Record> = read_all(&bytes).into_iter().collect::<Result<_>>().unwrap();
        assert_eq!(read, records);
    }

    #[test]
    fn rejects_future_version() {
        let mut bytes = encode(&[]);
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
        let mut bytes = encode(&[]);
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
    fn cut_off_after_a_flush_keeps_flushed_records() {
        let shared = SharedBuf::default();
        let mut writer = TraceWriter::new(shared.clone(), &header()).unwrap();
        for record in &records()[..2] {
            writer.write(record).unwrap();
        }
        writer.flush().unwrap();
        let flushed = shared.0.borrow().clone();
        writer.write(&records()[2]).unwrap();

        let read = read_all(&flushed);
        assert_eq!(read.len(), 3, "{read:?}");
        assert_eq!(read[0].as_ref().unwrap(), &records()[0]);
        assert_eq!(read[1].as_ref().unwrap(), &records()[1]);
        assert!(matches!(read[2], Err(Error::Truncated)), "{:?}", read[2]);
    }

    #[test]
    fn missing_trailer_is_truncated() {
        let mut bytes = encode(&records());
        bytes.pop();
        let read = read_all(&bytes);
        assert!(
            matches!(read.last(), Some(Err(Error::Truncated))),
            "{read:?}"
        );
    }

    #[test]
    fn version_1_cut_off_mid_record_is_truncated() {
        let read = read_all(&V1_TRACE[..V1_TRACE.len() - 1]);
        assert_eq!(read[0].as_ref().unwrap(), &records()[0]);
        assert!(matches!(read[1], Err(Error::Truncated)), "{:?}", read[1]);
    }

    #[test]
    fn rejects_unknown_enum_bytes() {
        // Version 1's header is 25 bytes, then the first record's length,
        // timestamp and 2-byte protocol varint.
        let direction_at = 25 + 1 + 1 + 2;
        for (offset, what) in [(0, "unknown direction"), (1, "unknown phase")] {
            let mut bad = V1_TRACE.to_vec();
            bad[direction_at + offset] = 9;
            let read = read_all(&bad);
            assert!(
                matches!(read[0], Err(Error::Malformed(m)) if m == what),
                "{what}"
            );
        }
    }
}
