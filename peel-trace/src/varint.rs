//! LEB128 varints, the same encoding the Minecraft protocol uses.

use std::io::{self, Read};

pub fn write(buf: &mut Vec<u8>, mut v: u64) {
    loop {
        let byte = (v & 0x7F) as u8;
        v >>= 7;
        if v == 0 {
            buf.push(byte);
            return;
        }
        buf.push(byte | 0x80);
    }
}

/// Reads a varint from a stream. `Ok(None)` means a clean end of stream
/// before the first byte.
pub fn read_stream(r: &mut impl Read) -> crate::Result<Option<u64>> {
    let mut v = 0u64;
    for i in 0..10 {
        let mut byte = [0u8];
        if let Err(err) = r.read_exact(&mut byte) {
            return match err.kind() {
                io::ErrorKind::UnexpectedEof if i == 0 => Ok(None),
                io::ErrorKind::UnexpectedEof => Err(crate::Error::Truncated),
                _ => Err(err.into()),
            };
        }
        v |= u64::from(byte[0] & 0x7F) << (i * 7);
        if byte[0] & 0x80 == 0 {
            return Ok(Some(v));
        }
    }
    Err(crate::Error::Malformed("varint too long"))
}

/// Reads a varint from a slice, advancing it.
pub fn read_slice(bytes: &mut &[u8]) -> crate::Result<u64> {
    read_stream(bytes)?.ok_or(crate::Error::Malformed("record body too short"))
}
