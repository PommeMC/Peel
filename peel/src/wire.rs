//! Frame splitting, decompression and the few fields capture has to read,
//! following the vanilla codecs under `net/minecraft/network/`.

use std::io::Read;

use anyhow::{Context, Result, bail, ensure};
use flate2::read::ZlibDecoder;
use peel_decode::fields::reader::Reader;
use pomme_protocol::wire::read_varint;

/// `Varint21FrameDecoder`: the frame length prefix is at most 3 bytes.
const MAX_LENGTH_PREFIX: usize = 3;
/// `CompressionDecoder.MAXIMUM_UNCOMPRESSED_LENGTH`, which is also the
/// longest record a trace may hold.
const MAX_UNCOMPRESSED_LENGTH: u32 = peel_trace::MAX_RECORD_LENGTH as u32;

/// Splits a byte stream into frames the way `Varint21FrameDecoder` does,
/// however the stream was cut into reads.
#[derive(Default)]
pub struct FrameSplitter {
    buf: Vec<u8>,
    start: usize,
}

impl FrameSplitter {
    pub fn push(&mut self, bytes: &[u8]) {
        self.buf.drain(..self.start);
        self.start = 0;
        self.buf.extend_from_slice(bytes);
    }

    /// The next whole frame body, or `None` until more bytes arrive.
    pub fn next_frame(&mut self) -> Result<Option<Vec<u8>>> {
        loop {
            let pending = &self.buf[self.start..];
            let terminated = pending
                .iter()
                .take(MAX_LENGTH_PREFIX)
                .any(|b| b & 0x80 == 0);
            if !terminated {
                ensure!(
                    pending.len() < MAX_LENGTH_PREFIX,
                    "frame length wider than 21 bits"
                );
                return Ok(None);
            }
            let mut body_at = 0;
            let len = read_varint(pending, &mut body_at).expect("terminated varint") as usize;
            let Some(frame) = pending.get(body_at..body_at + len) else {
                return Ok(None);
            };
            let frame = frame.to_vec();
            self.start += body_at + len;
            // 26.2 rejects empty frames and 1.20.1 skips them; neither
            // carries a packet.
            if len > 0 {
                return Ok(Some(frame));
            }
        }
    }
}

/// `CompressionDecoder`: a data-length varint, 0 for a packet sent as is,
/// otherwise the inflated length of the zlib data that follows.
pub fn decompress(frame: &[u8]) -> Result<Vec<u8>> {
    let mut pos = 0;
    let data_len = read_varint(frame, &mut pos).context("truncated compression header")?;
    let data = &frame[pos..];
    if data_len == 0 {
        return Ok(data.to_vec());
    }
    ensure!(
        data_len <= MAX_UNCOMPRESSED_LENGTH,
        "compressed packet declares {data_len} bytes, over the protocol maximum of \
         {MAX_UNCOMPRESSED_LENGTH}"
    );
    let mut packet = Vec::with_capacity(data_len as usize);
    ZlibDecoder::new(data)
        .take(u64::from(data_len))
        .read_to_end(&mut packet)
        .context("inflating a compressed packet")?;
    ensure!(
        packet.len() == data_len as usize,
        "compressed packet inflated to {} bytes but declares {data_len}",
        packet.len()
    );
    Ok(packet)
}

/// `ClientIntent`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Intent {
    Status,
    Login,
    Transfer,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Intention {
    pub protocol: i32,
    pub intent: Intent,
}

/// `ClientIntentionPacket` fields: protocol varint, host string, u16 port,
/// intent varint.
pub fn parse_intention(body: &[u8]) -> Result<Intention> {
    let mut reader = Reader::new(body);
    let mut read = || -> Result<(i32, i32)> {
        let protocol = reader.var_int()?;
        reader.byte_array()?;
        reader.u16()?;
        Ok((protocol, reader.var_int()?))
    };
    let (protocol, intent) = read().context("truncated handshake")?;
    let intent = match intent {
        1 => Intent::Status,
        2 => Intent::Login,
        3 => Intent::Transfer,
        other => bail!("unknown handshake intent {other}"),
    };
    Ok(Intention { protocol, intent })
}

/// `ClientboundLoginCompressionPacket` fields: the threshold varint.
pub fn read_threshold(body: &[u8]) -> Result<i32> {
    Reader::new(body)
        .var_int()
        .context("truncated login_compression")
}

#[cfg(test)]
pub mod test_frames {
    use std::io::Write;

    use flate2::Compression;
    use flate2::write::ZlibEncoder;
    use peel_decode::test_util::named;
    use peel_trace::{Direction, Phase};
    use pomme_protocol::PacketTable;
    use pomme_protocol::wire::write_varint;

    /// `ClientIntent` ids.
    pub const STATUS: u32 = 1;
    pub const LOGIN: u32 = 2;
    pub const TRANSFER: u32 = 3;

    /// The handshake packet for `table`'s protocol.
    pub fn handshake(table: &PacketTable, intent: u32) -> Vec<u8> {
        let body = intention(table.version().protocol, intent);
        named(
            table,
            Phase::Handshake,
            Direction::Serverbound,
            "intention",
            &body,
        )
        .1
    }

    /// Frames a packet as vanilla's encoder does: `CompressionEncoder` when
    /// a threshold is set, then the length prefix.
    pub fn frame(packet: &[u8], threshold: Option<usize>) -> Vec<u8> {
        let mut data = Vec::new();
        match threshold {
            None => data.extend_from_slice(packet),
            Some(threshold) if packet.len() < threshold => {
                write_varint(&mut data, 0);
                data.extend_from_slice(packet);
            }
            Some(_) => {
                write_varint(&mut data, packet.len() as u32);
                let mut zlib = ZlibEncoder::new(Vec::new(), Compression::default());
                zlib.write_all(packet).unwrap();
                data.extend(zlib.finish().unwrap());
            }
        }
        let mut frame = Vec::new();
        write_varint(&mut frame, data.len() as u32);
        frame.extend(data);
        frame
    }

    pub fn intention(protocol: i32, intent: u32) -> Vec<u8> {
        let mut body = Vec::new();
        write_varint(&mut body, protocol as u32);
        write_varint(&mut body, 9);
        body.extend_from_slice(b"localhost");
        body.extend_from_slice(&25565u16.to_be_bytes());
        write_varint(&mut body, intent);
        body
    }
}

#[cfg(test)]
mod tests {
    use peel_decode::test_util::packet;
    use pomme_protocol::version::LATEST;
    use pomme_protocol::wire::write_varint;

    use super::test_frames::{LOGIN, STATUS, TRANSFER, frame, intention};
    use super::*;

    fn drain(splitter: &mut FrameSplitter) -> Vec<Vec<u8>> {
        std::iter::from_fn(|| splitter.next_frame().unwrap()).collect()
    }

    #[test]
    fn several_frames_in_one_read() {
        let mut bytes = frame(&[1, 2, 3], None);
        bytes.extend(frame(&[4], None));
        bytes.extend(frame(&[5, 6], None));
        let mut splitter = FrameSplitter::default();
        splitter.push(&bytes);
        assert_eq!(drain(&mut splitter), [vec![1, 2, 3], vec![4], vec![5, 6]]);
    }

    #[test]
    fn frames_split_across_reads() {
        let body = vec![9u8; 300];
        let mut bytes = frame(&body, None);
        bytes.extend(frame(&[7], None));
        let mut splitter = FrameSplitter::default();
        let mut frames = Vec::new();
        // Byte by byte, which also splits inside the 2-byte length prefix.
        for byte in &bytes {
            splitter.push(std::slice::from_ref(byte));
            frames.extend(drain(&mut splitter));
        }
        assert_eq!(frames, [body, vec![7]]);
    }

    #[test]
    fn length_prefix_boundaries() {
        for (len, prefix) in [
            (1, 1),
            (127, 1),
            (128, 2),
            (16_383, 2),
            (16_384, 3),
            ((1 << 21) - 1, 3),
        ] {
            let bytes = frame(&vec![0xAB; len], None);
            assert_eq!(bytes.len(), len + prefix, "{len}");
            let mut splitter = FrameSplitter::default();
            splitter.push(&bytes[..bytes.len() - 1]);
            assert_eq!(splitter.next_frame().unwrap(), None, "{len}");
            splitter.push(&bytes[bytes.len() - 1..]);
            assert_eq!(splitter.next_frame().unwrap().unwrap().len(), len);
        }
    }

    #[test]
    fn length_wider_than_21_bits_is_corrupt() {
        let mut splitter = FrameSplitter::default();
        splitter.push(&[0x80, 0x80]);
        assert_eq!(splitter.next_frame().unwrap(), None);
        splitter.push(&[0x80]);
        let err = splitter.next_frame().unwrap_err();
        assert!(err.to_string().contains("21 bits"), "{err}");
    }

    #[test]
    fn non_minimal_length_and_empty_frames() {
        let mut splitter = FrameSplitter::default();
        // 0x81 0x00 is a non-minimal 1; vanilla accepts it.
        splitter.push(&[0x00, 0x81, 0x00, 42, 0x00]);
        assert_eq!(drain(&mut splitter), [vec![42]]);
    }

    #[test]
    fn decompress_modes() {
        let small = packet(1, &[1, 2, 3]);
        let large = packet(2, &[5; 1000]);
        let threshold = Some(256);
        for packet in [&small, &large] {
            let mut splitter = FrameSplitter::default();
            splitter.push(&frame(packet, threshold));
            let body = splitter.next_frame().unwrap().unwrap();
            assert_eq!(&decompress(&body).unwrap(), packet);
        }
        // Frames under 128 bytes have a 1-byte length prefix, so byte 1
        // starts the data length: 0 below the threshold, the packet's
        // length above it.
        assert_eq!(frame(&small, threshold)[1], 0);
        assert_eq!(frame(&large, threshold)[1..3], [0xE9, 0x07]);
    }

    #[test]
    fn decompress_rejects_bad_lengths() {
        // A 1001-byte packet: 1-byte length prefix, 2-byte data length, zlib.
        let zlib = frame(&packet(2, &[5; 1000]), Some(1)).split_off(3);
        let mut declared_more = Vec::new();
        write_varint(&mut declared_more, 1002);
        declared_more.extend(zlib);
        let err = decompress(&declared_more).unwrap_err();
        assert!(err.to_string().contains("inflated to 1001"), "{err}");

        let mut too_big = Vec::new();
        write_varint(&mut too_big, MAX_UNCOMPRESSED_LENGTH + 1);
        let err = decompress(&too_big).unwrap_err();
        assert!(err.to_string().contains("protocol maximum"), "{err}");
    }

    #[test]
    fn intention_fields() {
        assert_eq!(
            parse_intention(&intention(LATEST.protocol, LOGIN)).unwrap(),
            Intention {
                protocol: LATEST.protocol,
                intent: Intent::Login
            }
        );
        assert_eq!(
            parse_intention(&intention(0x4000_0145, TRANSFER))
                .unwrap()
                .intent,
            Intent::Transfer
        );
        assert!(parse_intention(&intention(LATEST.protocol, 4)).is_err());
        assert!(parse_intention(&intention(LATEST.protocol, STATUS)[..5]).is_err());
    }
}
