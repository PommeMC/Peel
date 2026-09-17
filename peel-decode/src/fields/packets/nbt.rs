//! Network NBT (`NbtIo.writeAnyTag`): a tag type byte, then the payload.
//! 1.20.1 frames the root like a file, with an empty name after the type.

use super::super::reader::{Reader, Result};
use super::super::{Decoder, Name, count};
use super::V1_20_2;

/// What text components need from a tag; numbers and arrays are only shown.
#[derive(Debug, PartialEq)]
pub enum Value {
    String(String),
    List(Vec<Value>),
    Compound(Vec<(String, Value)>),
    Other,
}

/// `NbtAccounter`'s depth limit.
const MAX_DEPTH: usize = 512;

/// `FriendlyByteBuf.readNbt`: one tag, or `End` for none.
pub fn root(d: &mut Decoder, name: impl Into<Name>) -> Result<Value> {
    let name = name.into();
    d.prefixed(Reader::u8, |d, kind| {
        if kind == 0 {
            d.note(name, "none")?;
            return Ok(Value::Other);
        }
        if d.protocol() < V1_20_2 {
            string(d.reader())?;
        }
        tag(d, name, kind, 0)
    })
}

fn tag(d: &mut Decoder, name: Name, kind: u8, depth: usize) -> Result<Value> {
    if depth > MAX_DEPTH {
        return Err(d.reader().error("NBT nests deeper than 512"));
    }
    match kind {
        1 => d.i8(name).map(|_| Value::Other),
        2 => d.i16(name).map(|_| Value::Other),
        3 => d.i32(name).map(|_| Value::Other),
        4 => d.i64(name).map(|_| Value::Other),
        5 => d.f32(name).map(|_| Value::Other),
        6 => d.f64(name).map(|_| Value::Other),
        7 => array(d, name, 1, "byte"),
        8 => d
            .read(name, string, |s| format!("{s:?}"))
            .map(Value::String),
        9 => {
            let items = d.group(name, |d| {
                let kind = d.reader().u8()?;
                let len = length(d.reader())?;
                (0..len)
                    .map(|i| tag(d, i.to_string().into(), kind, depth + 1))
                    .collect::<Result<Vec<_>>>()
            })?;
            d.show_last(count(items.len(), "item"));
            Ok(Value::List(items))
        }
        10 => {
            let entries = d.group(name, |d| {
                let mut entries = Vec::new();
                loop {
                    let kind = d.reader().u8()?;
                    if kind == 0 {
                        return Ok(entries);
                    }
                    let key = string(d.reader())?;
                    let value = tag(d, key.clone().into(), kind, depth + 1)?;
                    entries.push((key, value));
                }
            })?;
            d.show_last(count(entries.len(), "key"));
            Ok(Value::Compound(entries))
        }
        11 => array(d, name, 4, "int"),
        12 => array(d, name, 8, "long"),
        _ => Err(d.reader().error(format!("unknown NBT tag type {kind}"))),
    }
}

/// `ByteArrayTag`, `IntArrayTag` and `LongArrayTag`: an int count, then the
/// elements, shown by count.
fn array(d: &mut Decoder, name: Name, width: usize, noun: &str) -> Result<Value> {
    d.read(
        name,
        |r| {
            let len = length(r)?;
            r.bytes(len * width, "an NBT array")?;
            Ok(len)
        },
        |&len| count(len, noun),
    )?;
    Ok(Value::Other)
}

fn length(r: &mut Reader) -> Result<usize> {
    let len = r.i32()?;
    usize::try_from(len).map_err(|_| r.error(format!("negative NBT length {len}")))
}

/// `DataInput.readUTF`: a u16 byte length, then Java's modified UTF-8, which
/// writes U+0000 as two bytes and characters outside the BMP as surrogates.
fn string(r: &mut Reader) -> Result<String> {
    let len = usize::from(r.u16()?);
    let start = r.pos();
    let bytes = r.bytes(len, "an NBT string")?;
    let invalid = || super::super::reader::Error {
        at: start,
        message: "NBT string is not modified UTF-8".into(),
    };
    let continuation = |i: usize| {
        bytes
            .get(i)
            .filter(|b| *b & 0xC0 == 0x80)
            .map(|b| u16::from(b & 0x3F))
            .ok_or_else(invalid)
    };
    let mut units = Vec::with_capacity(len);
    let mut i = 0;
    while i < len {
        let b = u16::from(bytes[i]);
        let (unit, width) = match bytes[i] {
            0x00..=0x7F => (b, 1),
            0xC0..=0xDF => ((b & 0x1F) << 6 | continuation(i + 1)?, 2),
            0xE0..=0xEF => (
                (b & 0x0F) << 12 | continuation(i + 1)? << 6 | continuation(i + 2)?,
                3,
            ),
            _ => return Err(invalid()),
        };
        units.push(unit);
        i += width;
    }
    String::from_utf16(&units).map_err(|_| invalid())
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::fields::testing::lines;

    /// Network NBT for a compound holding `entries`, each already encoded as
    /// a type byte, name and payload.
    pub fn compound(entries: &[Vec<u8>]) -> Vec<u8> {
        let mut bytes = vec![10];
        for entry in entries {
            bytes.extend_from_slice(entry);
        }
        bytes.push(0);
        bytes
    }

    pub fn named_string(name: &str, value: &str) -> Vec<u8> {
        [&[8][..], &utf(name), &utf(value)].concat()
    }

    pub fn utf(s: &str) -> Vec<u8> {
        [&(s.len() as u16).to_be_bytes()[..], s.as_bytes()].concat()
    }

    fn read(protocol: i32, bytes: &[u8]) -> (Result<Value>, Vec<String>, usize) {
        let mut d = Decoder::new(Reader::new(bytes), protocol);
        let value = root(&mut d, "nbt");
        let (reader, fields) = d.into_parts();
        (value, lines(&fields), reader.remaining())
    }

    #[test]
    fn unnamed_root_from_1_20_2() {
        let list = [
            &[9][..],
            &utf("xs"),
            &[3],
            &2i32.to_be_bytes(),
            &7i32.to_be_bytes(),
            &8i32.to_be_bytes(),
        ]
        .concat();
        let bytes = compound(&[named_string("text", "hi"), list]);
        let (value, lines, left) = read(V1_20_2, &bytes);
        assert_eq!(
            value.unwrap(),
            Value::Compound(vec![
                ("text".into(), Value::String("hi".into())),
                ("xs".into(), Value::List(vec![Value::Other, Value::Other])),
            ])
        );
        assert_eq!(
            lines,
            [
                "nbt: 2 keys [0..31]",
                "  text: \"hi\" [8..12]",
                "  xs: 2 items [17..30]",
                "    0: 7 [22..26]",
                "    1: 8 [26..30]",
            ]
        );
        assert_eq!(left, 0);
    }

    #[test]
    fn named_root_in_1_20_1() {
        let bytes = [&[8][..], &utf(""), &utf("hi")].concat();
        let (value, lines, left) = read(V1_20_2 - 1, &bytes);
        assert_eq!(value.unwrap(), Value::String("hi".into()));
        assert_eq!(lines, ["nbt: \"hi\" [0..7]"]);
        assert_eq!(left, 0);
    }

    #[test]
    fn end_is_none() {
        let (value, lines, _) = read(V1_20_2, &[0]);
        assert_eq!(value.unwrap(), Value::Other);
        assert_eq!(lines, ["nbt: none [0..1]"]);
    }

    #[test]
    fn modified_utf8() {
        // U+0000 as C0 80, and U+1F600 as a surrogate pair of 3-byte units.
        let bytes = [0x00, 0x08, 0x41, 0xC0, 0x80, 0xED, 0xA0, 0xBD, 0xED, 0xB8];
        let mut reader = Reader::new(&bytes);
        assert!(string(&mut reader).is_err());
        let bytes = [
            0x00, 0x09, 0x41, 0xC0, 0x80, 0xED, 0xA0, 0xBD, 0xED, 0xB8, 0x80,
        ];
        assert_eq!(string(&mut Reader::new(&bytes)).unwrap(), "A\0\u{1F600}");
    }

    #[test]
    fn arrays_are_counted() {
        let bytes = [&[11][..], &2i32.to_be_bytes(), &[0; 8]].concat();
        let (_, lines, left) = read(V1_20_2, &bytes);
        assert_eq!(lines, ["nbt: 2 ints [0..13]"]);
        assert_eq!(left, 0);
    }
}
