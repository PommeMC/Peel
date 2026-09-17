//! Reads the primitive types of vanilla's `FriendlyByteBuf` and `ByteBufCodecs`
//! from a packet frame. Vanilla's length limits aren't enforced: Peel shows
//! what was sent.

use std::fmt;

use pomme_protocol::wire::{read_lp_vec3, read_varint};

#[derive(Clone, Debug, PartialEq)]
pub struct Error {
    /// Offset into the frame where the read failed.
    pub at: usize,
    pub message: String,
    /// Decoding reached a codec Peel hasn't ported yet, rather than misreading.
    pub unsupported: bool,
}

impl Error {
    pub fn misread(at: usize, message: impl Into<String>) -> Self {
        Self {
            at,
            message: message.into(),
            unsupported: false,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} at byte {}", self.message, self.at)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone)]
pub struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlockPos {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl<'a> Reader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn remaining(&self) -> usize {
        self.bytes.len() - self.pos
    }

    pub fn error(&self, message: impl Into<String>) -> Error {
        Error::misread(self.pos, message)
    }

    /// Stops at a value whose codec Peel doesn't decode yet.
    pub fn unsupported(&self, what: impl fmt::Display) -> Error {
        Error {
            unsupported: true,
            ..self.error(format!("{what} isn't decoded yet"))
        }
    }

    pub fn bytes(&mut self, len: usize, what: &str) -> Result<&'a [u8]> {
        if len > self.remaining() {
            return Err(self.error(format!(
                "{what} needs {len} bytes, {} left",
                self.remaining()
            )));
        }
        let bytes = &self.bytes[self.pos..self.pos + len];
        self.pos += len;
        Ok(bytes)
    }

    /// `len` items of `width` bytes each, refusing a length whose size
    /// doesn't fit rather than wrapping. `usize` is 32-bit in the viewer's
    /// WebAssembly build, where a wrapped length would read the wrong bytes.
    pub fn sized(&mut self, len: usize, width: usize, what: &str) -> Result<&'a [u8]> {
        let size = len
            .checked_mul(width)
            .ok_or_else(|| self.error(format!("{what} is longer than any frame")))?;
        self.bytes(size, what)
    }

    /// Up to `len` of the bytes ahead, without reading them.
    pub fn upcoming(&self, len: usize) -> &'a [u8] {
        &self.bytes[self.pos..self.pos.saturating_add(len).min(self.bytes.len())]
    }

    pub fn rest(&mut self) -> &'a [u8] {
        let rest = &self.bytes[self.pos..];
        self.pos = self.bytes.len();
        rest
    }

    fn array<const N: usize>(&mut self, what: &str) -> Result<[u8; N]> {
        Ok(self.bytes(N, what)?.try_into().expect("N bytes"))
    }

    pub fn bool(&mut self) -> Result<bool> {
        Ok(self.u8()? != 0)
    }

    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.array::<1>("a byte")?[0])
    }

    pub fn i8(&mut self) -> Result<i8> {
        Ok(self.u8()? as i8)
    }

    pub fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(self.array("a short")?))
    }

    pub fn i16(&mut self) -> Result<i16> {
        Ok(i16::from_be_bytes(self.array("a short")?))
    }

    pub fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_be_bytes(self.array("an int")?))
    }

    pub fn i64(&mut self) -> Result<i64> {
        Ok(i64::from_be_bytes(self.array("a long")?))
    }

    pub fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_be_bytes(self.array("a float")?))
    }

    pub fn f64(&mut self) -> Result<f64> {
        Ok(f64::from_be_bytes(self.array("a double")?))
    }

    /// Runs one of `pomme_protocol::wire`'s readers, staying at the value's
    /// start when it fails.
    fn wire<T>(&mut self, read: fn(&[u8], &mut usize) -> Option<T>, message: &str) -> Result<T> {
        let start = self.pos;
        read(self.bytes, &mut self.pos).ok_or_else(|| {
            self.pos = start;
            self.error(message)
        })
    }

    /// `VarInt`: 7 bits per byte, low bits first, at most 5 bytes.
    pub fn var_int(&mut self) -> Result<i32> {
        self.wire(read_varint, "VarInt is cut off or longer than 5 bytes")
            .map(|v| v as i32)
    }

    /// `VarLong`: like `VarInt`, at most 10 bytes.
    pub fn var_long(&mut self) -> Result<i64> {
        let start = self.pos;
        let mut value = 0u64;
        for shift in 0..10 {
            let Some(&byte) = self.bytes.get(self.pos) else {
                break;
            };
            self.pos += 1;
            value |= u64::from(byte & 0x7F) << (shift * 7);
            if byte & 0x80 == 0 {
                return Ok(value as i64);
            }
        }
        self.pos = start;
        Err(self.error("VarLong is cut off or longer than 10 bytes"))
    }

    /// `ByteBufCodecs.OPTIONAL_VAR_INT`: 0 for none, otherwise the value plus
    /// one.
    pub fn optional_var_int(&mut self) -> Result<Option<i32>> {
        Ok(match self.var_int()? {
            0 => None,
            value => Some(value - 1),
        })
    }

    /// A `VarInt` that is a length or count.
    pub fn count(&mut self) -> Result<usize> {
        let len = self.var_int()?;
        usize::try_from(len).map_err(|_| self.error(format!("negative length {len}")))
    }

    /// `Utf8String`: a `VarInt` byte length, then UTF-8.
    pub fn string(&mut self) -> Result<&'a str> {
        let len = self.count()?;
        let start = self.pos;
        let bytes = self.bytes(len, "a string")?;
        std::str::from_utf8(bytes).map_err(|_| Error::misread(start, "string is not valid UTF-8"))
    }

    /// `ByteBufCodecs.BYTE_ARRAY`: a `VarInt` length, then the bytes.
    pub fn byte_array(&mut self) -> Result<&'a [u8]> {
        let len = self.count()?;
        self.bytes(len, "a byte array")
    }

    /// `UUIDUtil.STREAM_CODEC`: most significant long, then least.
    pub fn uuid(&mut self) -> Result<u128> {
        Ok(u128::from_be_bytes(self.array("a UUID")?))
    }

    /// `BlockPos.asLong`: x in the top 26 bits, then z in 26, then y in 12.
    pub fn block_pos(&mut self) -> Result<BlockPos> {
        let packed = self.i64()?;
        Ok(BlockPos {
            x: (packed >> 38) as i32,
            y: (packed << 52 >> 52) as i32,
            z: (packed << 26 >> 38) as i32,
        })
    }

    /// `ChunkPos.asLong`: x in the low 32 bits, z in the high 32.
    pub fn chunk_pos(&mut self) -> Result<(i32, i32)> {
        let packed = self.i64()?;
        Ok((packed as i32, (packed >> 32) as i32))
    }

    /// `Mth.unpackDegrees`: a byte of 256ths of a turn.
    pub fn angle(&mut self) -> Result<f32> {
        Ok(f32::from(self.i8()?) * 360.0 / 256.0)
    }

    /// `FriendlyByteBuf.readVector3f`: three floats.
    pub fn vector3f(&mut self) -> Result<[f32; 3]> {
        Ok([self.f32()?, self.f32()?, self.f32()?])
    }

    /// `Vec3.STREAM_CODEC`: three doubles.
    pub fn vec3(&mut self) -> Result<[f64; 3]> {
        Ok([self.f64()?, self.f64()?, self.f64()?])
    }

    /// `LpVec3`: a low-precision vector, one byte when zero.
    pub fn lp_vec3(&mut self) -> Result<[f64; 3]> {
        self.wire(read_lp_vec3, "LpVec3 is cut off")
            .map(|v| [v.x, v.y, v.z])
    }
}

#[cfg(test)]
mod tests {
    use pomme_protocol::wire::write_varint;

    use super::super::testing::block_pos;
    use super::*;

    #[test]
    fn var_int_edges() {
        for value in [0, 1, 127, 128, 25565, i32::MAX, -1, i32::MIN] {
            let mut bytes = Vec::new();
            write_varint(&mut bytes, value as u32);
            let mut reader = Reader::new(&bytes);
            assert_eq!(reader.var_int(), Ok(value));
            assert_eq!(reader.remaining(), 0);
        }
        let mut cut = Reader::new(&[0x80, 0x80]);
        assert_eq!(cut.var_int().unwrap_err().at, 0);
        let mut long = Reader::new(&[0xff; 6]);
        assert!(long.var_int().is_err());
    }

    #[test]
    fn var_long_edges() {
        let mut reader = Reader::new(&[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01]);
        assert_eq!(reader.var_long(), Ok(-1));
        let mut reader = Reader::new(&[0xac, 0x02]);
        assert_eq!(reader.var_long(), Ok(300));
        assert!(Reader::new(&[0x80]).var_long().is_err());
    }

    #[test]
    fn truncation_reports_the_offset() {
        let mut reader = Reader::new(&[1, 2, 3]);
        reader.u8().unwrap();
        let err = reader.i64().unwrap_err();
        assert_eq!(err.at, 1);
        assert_eq!(err.message, "a long needs 8 bytes, 2 left");
    }

    #[test]
    fn strings() {
        let mut reader = Reader::new(b"\x07vanilla\x02\xff\xfe");
        assert_eq!(reader.string(), Ok("vanilla"));
        let err = reader.string().unwrap_err();
        assert_eq!(
            (err.at, err.message.as_str()),
            (9, "string is not valid UTF-8")
        );
    }

    #[test]
    fn block_pos_is_sign_extended() {
        let round_trip = |x: i64, y: i64, z: i64| {
            let pos = Reader::new(&block_pos(x, y, z)).block_pos().unwrap();
            assert_eq!((pos.x.into(), pos.y.into(), pos.z.into()), (x, y, z));
        };
        round_trip(-1, -2, -3);
        round_trip(30_000_000, 2047, -30_000_000);
        round_trip(-30_000_000, -2048, 30_000_000);
    }

    #[test]
    fn chunk_pos_is_x_low_z_high() {
        let packed = (-5i64 << 32) | (7i64 & 0xFFFF_FFFF);
        assert_eq!(Reader::new(&packed.to_be_bytes()).chunk_pos(), Ok((7, -5)));
    }

    #[test]
    fn angles() {
        assert_eq!(Reader::new(&[64]).angle(), Ok(90.0));
        assert_eq!(Reader::new(&[0x80]).angle(), Ok(-180.0));
    }

    #[test]
    fn lp_vec3_zero_and_continuation() {
        let mut zero = Reader::new(&[0, 9]);
        assert_eq!(zero.lp_vec3(), Ok([0.0; 3]));
        assert_eq!(zero.pos(), 1);
        // Scale 8 needs the continuation flag: low bits 0, flag 4, varint 2.
        // Every axis packs 32766, the top of the range, so reads back as 8.
        let buffer: u64 = 4 | 32766 << 3 | 32766 << 18 | 32766 << 33;
        let mut bytes = vec![buffer as u8, (buffer >> 8) as u8];
        bytes.extend_from_slice(&((buffer >> 16) as u32).to_be_bytes());
        bytes.push(2);
        let mut reader = Reader::new(&bytes);
        assert_eq!(reader.lp_vec3(), Ok([8.0; 3]));
        assert_eq!(reader.remaining(), 0);
        assert_eq!(Reader::new(&bytes[..3]).lp_vec3().unwrap_err().at, 0);
    }
}
