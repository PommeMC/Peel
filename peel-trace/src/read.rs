use std::fmt;
use std::io::{self, Read};

use flate2::read::ZlibDecoder;

use crate::{
    Direction, Error, FORMAT_VERSION, Header, MAGIC, MAX_RECORD_LENGTH, Phase, Record, Result,
    varint,
};

/// Reads a trace of any format version up to [`FORMAT_VERSION`], yielding its
/// records in order. Iteration stops after the first error.
pub struct TraceReader<R: Read> {
    records: Source<R>,
    version: u16,
    header: Header,
    done: bool,
}

/// Where records are read from: the file itself up to version 1, a zlib
/// stream from version 2.
enum Source<R: Read> {
    Plain(R),
    Zlib(ZlibDecoder<R>),
}

/// Marks a zlib stream that ends without its trailer, which a plain
/// `UnexpectedEof` can't express: the varint reader takes that for a clean
/// end of the trace.
#[derive(Debug)]
pub(crate) struct IncompleteStream;

impl<R: Read> TraceReader<R> {
    pub fn new(mut inner: R) -> Result<Self> {
        let mut preamble = [0u8; 6];
        inner
            .read_exact(&mut preamble)
            .map_err(|err| match err.kind() {
                io::ErrorKind::UnexpectedEof => Error::NotATrace,
                _ => err.into(),
            })?;
        if &preamble[..4] != MAGIC {
            return Err(Error::NotATrace);
        }
        let version = u16::from_le_bytes([preamble[4], preamble[5]]);
        if version == 0 || version > FORMAT_VERSION {
            return Err(Error::UnsupportedVersion {
                found: version,
                newest: FORMAT_VERSION,
            });
        }
        let header = read_header(&mut inner)?;
        let records = match version {
            1 => Source::Plain(inner),
            _ => Source::Zlib(ZlibDecoder::new(inner)),
        };
        Ok(Self {
            records,
            version,
            header,
            done: false,
        })
    }

    pub fn format_version(&self) -> u16 {
        self.version
    }

    pub fn header(&self) -> &Header {
        &self.header
    }

    fn read_record(&mut self) -> Result<Option<Record>> {
        let Some(len) = varint::read_stream(&mut self.records)? else {
            return Ok(None);
        };
        if len > MAX_RECORD_LENGTH {
            return Err(Error::TooLarge("a record longer than a packet can be"));
        }
        read_body(&read_bytes(&mut self.records, len)?).map(Some)
    }
}

impl<R: Read> Iterator for TraceReader<R> {
    type Item = Result<Record>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        let item = self.read_record().transpose();
        if !matches!(item, Some(Ok(_))) {
            self.done = true;
        }
        item
    }
}

impl<R: Read> Read for Source<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Source::Plain(inner) => inner.read(buf),
            Source::Zlib(inner) => inner.read(buf).map_err(|err| match err.kind() {
                io::ErrorKind::UnexpectedEof => io::Error::other(IncompleteStream),
                _ => err,
            }),
        }
    }
}

impl fmt::Display for IncompleteStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("record stream ends without its trailer")
    }
}

impl std::error::Error for IncompleteStream {}

fn eof_is_truncated(err: io::Error) -> Error {
    match err.kind() {
        io::ErrorKind::UnexpectedEof => Error::Truncated,
        _ => err.into(),
    }
}

/// Reads exactly `len` bytes, growing the buffer as data arrives so a corrupt
/// length can't force a huge allocation up front.
fn read_bytes(r: &mut impl Read, len: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    r.take(len).read_to_end(&mut bytes)?;
    if bytes.len() as u64 != len {
        return Err(Error::Truncated);
    }
    Ok(bytes)
}

fn read_header(r: &mut impl Read) -> Result<Header> {
    let mut started = [0u8; 8];
    r.read_exact(&mut started).map_err(eof_is_truncated)?;
    let len = varint::read_stream(r)?.ok_or(Error::Truncated)?;
    let recorder = read_bytes(r, len)?;
    Ok(Header {
        started_at_unix_ms: u64::from_le_bytes(started),
        recorder: String::from_utf8(recorder)
            .map_err(|_| Error::Malformed("recorder is not UTF-8"))?,
    })
}

fn read_body(mut body: &[u8]) -> Result<Record> {
    let timestamp_us = varint::read_slice(&mut body)?;
    let protocol = u32::try_from(varint::read_slice(&mut body)?)
        .map_err(|_| Error::Malformed("protocol out of range"))? as i32;
    let [direction, phase, rest @ ..] = body else {
        return Err(Error::Malformed("record body too short"));
    };
    let direction = Direction::from_byte(*direction)?;
    let phase = Phase::from_byte(*phase)?;
    body = rest;
    let packet_id = u32::try_from(varint::read_slice(&mut body)?)
        .map_err(|_| Error::Malformed("packet id out of range"))?;
    Ok(Record {
        timestamp_us,
        protocol,
        direction,
        phase,
        packet_id,
        frame: body.to_vec(),
    })
}
