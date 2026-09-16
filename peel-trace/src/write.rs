use std::io::{self, Write};

use crate::{FORMAT_VERSION, Header, MAGIC, Record, varint};

/// Writes a trace. Records are appended as they arrive with no trailer, so a
/// trace cut off mid-capture still reads up to its last whole record.
pub struct TraceWriter<W: Write> {
    inner: W,
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
        Ok(Self { inner })
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
        self.inner.write_all(&len)?;
        self.inner.write_all(&fields)?;
        self.inner.write_all(&record.frame)
    }

    pub fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }

    pub fn into_inner(mut self) -> io::Result<W> {
        self.inner.flush()?;
        Ok(self.inner)
    }
}
