//! Chunk and light packets. Sections show their palettes and data sizes
//! rather than every block.

use peel_trace::Direction::Clientbound;
use pomme_protocol::ClientRegistry;

use super::super::decoder::set_bits;
use super::super::reader::{Reader, Result};
use super::super::{Decoder, NEWEST, OLDEST, PacketDecoder, count};
use super::{V1_21_5, V26_1, V26_3, game, nbt, raw_id};

pub static DECODERS: &[PacketDecoder] = &[
    game(
        Clientbound,
        "level_chunk_with_light",
        OLDEST..=NEWEST,
        |d| {
            d.i32("x")?;
            d.i32("z")?;
            heightmaps(d)?;
            sections(d)?;
            d.list("block_entities", |d, name| {
                d.group(name, |d| {
                    d.read("xz", Reader::u8, |packed| {
                        format!("{}, {}", packed >> 4, packed & 15)
                    })?;
                    d.i16("y")?;
                    d.registry("type", ClientRegistry::BlockEntityType)?;
                    nbt::root(d, "data")?;
                    Ok(())
                })
            })?;
            light(d)
        },
    ),
    game(Clientbound, "light_update", OLDEST..=NEWEST, |d| {
        d.var_int("x")?;
        d.var_int("z")?;
        light(d)
    }),
    game(Clientbound, "chunks_biomes", OLDEST..=NEWEST, |d| {
        d.list("chunks", |d, name| {
            d.group(name, |d| {
                d.chunk_pos("pos")?;
                section_buffer(d, "biomes", false, |d| palette(d, &BIOMES))
            })
        })?;
        Ok(())
    }),
    game(Clientbound, "section_blocks_update", OLDEST..=NEWEST, |d| {
        d.read("section", Reader::i64, |&packed| {
            format!(
                "{}, {}, {}",
                packed >> 42,
                packed << 44 >> 44,
                packed << 22 >> 42
            )
        })?;
        d.list("blocks", |d, name| {
            d.read(name, Reader::var_long, |&entry| {
                let local = entry & 0xFFF;
                format!(
                    "#{} at {}, {}, {}",
                    entry >> 12,
                    local >> 8,
                    local & 15,
                    local >> 4 & 15
                )
            })?;
            Ok(())
        })?;
        Ok(())
    }),
];

/// Heightmaps: an NBT compound until 1.21.5 sent a map of type id to longs.
fn heightmaps(d: &mut Decoder) -> Result<()> {
    if d.protocol() < V1_21_5 {
        return nbt::root(d, "heightmaps").map(drop);
    }
    d.list("heightmaps", |d, name| {
        d.group(name, |d| {
            d.variant(
                "type",
                Reader::var_int,
                &[
                    "world_surface_wg",
                    "world_surface",
                    "ocean_floor_wg",
                    "ocean_floor",
                    "motion_blocking",
                    "motion_blocking_no_leaves",
                ],
            )?;
            d.long_array("data")?;
            Ok(())
        })
    })?;
    Ok(())
}

/// How a paletted container stores ids: `PalettedContainer.Strategy`.
struct Strategy {
    entries: usize,
    /// The largest bits value with a palette on the wire.
    max_palette_bits: u8,
    /// Palettes store at least this many bits per entry.
    min_storage_bits: u8,
}

const BLOCK_STATES: Strategy = Strategy {
    entries: 4096,
    max_palette_bits: 8,
    min_storage_bits: 4,
};

const BIOMES: Strategy = Strategy {
    entries: 64,
    max_palette_bits: 3,
    min_storage_bits: 1,
};

/// A length-prefixed buffer of sections, one per 16 blocks of the
/// dimension's height, which the packet doesn't say. 1.20.1 and 1.21.5
/// overestimate a chunk's buffer size, so it can end in zero bytes vanilla
/// never reads.
fn section_buffer(
    d: &mut Decoder,
    name: &'static str,
    padded: bool,
    section: impl Fn(&mut Decoder) -> Result<()>,
) -> Result<()> {
    let sections = d.group(name, |d| {
        let len = d.reader().count()?;
        let end = d.reader().pos() + len;
        if len > d.reader().remaining() {
            return Err(d
                .reader()
                .error(format!("a {len}-byte section buffer is cut off")));
        }
        let mut sections = 0;
        while d.reader().pos() < end {
            let left = end - d.reader().pos();
            if padded && d.reader().upcoming(left).iter().all(|&b| b == 0) {
                d.bytes("padding", left)?;
                break;
            }
            d.group(sections.to_string(), &section)?;
            sections += 1;
        }
        if d.reader().pos() != end {
            return Err(d
                .reader()
                .error("sections ran past the end of their buffer"));
        }
        Ok(sections)
    })?;
    d.show_last(count(sections, "section"));
    Ok(())
}

fn sections(d: &mut Decoder) -> Result<()> {
    let padded = matches!(d.protocol(), OLDEST | V1_21_5);
    section_buffer(d, "sections", padded, |d| {
        d.i16("block_count")?;
        if d.protocol() >= V26_1 {
            d.i16("fluid_count")?;
        }
        paletted(d, "block_states", &BLOCK_STATES)?;
        paletted(d, "biomes", &BIOMES)
    })
}

/// `PalettedContainer`: bits per entry, a palette, then the packed ids, whose
/// long count is on the wire until 1.21.5.
fn paletted(d: &mut Decoder, name: &'static str, strategy: &Strategy) -> Result<()> {
    d.group(name, |d| palette(d, strategy))
}

fn palette(d: &mut Decoder, strategy: &Strategy) -> Result<()> {
    let bits = d.u8("bits")?;
    let storage_bits = match bits {
        0 => {
            d.raw_id("value")?;
            Some(0)
        }
        bits if bits <= strategy.max_palette_bits => {
            d.list("palette", raw_id)?;
            Some(bits.max(strategy.min_storage_bits))
        }
        _ => None,
    };
    if d.protocol() < V1_21_5 {
        d.long_array("data")?;
        return Ok(());
    }
    let Some(storage_bits) = storage_bits else {
        return Err(d.reader().unsupported(
            "the data of a global palette, whose size depends on the dimension's block states or biomes,",
        ));
    };
    let longs = match storage_bits {
        0 => 0,
        bits => strategy.entries.div_ceil(64 / usize::from(bits)),
    };
    d.read(
        "data",
        |r| r.bytes(longs * 8, "palette data"),
        |b| count(b.len() / 8, "long"),
    )?;
    Ok(())
}

/// `ClientboundLightUpdatePacketData`.
fn light(d: &mut Decoder) -> Result<()> {
    for name in [
        "sky_mask",
        "block_mask",
        "empty_sky_mask",
        "empty_block_mask",
    ] {
        section_mask(d, name)?;
    }
    for name in ["sky_updates", "block_updates"] {
        d.list(name, |d, name| d.byte_array(name).map(drop))?;
    }
    Ok(())
}

/// A bit set of light sections: longs until 26.3 sent bytes. Shown as the
/// set bit numbers.
fn section_mask(d: &mut Decoder, name: &'static str) -> Result<()> {
    let width = if d.protocol() >= V26_3 { 8 } else { 64 };
    d.read(
        name,
        |r| {
            let len = r.count()?;
            r.bytes(len * width / 8, "a bit set")
        },
        |bytes| {
            let words: Vec<u64> = bytes
                .chunks(width / 8)
                .map(|word| word.iter().fold(0, |value, &b| value << 8 | u64::from(b)))
                .collect();
            set_bits((0..words.len() * width).map(|i| words[i / width] >> (i % width) & 1 == 1))
        },
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::ops::RangeInclusive;

    use peel_trace::Phase::Game;

    use super::super::super::Decoded;
    use super::super::super::testing::{assert_decodes, decode, entry, protocols};
    use super::super::V1_21_4;
    use super::*;

    fn check(range: RangeInclusive<i32>, name: &str, body: &[u8], expected: &[impl AsRef<str>]) {
        assert_decodes(range, Game, Clientbound, name, body, expected);
    }

    #[test]
    fn chunk_in_26_3() {
        let body = [
            &1i32.to_be_bytes()[..],
            &(-2i32).to_be_bytes(),
            &[1, 1, 1],
            &7i64.to_be_bytes(),
            &[8, 0x10, 0, 0, 0, 0, 1, 0, 5],
            &[1, 0x21, 0, 64, 7, 0],
            &[1, 5, 0, 0, 0, 1, 2, 0xa, 0xb, 0],
        ]
        .concat();
        let expected = [
            "x: 1 [1..5]".to_string(),
            "z: -2 [5..9]".into(),
            "heightmaps: 1 item [9..20]".into(),
            "  0:  [10..20]".into(),
            "    type: world_surface (1) [10..11]".into(),
            "    data: 1 long [11..20]".into(),
            "sections: 1 section [20..29]".into(),
            "  0:  [21..29]".into(),
            "    block_count: 4096 [21..23]".into(),
            "    fluid_count: 0 [23..25]".into(),
            "    block_states:  [25..27]".into(),
            "      bits: 0 [25..26]".into(),
            "      value: #1 [26..27]".into(),
            "      data: 0 longs [27..27]".into(),
            "    biomes:  [27..29]".into(),
            "      bits: 0 [27..28]".into(),
            "      value: #5 [28..29]".into(),
            "      data: 0 longs [29..29]".into(),
            "block_entities: 1 item [29..35]".into(),
            "  0:  [30..35]".into(),
            "    xz: 2, 1 [30..31]".into(),
            "    y: 64 [31..33]".into(),
            format!(
                "    type: {} [33..34]",
                entry(V26_3, ClientRegistry::BlockEntityType, 7)
            ),
            "    data: none [34..35]".into(),
            "sky_mask: 0, 2 [35..37]".into(),
            "block_mask: none [37..38]".into(),
            "empty_sky_mask: none [38..39]".into(),
            "empty_block_mask: none [39..40]".into(),
            "sky_updates: 1 item [40..44]".into(),
            "  0: 2 bytes [41..44]".into(),
            "block_updates: 0 items [44..45]".into(),
        ];
        check(V26_3..=NEWEST, "level_chunk_with_light", &body, &expected);
    }

    #[test]
    fn chunk_with_length_prefixed_longs() {
        let body = [
            &1i32.to_be_bytes()[..],
            &2i32.to_be_bytes(),
            &[10, 0],
            &[17, 0, 5, 4, 1, 9, 1],
            &[0; 8],
            &[0, 3, 0],
            &[0],
            &[1],
            &(1u64 << 3).to_be_bytes(),
            &[0, 0, 0, 0, 0],
        ]
        .concat();
        let expected = [
            "x: 1 [1..5]",
            "z: 2 [5..9]",
            "heightmaps: 0 keys [9..11]",
            "sections: 1 section [11..29]",
            "  0:  [12..29]",
            "    block_count: 5 [12..14]",
            "    block_states:  [14..26]",
            "      bits: 4 [14..15]",
            "      palette: 1 item [15..17]",
            "        0: #9 [16..17]",
            "      data: 1 long [17..26]",
            "    biomes:  [26..29]",
            "      bits: 0 [26..27]",
            "      value: #3 [27..28]",
            "      data: 0 longs [28..29]",
            "block_entities: 0 items [29..30]",
            "sky_mask: 3 [30..39]",
            "block_mask: none [39..40]",
            "empty_sky_mask: none [40..41]",
            "empty_block_mask: none [41..42]",
            "sky_updates: 0 items [42..43]",
            "block_updates: 0 items [43..44]",
        ];
        check(
            OLDEST + 1..=V1_21_4,
            "level_chunk_with_light",
            &body,
            &expected,
        );
    }

    #[test]
    fn padding_after_the_last_section() {
        let body = [
            &[0; 8][..],
            &[10, 0, 0, 0],
            &[10, 0, 0, 0, 5, 0, 0, 1, 0, 0, 0],
            &[0, 0, 0, 0, 0, 0, 0],
        ]
        .concat();
        let Decoded::Fields { fields } =
            decode(OLDEST, Game, Clientbound, "level_chunk_with_light", &body)
        else {
            panic!()
        };
        let sections = &fields[3];
        assert_eq!(sections.value.as_deref(), Some("1 section"));
        let padding = sections.children.last().unwrap();
        assert_eq!(
            (padding.name.as_ref(), padding.value.as_deref()),
            ("padding", Some("2 bytes"))
        );
    }

    #[test]
    fn global_palettes_stop_from_1_21_5() {
        for protocol in protocols(V1_21_5..=NEWEST) {
            let (section, bits_end): (&[u8], usize) = if protocol >= V26_1 {
                (&[5, 0, 0, 0, 0, 15], 16)
            } else {
                (&[3, 0, 0, 15], 14)
            };
            let body = [&[0; 8][..], &[0], section].concat();
            let Decoded::Partial { fields, reason, at } =
                decode(protocol, Game, Clientbound, "level_chunk_with_light", &body)
            else {
                panic!("protocol {protocol}")
            };
            assert!(reason.contains("global palette"), "{reason}");
            assert_eq!(at, bits_end, "protocol {protocol}");
            assert_eq!(fields[3].name, "sections");
        }
    }

    #[test]
    fn light_update() {
        check(
            OLDEST..=V26_3 - 1,
            "light_update",
            &[&[1, 2, 1][..], &3u64.to_be_bytes(), &[0, 0, 0, 0, 0]].concat(),
            &[
                "x: 1 [1..2]",
                "z: 2 [2..3]",
                "sky_mask: 0, 1 [3..12]",
                "block_mask: none [12..13]",
                "empty_sky_mask: none [13..14]",
                "empty_block_mask: none [14..15]",
                "sky_updates: 0 items [15..16]",
                "block_updates: 0 items [16..17]",
            ],
        );
    }

    #[test]
    fn section_blocks_update() {
        let packed: i64 = (1 << 42) | (0x3F_FFFF << 20) | 4;
        check(
            OLDEST..=NEWEST,
            "section_blocks_update",
            &[&packed.to_be_bytes()[..], &[1, 0xb2, 0xa2, 0x02]].concat(),
            &[
                "section: 1, 4, -1 [1..9]",
                "blocks: 1 item [9..13]",
                "  0: #9 at 1, 2, 3 [10..13]",
            ],
        );
    }

    #[test]
    fn chunks_biomes() {
        let pos = ((2i64 << 32) | 1).to_be_bytes();
        let head = [
            "chunks: 1 item [1..END]",
            "  0:  [2..END]",
            "    pos: 1, 2 [2..10]",
            "    biomes: 1 section [10..END]",
            "      0:  [11..END]",
            "        bits: 0 [11..12]",
            "        value: #5 [12..13]",
        ];
        let lines = |end: usize, data: &str| {
            let mut lines: Vec<String> = head
                .iter()
                .map(|line| line.replace("END", &end.to_string()))
                .collect();
            lines.push(format!("        data: 0 longs {data}"));
            lines
        };
        check(
            OLDEST..=V1_21_5 - 1,
            "chunks_biomes",
            &[&[1][..], &pos, &[3, 0, 5, 0]].concat(),
            &lines(14, "[13..14]"),
        );
        check(
            V1_21_5..=NEWEST,
            "chunks_biomes",
            &[&[1][..], &pos, &[2, 0, 5]].concat(),
            &lines(13, "[13..13]"),
        );
    }
}
