use std::io::{self, Write};

use flate2::Compression;
use flate2::write::ZlibEncoder;

use crate::{FORMAT_VERSION, Header, MAGIC, Record, varint};

/// Writes a trace. Records are appended to one zlib stream as they arrive;
/// [`flush`](Self::flush) makes everything written so far readable, so a
/// trace cut off mid-capture still reads up to its last flush.
pub struct TraceWriter<W: Write> {
    records: ZlibEncoder<W>,
}

impl<W: Write> TraceWriter<W> {
    pub fn new(mut inner: W, header: &Header) -> io::Result<Self> {
        let mut buf = Vec::new();
        buf.extend_from_slice(MAGIC);
        buf.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        buf.extend_from_slice(&header.started_at_unix_ms.to_le_bytes());
        varint::write(&mut buf, header.recorder.len() as u64);
        buf.extend_from_slice(header.recorder.as_bytes());
        inner.write_all(&buf)?;
        Ok(Self {
            records: ZlibEncoder::new(inner, Compression::default()),
        })
    }

    pub fn write(&mut self, record: &Record) -> io::Result<()> {
        let mut fields = Vec::with_capacity(16);
        varint::write(&mut fields, record.timestamp_us);
        varint::write(&mut fields, u64::from(record.protocol as u32));
        fields.push(record.direction as u8);
        fields.push(record.phase as u8);
        varint::write(&mut fields, u64::from(record.packet_id));

        let mut len = Vec::with_capacity(4);
        varint::write(&mut len, (fields.len() + record.frame.len()) as u64);
        self.records.write_all(&len)?;
        self.records.write_all(&fields)?;
        self.records.write_all(&record.frame)
    }

    pub fn flush(&mut self) -> io::Result<()> {
        self.records.flush()
    }

    /// Compressed bytes produced so far, excluding the header.
    pub fn compressed_len(&self) -> u64 {
        self.records.total_out()
    }

    /// Ends the record stream and returns the flushed writer.
    pub fn into_inner(self) -> io::Result<W> {
        let mut inner = self.records.finish()?;
        inner.flush()?;
        Ok(inner)
    }
}
