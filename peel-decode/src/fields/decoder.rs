use std::borrow::Cow;
use std::fmt::Debug;

use pomme_protocol::{ClientRegistry, RegistryTable};
use serde::Serialize;

use super::reader::{BlockPos, Reader, Result};

pub type Name = Cow<'static, str>;

/// One decoded field, with its value rendered for display. Rendering happens
/// here so `peel dump` and the viewer show the same text.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Field {
    pub name: Name,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// Where the field's bytes start in the frame, counting the packet id.
    pub start: usize,
    /// Where they end, exclusive.
    pub end: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Field>,
}

impl Field {
    /// Visits this field and its descendants depth first.
    pub fn walk(&self, depth: usize, visit: &mut impl FnMut(usize, &Field)) {
        visit(depth, self);
        for child in &self.children {
            child.walk(depth + 1, visit);
        }
    }
}

/// Reads a packet's fields in codec order, recording each one.
pub struct Decoder<'a> {
    reader: Reader<'a>,
    protocol: i32,
    fields: Vec<Field>,
}

impl<'a> Decoder<'a> {
    pub(super) fn new(reader: Reader<'a>, protocol: i32) -> Self {
        Self {
            reader,
            protocol,
            fields: Vec::new(),
        }
    }

    pub(super) fn into_parts(self) -> (Reader<'a>, Vec<Field>) {
        (self.reader, self.fields)
    }

    pub fn protocol(&self) -> i32 {
        self.protocol
    }

    pub fn reader(&mut self) -> &mut Reader<'a> {
        &mut self.reader
    }

    /// Reads a value with `read` and records it, rendered by `show`.
    pub fn read<T>(
        &mut self,
        name: impl Into<Name>,
        read: impl FnOnce(&mut Reader<'a>) -> Result<T>,
        show: impl FnOnce(&T) -> String,
    ) -> Result<T> {
        let start = self.reader.pos();
        let value = read(&mut self.reader)?;
        self.fields.push(Field {
            name: name.into(),
            value: Some(show(&value)),
            start,
            end: self.reader.pos(),
            children: Vec::new(),
        });
        Ok(value)
    }

    /// Records the fields `decode` reads as children of one field, keeping
    /// them when a read fails part way.
    pub fn group<T>(
        &mut self,
        name: impl Into<Name>,
        decode: impl FnOnce(&mut Self) -> Result<T>,
    ) -> Result<T> {
        let start = self.reader.pos();
        let outer = std::mem::take(&mut self.fields);
        let result = decode(self);
        let children = std::mem::replace(&mut self.fields, outer);
        self.fields.push(Field {
            name: name.into(),
            value: None,
            start,
            end: self.reader.pos(),
            children,
        });
        result
    }

    /// Sets the value shown for the field recorded last, such as a summary
    /// of a group.
    pub fn show_last(&mut self, value: String) {
        if let Some(field) = self.fields.last_mut() {
            field.value = Some(value);
        }
    }

    /// `ByteBufCodecs.list` and `collection`: a `VarInt` count, then the
    /// items, each read by `item` under its index as the name.
    pub fn list(
        &mut self,
        name: impl Into<Name>,
        mut item: impl FnMut(&mut Self, Name) -> Result<()>,
    ) -> Result<usize> {
        let len = self.group(name, |d| {
            let len = d.reader.count()?;
            for index in 0..len {
                item(d, index.to_string().into())?;
            }
            Ok(len)
        })?;
        self.show_last(count(len, "item"));
        Ok(len)
    }

    /// Reads a prefix such as a presence flag or tag type, then the one
    /// field `field` records, widened to cover the prefix.
    pub fn prefixed<P, T>(
        &mut self,
        prefix: impl FnOnce(&mut Reader<'a>) -> Result<P>,
        field: impl FnOnce(&mut Self, P) -> Result<T>,
    ) -> Result<T> {
        let start = self.reader.pos();
        let recorded = self.fields.len();
        let prefix = prefix(&mut self.reader)?;
        let result = field(self, prefix);
        if self.fields.len() > recorded
            && let Some(last) = self.fields.last_mut()
        {
            last.start = start;
        }
        result
    }

    /// Records a field with no bytes of its own beyond a prefix, such as an
    /// absent optional.
    pub fn note(&mut self, name: impl Into<Name>, value: &str) -> Result<()> {
        self.read(name, |_| Ok(()), |()| value.into())
    }

    /// `ByteBufCodecs.optional`: a bool, then, when it is true, the value
    /// `present` records as one field under `name`.
    pub fn optional<T>(
        &mut self,
        name: impl Into<Name>,
        present: impl FnOnce(&mut Self, Name) -> Result<T>,
    ) -> Result<Option<T>> {
        let name = name.into();
        self.prefixed(Reader::bool, |d, is_present| {
            if is_present {
                present(d, name).map(Some)
            } else {
                d.note(name, "absent").map(|()| None)
            }
        })
    }

    pub fn bool(&mut self, name: impl Into<Name>) -> Result<bool> {
        self.read(name, Reader::bool, ToString::to_string)
    }

    pub fn u8(&mut self, name: impl Into<Name>) -> Result<u8> {
        self.read(name, Reader::u8, ToString::to_string)
    }

    pub fn i8(&mut self, name: impl Into<Name>) -> Result<i8> {
        self.read(name, Reader::i8, ToString::to_string)
    }

    pub fn u16(&mut self, name: impl Into<Name>) -> Result<u16> {
        self.read(name, Reader::u16, ToString::to_string)
    }

    pub fn i16(&mut self, name: impl Into<Name>) -> Result<i16> {
        self.read(name, Reader::i16, ToString::to_string)
    }

    pub fn i32(&mut self, name: impl Into<Name>) -> Result<i32> {
        self.read(name, Reader::i32, ToString::to_string)
    }

    pub fn i64(&mut self, name: impl Into<Name>) -> Result<i64> {
        self.read(name, Reader::i64, ToString::to_string)
    }

    pub fn f32(&mut self, name: impl Into<Name>) -> Result<f32> {
        self.read(name, Reader::f32, debug)
    }

    pub fn f64(&mut self, name: impl Into<Name>) -> Result<f64> {
        self.read(name, Reader::f64, debug)
    }

    pub fn var_int(&mut self, name: impl Into<Name>) -> Result<i32> {
        self.read(name, Reader::var_int, ToString::to_string)
    }

    pub fn var_long(&mut self, name: impl Into<Name>) -> Result<i64> {
        self.read(name, Reader::var_long, ToString::to_string)
    }

    pub fn string(&mut self, name: impl Into<Name>) -> Result<&'a str> {
        self.read(name, Reader::string, debug)
    }

    pub fn uuid(&mut self, name: impl Into<Name>) -> Result<u128> {
        self.read(name, Reader::uuid, |uuid| {
            let hex = format!("{uuid:032x}");
            format!(
                "{}-{}-{}-{}-{}",
                &hex[..8],
                &hex[8..12],
                &hex[12..16],
                &hex[16..20],
                &hex[20..]
            )
        })
    }

    pub fn block_pos(&mut self, name: impl Into<Name>) -> Result<BlockPos> {
        self.read(name, Reader::block_pos, |p| {
            format!("{}, {}, {}", p.x, p.y, p.z)
        })
    }

    pub fn chunk_pos(&mut self, name: impl Into<Name>) -> Result<(i32, i32)> {
        self.read(name, Reader::chunk_pos, |(x, z)| format!("{x}, {z}"))
    }

    pub fn angle(&mut self, name: impl Into<Name>) -> Result<f32> {
        self.read(name, Reader::angle, |degrees| format!("{degrees:?}°"))
    }

    pub fn vector3f(&mut self, name: impl Into<Name>) -> Result<[f32; 3]> {
        self.read(name, Reader::vector3f, show_vec3)
    }

    pub fn vec3(&mut self, name: impl Into<Name>) -> Result<[f64; 3]> {
        self.read(name, Reader::vec3, show_vec3)
    }

    pub fn lp_vec3(&mut self, name: impl Into<Name>) -> Result<[f64; 3]> {
        self.read(name, Reader::lp_vec3, show_vec3)
    }

    /// Three integers read by `read`, in fixed point with `unit` steps per
    /// block.
    pub fn fixed_vec3<T: Into<f64>>(
        &mut self,
        name: impl Into<Name>,
        read: impl Fn(&mut Reader<'a>) -> Result<T>,
        unit: f64,
    ) -> Result<[f64; 3]> {
        self.read(
            name,
            |r| {
                Ok([
                    read(r)?.into() / unit,
                    read(r)?.into() / unit,
                    read(r)?.into() / unit,
                ])
            },
            show_vec3,
        )
    }

    /// `len` raw bytes, shown by length.
    pub fn bytes(&mut self, name: impl Into<Name>, len: usize) -> Result<&'a [u8]> {
        self.read(
            name,
            |r| r.bytes(len, "raw bytes"),
            |b| count(b.len(), "byte"),
        )
    }

    /// `ByteBufCodecs.BYTE_ARRAY`, shown by length.
    pub fn byte_array(&mut self, name: impl Into<Name>) -> Result<&'a [u8]> {
        self.read(name, Reader::byte_array, |b| count(b.len(), "byte"))
    }

    /// Everything left in the frame, such as an unrecognised payload.
    pub fn rest(&mut self, name: impl Into<Name>) -> Result<&'a [u8]> {
        self.read(name, |r| Ok(r.rest()), |b| count(b.len(), "byte"))
    }

    /// A value read by `read` naming one of `names` by ordinal; an empty name
    /// marks an unused ordinal.
    pub fn variant<T: Copy + Into<i64>>(
        &mut self,
        name: impl Into<Name>,
        read: impl FnOnce(&mut Reader<'a>) -> Result<T>,
        names: &[&str],
    ) -> Result<T> {
        self.read(name, read, |&ordinal| {
            let ordinal = ordinal.into();
            let label = usize::try_from(ordinal)
                .ok()
                .and_then(|i| names.get(i))
                .filter(|name| !name.is_empty())
                .unwrap_or(&"unknown");
            format!("{label} ({ordinal})")
        })
    }

    /// A value read by `read` whose bit `i` is flag `names[i]`; an empty or
    /// missing name shows the bit number.
    pub fn flags<T: Copy + Into<i64>>(
        &mut self,
        name: impl Into<Name>,
        read: impl FnOnce(&mut Reader<'a>) -> Result<T>,
        names: &[&str],
    ) -> Result<T> {
        self.read(name, read, |&bits| {
            let bits = bits.into() as u64;
            let set: Vec<Cow<str>> = (0..64)
                .filter(|bit| bits & (1 << bit) != 0)
                .map(|bit| match names.get(bit) {
                    Some(&name) if !name.is_empty() => name.into(),
                    _ => format!("bit {bit}").into(),
                })
                .collect();
            if set.is_empty() {
                "none".into()
            } else {
                format!("0x{bits:02x} {}", set.join(" | "))
            }
        })
    }

    /// A `VarInt` id in a registry `pomme-protocol` has no table for, such as
    /// block states or synced registries.
    pub fn raw_id(&mut self, name: impl Into<Name>) -> Result<i32> {
        self.read(name, Reader::var_int, |id| format!("#{id}"))
    }

    /// A `VarInt` id in one of the registries `pomme-protocol` carries.
    pub fn registry(&mut self, name: impl Into<Name>, registry: ClientRegistry) -> Result<i32> {
        let protocol = self.protocol;
        self.read(name, Reader::var_int, |&id| {
            registry_entry(protocol, registry, id)
        })
    }

    /// Entry `id` of `registry`, named when the trace's version has a table
    /// for it.
    pub fn registry_entry(&self, registry: ClientRegistry, id: i32) -> String {
        registry_entry(self.protocol, registry, id)
    }
}

fn registry_entry(protocol: i32, registry: ClientRegistry, id: i32) -> String {
    let entry = u32::try_from(id)
        .ok()
        .and_then(|id| RegistryTable::for_protocol(protocol)?.name_of(registry, id));
    match entry {
        Some(entry) => format!("{entry} (#{id})"),
        None => format!("#{id}"),
    }
}

fn show_vec3<T: Debug>(v: &[T; 3]) -> String {
    format!("{:?}, {:?}, {:?}", v[0], v[1], v[2])
}

/// `"1 item"`, `"2 items"`.
pub fn count(n: usize, noun: &str) -> String {
    format!("{n} {noun}{}", if n == 1 { "" } else { "s" })
}

/// Floats in their shortest exact form, strings quoted and escaped.
fn debug<T: Debug>(value: &T) -> String {
    format!("{value:?}")
}
