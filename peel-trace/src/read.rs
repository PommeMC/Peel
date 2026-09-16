use std::io::{self, Read};

use crate::{Direction, Error, FORMAT_VERSION, Header, MAGIC, Phase, Record, Result, varint};

/// Reads a trace of any format version up to [`FORMAT_VERSION`], yielding its
/// records in order. Iteration stops after the first error.
pub struct TraceReader<R: Read> {
    inner: R,
    version: u16,
    header: Header,
    done: bool,
}

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
        let header = read_header_v1(&mut inner)?;
        Ok(Self {
            inner,
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
        let Some(len) = varint::read_stream(&mut self.inner)? else {
            return Ok(None);
        };
        let body = read_bytes(&mut self.inner, len)?;
        match self.version {
            1 => record_v1(&body).map(Some),
            _ => unreachable!("version checked in new"),
        }
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

fn read_header_v1(r: &mut impl Read) -> Result<Header> {
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

fn record_v1(mut body: &[u8]) -> Result<Record> {
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
