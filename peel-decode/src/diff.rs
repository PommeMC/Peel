//! Compares a trace against a vanilla reference, tick by tick: packets that
//! are missing, extra or out of order, and fields that differ.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;

use peel_trace::{Direction, Phase};
use serde::{Deserialize, Serialize};

use crate::fields::{self, Decoded, Field};
use crate::{GroupKey, Grouping, Loaded, Packet, Timeline};

/// Fields that differ between any two sessions, such as positions, ids and
/// signatures, so comparing them only buries real differences.
pub const DEFAULT_IGNORES: &[&str] = &[
    "*.position",
    "*.y_rot",
    "*.x_rot",
    "*.y_head_rot",
    "*.delta_movement",
    "*.movement",
    "*.velocity",
    "*.delta",
    "*.steps",
    "*.location",
    "*.hit_result",
    "*.block_pos",
    "*.chunk_pos",
    "*.entity_id",
    "*.entity_ids",
    "*.vehicle_id",
    "*.passenger_ids",
    "*.source_id",
    "*.dest_id",
    "*.item_id",
    "*.player_id",
    "*.breaker_id",
    "*.cause_id",
    "*.direct_id",
    "*.id",
    "*.teleport_id",
    "*.sequence",
    "*.uuid",
    "*.session_id",
    "*.profile_id",
    "*.sender",
    "*.index",
    "*.global_index",
    "*.time",
    "*.timestamp",
    "*.obtained_at",
    "*.game_time",
    "*.clocks",
    "*.seed",
    "*.salt",
    "*.signature",
    "*.argument_signatures",
    "*.last_seen",
    "*.expires_at",
    "*.public_key",
    "*.key_signature",
    "*.desired_chunks_per_tick",
    "hello.name",
    "game_profile.name",
    "login_finished.name",
    "intention.host",
    "intention.port",
];

/// How many groups apart the same packet may be and still count as shifted
/// rather than missing and extra, when [`Options::shift_window`] says nothing.
pub const DEFAULT_SHIFT_WINDOW: i64 = 20;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Options {
    /// Compare clientbound packets too, not only what the client sends.
    pub all_directions: bool,
    /// Compare the trace's tick or window `n + offset` with the reference's
    /// `n`.
    pub offset: i64,
    /// Packets (`keep_alive`) or fields (`move_player_pos.flags`, `*.salt`)
    /// to skip, on top of [`DEFAULT_IGNORES`].
    pub ignores: Vec<String>,
    pub no_default_ignores: bool,
    /// Groups within which the same packet counts as shifted instead of
    /// missing and extra. `Some(0)` reports every such pair as both.
    pub shift_window: Option<i64>,
}

#[derive(Debug, PartialEq)]
pub enum Error {
    Protocols { reference: i32, trace: i32 },
    NoPackets,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Protocols { reference, trace } => write!(
                f,
                "the reference is protocol {reference} but the trace is protocol {trace}; \
                 diff compares traces of one version"
            ),
            Self::NoPackets => write!(f, "both traces need packets to compare"),
        }
    }
}

impl std::error::Error for Error {}

#[derive(Debug, Serialize)]
pub struct Diff {
    pub protocol: i32,
    pub version: Option<&'static str>,
    pub grouping: Grouping,
    pub all_directions: bool,
    /// Every rule applied, defaults included.
    pub ignores: Vec<String>,
    pub group_counts: GroupCounts,
    pub groups: Vec<GroupDiff>,
    pub findings: Vec<Finding>,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct GroupCounts {
    pub compared: usize,
    pub only_in_reference: usize,
    pub only_in_trace: usize,
}

#[derive(Debug, Serialize)]
pub struct GroupDiff {
    pub key: GroupKey,
    /// The group's position in each timeline, where that trace has it.
    pub reference: Option<usize>,
    pub trace: Option<usize>,
    pub rows: Vec<Row>,
}

/// One packet or pair of packets, by record index in each trace.
#[derive(Debug, PartialEq, Serialize)]
pub struct Row {
    pub kind: RowKind,
    /// For [`RowKind::Shifted`], how many groups later the trace sends it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shift: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace: Option<usize>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<FieldDiff>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RowKind {
    Same,
    /// In both traces, but in a different order within the group.
    Moved,
    /// In both traces, in a different group within the shift window. The row
    /// sits in the reference's group.
    Shifted,
    Missing,
    Extra,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct FieldDiff {
    pub path: String,
    /// `None` where that trace has no such field.
    pub reference: Option<String>,
    pub trace: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingKind {
    Missing,
    Extra,
    Moved,
    Shifted,
    Field,
}

/// All rows of one kind for one packet, or all differences in one field,
/// counted and located by the first.
#[derive(Debug, PartialEq, Serialize)]
pub struct Finding {
    pub kind: FindingKind,
    /// For [`FindingKind::Shifted`], the first occurrence's shift.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shift: Option<i64>,
    pub direction: Direction,
    pub phase: Phase,
    pub id: u32,
    pub name: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub count: usize,
    /// For fields: how many pairs of this packet were compared.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compared: Option<usize>,
    pub group: GroupKey,
    pub reference: Option<usize>,
    pub trace: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_value: Option<String>,
}

impl Diff {
    pub fn differs(&self) -> bool {
        !self.findings.is_empty()
    }
}

pub fn diff(reference: &Loaded, trace: &Loaded, options: &Options) -> Result<Diff, Error> {
    let timelines = [reference.timeline(), trace.timeline()];
    let [Some(protocol), Some(trace_protocol)] = timelines.each_ref().map(|t| t.protocol) else {
        return Err(Error::NoPackets);
    };
    if protocol != trace_protocol {
        return Err(Error::Protocols {
            reference: protocol,
            trace: trace_protocol,
        });
    }
    let ignores = Ignores::new(options);
    let sides = [
        Side::new(reference, &timelines[0], 0),
        Side::new(trace, &timelines[1], options.offset),
    ];

    let mut ordinals: BTreeMap<GroupKey, [Option<usize>; 2]> = BTreeMap::new();
    for (side, s) in sides.iter().enumerate() {
        for (ordinal, key) in s.keys.iter().enumerate() {
            ordinals.entry(*key).or_default()[side] = Some(ordinal);
        }
    }
    let mut groups: Vec<GroupDiff> = ordinals
        .into_iter()
        .map(|(key, ordinals)| GroupDiff {
            key,
            reference: ordinals[0],
            trace: ordinals[1],
            rows: group_rows(ordinals, &sides, options, &ignores),
        })
        .collect();
    pair_shifts(&mut groups, &sides, options, &ignores);
    let findings = findings(&groups, &sides);
    let count = |present: fn(&GroupDiff) -> bool| groups.iter().filter(|g| present(g)).count();
    let group_counts = GroupCounts {
        compared: count(|g| g.reference.is_some() && g.trace.is_some()),
        only_in_reference: count(|g| g.trace.is_none()),
        only_in_trace: count(|g| g.reference.is_none()),
    };
    Ok(Diff {
        protocol,
        version: timelines[0].version,
        grouping: timelines[0].grouping,
        all_directions: options.all_directions,
        ignores: ignores.rules.iter().map(ToString::to_string).collect(),
        group_counts,
        groups,
        findings,
    })
}

/// One of the two traces being compared.
struct Side<'a> {
    loaded: &'a Loaded,
    timeline: &'a Timeline<'a>,
    /// Each group's key, shifted by the offset.
    keys: Vec<GroupKey>,
    /// Packets by record index.
    packets: Vec<&'a Packet>,
}

impl<'a> Side<'a> {
    fn new(loaded: &'a Loaded, timeline: &'a Timeline<'a>, offset: i64) -> Self {
        let keys = timeline
            .groups
            .iter()
            .map(|group| match group.key() {
                GroupKey::Setup => GroupKey::Setup,
                GroupKey::Tick(n) => GroupKey::Tick(n - offset),
                GroupKey::Window(n) => GroupKey::Window(n - offset),
            })
            .collect();
        Self {
            loaded,
            timeline,
            keys,
            packets: timeline.packets().collect(),
        }
    }
}

type PacketKey = (Direction, Phase, u32);

fn key(packet: &Packet) -> PacketKey {
    (packet.direction, packet.phase, packet.id)
}

#[derive(Clone, Copy)]
enum Step<'a> {
    Pair {
        reference: &'a Packet,
        trace: &'a Packet,
        moved: bool,
    },
    Missing(&'a Packet),
    Extra(&'a Packet),
}

/// The rows of one group, from whichever traces have it.
fn group_rows(
    ordinals: [Option<usize>; 2],
    sides: &[Side; 2],
    options: &Options,
    ignores: &Ignores,
) -> Vec<Row> {
    let compared = |side: usize| -> Vec<&Packet> {
        let Some(ordinal) = ordinals[side] else {
            return Vec::new();
        };
        sides[side].timeline.groups[ordinal]
            .packets
            .iter()
            .filter(|p| options.all_directions || p.direction == Direction::Serverbound)
            .filter(|p| !ignores.packet(p.name))
            .collect()
    };
    align(&compared(0), &compared(1))
        .into_iter()
        .map(|step| match step {
            Step::Pair {
                reference,
                trace,
                moved,
            } => Row {
                kind: if moved { RowKind::Moved } else { RowKind::Same },
                shift: None,
                reference: Some(reference.index),
                trace: Some(trace.index),
                fields: compare_fields([reference, trace], sides, ignores),
            },
            Step::Missing(packet) => lone(RowKind::Missing, Some(packet.index), None),
            Step::Extra(packet) => lone(RowKind::Extra, None, Some(packet.index)),
        })
        .collect()
}

/// A row for a packet only one trace has.
fn lone(kind: RowKind, reference: Option<usize>, trace: Option<usize>) -> Row {
    Row {
        kind,
        shift: None,
        reference,
        trace,
        fields: Vec::new(),
    }
}

/// Lines two packet sequences up along their longest common subsequence,
/// then pairs leftovers of the same packet as moved.
fn align<'a>(reference: &[&'a Packet], trace: &[&'a Packet]) -> Vec<Step<'a>> {
    let (n, m) = (reference.len(), trace.len());
    let width = m + 1;
    let mut common = vec![0u32; (n + 1) * width];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            common[i * width + j] = if key(reference[i]) == key(trace[j]) {
                common[(i + 1) * width + j + 1] + 1
            } else {
                common[(i + 1) * width + j].max(common[i * width + j + 1])
            };
        }
    }

    let mut steps = Vec::with_capacity(n.max(m));
    let (mut i, mut j) = (0, 0);
    while i < n || j < m {
        if i < n && j < m && key(reference[i]) == key(trace[j]) {
            steps.push(Some(Step::Pair {
                reference: reference[i],
                trace: trace[j],
                moved: false,
            }));
            i += 1;
            j += 1;
        } else if j == m || (i < n && common[(i + 1) * width + j] >= common[i * width + j + 1]) {
            steps.push(Some(Step::Missing(reference[i])));
            i += 1;
        } else {
            steps.push(Some(Step::Extra(trace[j])));
            j += 1;
        }
    }

    for i in 0..steps.len() {
        let Some(Step::Missing(reference)) = steps[i] else {
            continue;
        };
        let extra = steps
            .iter()
            .position(|s| matches!(s, Some(Step::Extra(t)) if key(t) == key(reference)));
        if let Some(Some(Step::Extra(trace))) = extra.map(|j| steps[j].take()) {
            steps[i] = Some(Step::Pair {
                reference,
                trace,
                moved: true,
            });
        }
    }
    steps.into_iter().flatten().collect()
}

/// Pairs a packet missing from one group with the same packet appearing
/// extra in a nearby group: the client sends it, a few ticks apart. The pair
/// becomes one shifted row in the reference's group, so timing differences
/// don't read as a missing and an extra packet.
fn pair_shifts(groups: &mut [GroupDiff], sides: &[Side; 2], options: &Options, ignores: &Ignores) {
    let window = options.shift_window.unwrap_or(DEFAULT_SHIFT_WINDOW);
    if window <= 0 {
        return;
    }

    // Each packet's unpaired rows: missing ones, then extra ones.
    let mut by_packet: HashMap<PacketKey, [Vec<Spot>; 2]> = HashMap::new();
    for (ordinal, group) in groups.iter().enumerate() {
        let Some(number) = group.key.number() else {
            continue;
        };
        for (index, row) in group.rows.iter().enumerate() {
            let (side, record) = match (row.kind, row.reference, row.trace) {
                (RowKind::Missing, Some(reference), _) => (0, reference),
                (RowKind::Extra, _, Some(trace)) => (1, trace),
                _ => continue,
            };
            by_packet
                .entry(key(sides[side].packets[record]))
                .or_default()[side]
                .push(Spot {
                    number,
                    ordinal,
                    index,
                    record,
                });
        }
    }

    let mut paired: HashSet<(usize, usize)> = HashSet::new();
    for [missing, extra] in by_packet.values() {
        // Both sides run in group order, so the nearest pairs match first.
        let (mut m, mut e) = (0, 0);
        while let (Some(reference), Some(trace)) = (missing.get(m), extra.get(e)) {
            let shift = trace.number - reference.number;
            if shift.abs() > window {
                if shift > 0 {
                    m += 1
                } else {
                    e += 1
                }
                continue;
            }
            let pair = [
                sides[0].packets[reference.record],
                sides[1].packets[trace.record],
            ];
            let row = &mut groups[reference.ordinal].rows[reference.index];
            row.kind = RowKind::Shifted;
            row.shift = Some(shift);
            row.trace = Some(trace.record);
            row.fields = compare_fields(pair, sides, ignores);
            paired.insert((trace.ordinal, trace.index));
            (m, e) = (m + 1, e + 1);
        }
    }

    for (ordinal, group) in groups.iter_mut().enumerate() {
        let mut index = 0;
        group.rows.retain(|_| {
            let keep = !paired.contains(&(ordinal, index));
            index += 1;
            keep
        });
    }
}

/// An unpaired row: its group's tick or window number, where the row sits,
/// and the packet's record index in its own trace.
struct Spot {
    number: i64,
    ordinal: usize,
    index: usize,
    record: usize,
}

/// The decoded fields that differ between a pair, in field order.
fn compare_fields(pair: [&Packet; 2], sides: &[Side; 2], ignores: &Ignores) -> Vec<FieldDiff> {
    let [reference, trace] = [0, 1].map(|side| {
        leaves(fields::decode(
            &sides[side].loaded.records[pair[side].index],
        ))
    });
    let lookup = |leaves: &[(String, String)], path: &str| {
        leaves
            .iter()
            .find(|(p, _)| p == path)
            .map(|(_, value)| value.clone())
    };
    let trace_only = trace
        .iter()
        .filter(|(path, _)| lookup(&reference, path).is_none());
    reference
        .iter()
        .chain(trace_only)
        .map(|(path, _)| path)
        .filter(|path| !ignores.field(pair[0].name, path))
        .filter_map(|path| {
            let (r, t) = (lookup(&reference, path), lookup(&trace, path));
            (r != t).then(|| FieldDiff {
                path: path.clone(),
                reference: r,
                trace: t,
            })
        })
        .collect()
}

/// Each leaf field as a path and value, plus a `decoding` entry when
/// decoding stopped short. A packet without a decoder has none.
fn leaves(decoded: Decoded) -> Vec<(String, String)> {
    fn collect(field: &Field, prefix: &str, leaves: &mut Vec<(String, String)>) {
        let path = if prefix.is_empty() {
            field.name.to_string()
        } else {
            format!("{prefix}.{}", field.name)
        };
        if field.children.is_empty() {
            leaves.push((path, field.value.clone().unwrap_or_default()));
        } else {
            for child in &field.children {
                collect(child, &path, leaves);
            }
        }
    }

    let mut leaves: Vec<_> = decoded
        .stop_note()
        .map(|note| ("decoding".to_string(), note))
        .into_iter()
        .collect();
    for field in decoded.fields() {
        collect(field, "", &mut leaves);
    }
    leaves
}

fn findings<'a>(groups: &'a [GroupDiff], sides: &[Side; 2]) -> Vec<Finding> {
    let mut findings: Vec<Finding> = Vec::new();
    let mut positions: HashMap<(FindingKind, PacketKey, Option<&'a str>), usize> = HashMap::new();
    let mut compared: HashMap<PacketKey, usize> = HashMap::new();
    for group in groups {
        for row in &group.rows {
            let kind = match row.kind {
                RowKind::Same => None,
                RowKind::Moved => Some(FindingKind::Moved),
                RowKind::Shifted => Some(FindingKind::Shifted),
                RowKind::Missing => Some(FindingKind::Missing),
                RowKind::Extra => Some(FindingKind::Extra),
            };
            let (reference, trace) = (row.reference, row.trace);
            let packet = match (reference, trace) {
                (Some(r), _) => sides[0].packets[r],
                (None, Some(t)) => sides[1].packets[t],
                (None, None) => unreachable!("every row has a packet"),
            };
            if reference.is_some() && trace.is_some() {
                *compared.entry(key(packet)).or_default() += 1;
            }
            let mut note = |kind: FindingKind, field: Option<&'a FieldDiff>| {
                let position = positions
                    .entry((kind, key(packet), field.map(|f| f.path.as_str())))
                    .or_insert_with(|| {
                        findings.push(Finding {
                            kind,
                            shift: row.shift.filter(|_| kind == FindingKind::Shifted),
                            direction: packet.direction,
                            phase: packet.phase,
                            id: packet.id,
                            name: packet.name,
                            path: field.map(|f| f.path.clone()),
                            count: 0,
                            compared: None,
                            group: group.key,
                            reference,
                            trace,
                            reference_value: field.and_then(|f| f.reference.clone()),
                            trace_value: field.and_then(|f| f.trace.clone()),
                        });
                        findings.len() - 1
                    });
                findings[*position].count += 1;
            };
            if let Some(kind) = kind {
                note(kind, None);
            }
            for field in &row.fields {
                note(FindingKind::Field, Some(field));
            }
        }
    }
    for finding in &mut findings {
        if finding.kind == FindingKind::Field {
            finding.compared = compared
                .get(&(finding.direction, finding.phase, finding.id))
                .copied();
        }
    }
    findings.sort_by_key(|finding| finding.kind);
    findings
}

struct Rule {
    /// `None` for `*`.
    packet: Option<String>,
    /// `None` to skip the whole packet.
    path: Option<String>,
}

impl Rule {
    fn parse(rule: &str) -> Self {
        let (packet, path) = match rule.split_once('.') {
            Some((packet, path)) => (packet, Some(path.to_string())),
            None => (rule, None),
        };
        Self {
            packet: (packet != "*").then(|| packet.to_string()),
            path,
        }
    }

    fn matches_packet(&self, name: Option<&str>) -> bool {
        self.packet.is_none() || self.packet.as_deref() == name
    }
}

impl fmt::Display for Rule {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(self.packet.as_deref().unwrap_or("*"))?;
        match &self.path {
            Some(path) => write!(f, ".{path}"),
            None => Ok(()),
        }
    }
}

struct Ignores {
    rules: Vec<Rule>,
}

impl Ignores {
    fn new(options: &Options) -> Self {
        let defaults = if options.no_default_ignores {
            &[][..]
        } else {
            DEFAULT_IGNORES
        };
        let rules = defaults
            .iter()
            .copied()
            .chain(options.ignores.iter().map(String::as_str))
            .map(str::trim)
            .filter(|rule| !rule.is_empty())
            .map(Rule::parse)
            .collect();
        Self { rules }
    }

    fn packet(&self, name: Option<&str>) -> bool {
        self.rules
            .iter()
            .any(|rule| rule.path.is_none() && rule.matches_packet(name))
    }

    /// Whether a rule covers `path` or a field it is nested in.
    fn field(&self, name: Option<&str>, path: &str) -> bool {
        self.rules.iter().any(|rule| {
            rule.matches_packet(name)
                && rule.path.as_deref().is_some_and(|rule_path| {
                    path.strip_prefix(rule_path)
                        .is_some_and(|rest| rest.is_empty() || rest.starts_with('.'))
                })
        })
    }
}

#[cfg(test)]
mod tests {
    use peel_trace::Direction::{Clientbound, Serverbound};
    use pomme_protocol::version::LATEST;

    use super::*;
    use crate::test_util::{loaded, record_with_body};

    type Spec = (Direction, Phase, &'static str, Vec<u8>);

    fn game(name: &'static str, body: &[u8]) -> Spec {
        (Serverbound, Phase::Game, name, body.to_vec())
    }

    fn input(bits: u8) -> Spec {
        game("player_input", &[bits])
    }

    fn tick_end() -> Spec {
        game("client_tick_end", &[])
    }

    fn load(protocol: i32, packets: &[Spec]) -> Loaded {
        loaded(
            packets
                .iter()
                .enumerate()
                .map(|(i, (direction, phase, name, body))| {
                    record_with_body(protocol, *direction, *phase, name, body, i as u64 * 1000)
                })
                .collect(),
        )
    }

    fn run(reference: &[Spec], trace: &[Spec], options: Options) -> Diff {
        let protocol = LATEST.protocol;
        diff(&load(protocol, reference), &load(protocol, trace), &options).unwrap()
    }

    /// Each group as its key and rows, rows as kind and record indices.
    fn shape(diff: &Diff) -> Vec<String> {
        diff.groups
            .iter()
            .map(|group| {
                let rows: Vec<String> = group
                    .rows
                    .iter()
                    .map(|row| {
                        let indices: Vec<String> = [row.reference, row.trace]
                            .iter()
                            .flatten()
                            .map(ToString::to_string)
                            .collect();
                        format!("{:?} {}", row.kind, indices.join("/")).to_lowercase()
                    })
                    .collect();
                format!("{}: {}", group.key, rows.join(", "))
            })
            .collect()
    }

    /// Findings as kind, packet, field path and count.
    fn summary(diff: &Diff) -> Vec<(FindingKind, &str, Option<&str>, usize)> {
        diff.findings
            .iter()
            .map(|f| (f.kind, f.name.unwrap(), f.path.as_deref(), f.count))
            .collect()
    }

    #[test]
    fn identical_traces_have_no_findings() {
        let packets = [input(1), tick_end(), input(0), tick_end()];
        let diff = run(&packets, &packets, Options::default());
        assert!(!diff.differs());
        assert_eq!(
            shape(&diff),
            ["tick 1: same 0/0, same 1/1", "tick 2: same 2/2, same 3/3"]
        );
        assert_eq!(
            diff.group_counts,
            GroupCounts {
                compared: 2,
                only_in_reference: 0,
                only_in_trace: 0
            }
        );
    }

    #[test]
    fn missing_extra_and_moved_packets() {
        let status = || game("move_player_status_only", &[1]);
        let reference = [
            input(1),
            status(),
            game("set_carried_item", &[0, 3]),
            tick_end(),
        ];
        let trace = [status(), input(1), game("player_loaded", &[]), tick_end()];
        let diff = run(&reference, &trace, Options::default());
        assert_eq!(
            shape(&diff),
            ["tick 1: moved 0/1, same 1/0, missing 2, extra 2, same 3/3"]
        );
        assert_eq!(
            summary(&diff),
            [
                (FindingKind::Missing, "set_carried_item", None, 1),
                (FindingKind::Extra, "player_loaded", None, 1),
                (FindingKind::Moved, "player_input", None, 1),
            ]
        );
        let missing = &diff.findings[0];
        assert_eq!(
            (missing.group, missing.reference, missing.trace),
            (GroupKey::Tick(1), Some(2), None)
        );
    }

    #[test]
    fn ticks_line_up_by_number_and_offset() {
        let reference = [input(1), tick_end()];
        let trace = [input(0), tick_end(), input(1), tick_end()];
        let diff = run(&reference, &trace, Options::default());
        assert_eq!(
            shape(&diff),
            ["tick 1: same 0/0, same 1/1", "tick 2: extra 2, extra 3"]
        );
        assert_eq!(
            diff.group_counts,
            GroupCounts {
                compared: 1,
                only_in_reference: 0,
                only_in_trace: 1
            }
        );
        assert_eq!(
            summary(&diff),
            [
                (FindingKind::Extra, "player_input", None, 1),
                (FindingKind::Extra, "client_tick_end", None, 1),
                (FindingKind::Field, "player_input", Some("input"), 1),
            ]
        );

        let shifted = run(
            &reference,
            &trace,
            Options {
                offset: 1,
                ..Options::default()
            },
        );
        assert_eq!(
            shape(&shifted),
            ["tick 0: extra 0, extra 1", "tick 1: same 0/2, same 1/3"]
        );
    }

    #[test]
    fn a_packet_a_few_ticks_early_is_shifted_not_missing() {
        let loaded = || game("player_loaded", &[]);
        let reference = [tick_end(), tick_end(), loaded(), tick_end()];
        let trace = [loaded(), tick_end(), tick_end(), tick_end()];
        let diff = run(&reference, &trace, Options::default());
        assert_eq!(
            shape(&diff),
            [
                "tick 1: same 0/1",
                "tick 2: same 1/2",
                "tick 3: shifted 2/0, same 3/3",
            ]
        );
        assert_eq!(
            summary(&diff),
            [(FindingKind::Shifted, "player_loaded", None, 1)]
        );
        assert_eq!(diff.findings[0].shift, Some(-2));

        // Outside the window it is a missing and an extra packet again.
        let narrow = run(
            &reference,
            &trace,
            Options {
                shift_window: Some(1),
                ..Options::default()
            },
        );
        assert_eq!(
            summary(&narrow),
            [
                (FindingKind::Missing, "player_loaded", None, 1),
                (FindingKind::Extra, "player_loaded", None, 1),
            ]
        );
    }

    #[test]
    fn field_differences_are_grouped_with_an_example() {
        let reference = [
            input(1),
            tick_end(),
            input(1),
            tick_end(),
            input(1),
            tick_end(),
        ];
        let trace = [
            input(0x11),
            tick_end(),
            input(1),
            tick_end(),
            input(0x11),
            tick_end(),
        ];
        let diff = run(&reference, &trace, Options::default());
        let [finding] = diff.findings.as_slice() else {
            panic!("{:?}", diff.findings)
        };
        assert_eq!((finding.count, finding.compared), (2, Some(3)));
        assert_eq!(finding.group, GroupKey::Tick(1));
        assert_eq!(finding.reference_value.as_deref(), Some("0x01 forward"));
        assert_eq!(finding.trace_value.as_deref(), Some("0x11 forward | jump"));
    }

    #[test]
    fn fields_on_one_side_differ() {
        let pack = |name: &str| {
            [
                &[9][..],
                b"minecraft",
                &[name.len() as u8],
                name.as_bytes(),
                &[4],
                b"26.3",
            ]
            .concat()
        };
        let packs = |names: &[&str]| {
            let mut body = vec![names.len() as u8];
            for name in names {
                body.extend(pack(name));
            }
            (
                Serverbound,
                Phase::Configuration,
                "select_known_packs",
                body,
            )
        };
        let diff = run(
            &[packs(&["core"])],
            &[packs(&["core", "extra"])],
            Options::default(),
        );
        assert_eq!(
            summary(&diff),
            [
                (
                    FindingKind::Field,
                    "select_known_packs",
                    Some("packs.1.namespace"),
                    1
                ),
                (
                    FindingKind::Field,
                    "select_known_packs",
                    Some("packs.1.id"),
                    1
                ),
                (
                    FindingKind::Field,
                    "select_known_packs",
                    Some("packs.1.version"),
                    1
                ),
            ]
        );
        assert_eq!(diff.findings[1].reference_value, None);
        assert_eq!(diff.findings[1].trace_value.as_deref(), Some("\"extra\""));

        let ignored = Options {
            ignores: vec!["select_known_packs.packs".into()],
            ..Options::default()
        };
        assert!(!run(&[packs(&["core"])], &[packs(&["core", "extra"])], ignored).differs());
    }

    #[test]
    fn default_ignores_skip_session_fields() {
        let position = |x: f64| {
            let body = [
                &x.to_be_bytes()[..],
                &64f64.to_be_bytes(),
                &0f64.to_be_bytes(),
                &[1],
            ]
            .concat();
            [game("move_player_pos", &body), tick_end()]
        };
        assert!(!run(&position(1.0), &position(2.0), Options::default()).differs());
        let all = Options {
            no_default_ignores: true,
            ..Options::default()
        };
        assert_eq!(
            summary(&run(&position(1.0), &position(2.0), all)),
            [(FindingKind::Field, "move_player_pos", Some("position"), 1)]
        );
    }

    #[test]
    fn ignore_rules_skip_packets_and_fields() {
        let reference = [input(1), tick_end()];
        let trace = [input(2), game("player_loaded", &[]), tick_end()];
        let with = |ignores: &[&str]| Options {
            ignores: ignores.iter().map(|rule| rule.to_string()).collect(),
            ..Options::default()
        };
        let whole_packets = run(&reference, &trace, with(&["player_input", "player_loaded"]));
        assert_eq!(shape(&whole_packets), ["tick 1: same 1/2"]);
        assert!(!whole_packets.differs());
        let wildcard = run(&reference, &trace, with(&["*.input"]));
        assert_eq!(
            summary(&wildcard),
            [(FindingKind::Extra, "player_loaded", None, 1)]
        );
        assert!(wildcard.ignores.contains(&"*.input".to_string()));
    }

    #[test]
    fn clientbound_packets_need_all_directions() {
        let keep_alive = (
            Clientbound,
            Phase::Game,
            "keep_alive",
            5i64.to_be_bytes().to_vec(),
        );
        let reference = [tick_end()];
        let trace = [keep_alive, tick_end()];
        let serverbound = run(&reference, &trace, Options::default());
        assert!(!serverbound.differs());
        assert_eq!(shape(&serverbound), ["tick 1: same 0/1"]);
        let all = run(
            &reference,
            &trace,
            Options {
                all_directions: true,
                ..Options::default()
            },
        );
        assert_eq!(summary(&all), [(FindingKind::Extra, "keep_alive", None, 1)]);
    }

    #[test]
    fn a_decode_failure_is_a_field_difference() {
        let status = |body: &[u8]| [game("move_player_status_only", body), tick_end()];
        let diff = run(&status(&[1]), &status(&[1, 9]), Options::default());
        let [finding] = diff.findings.as_slice() else {
            panic!("{:?}", diff.findings)
        };
        assert_eq!(finding.path.as_deref(), Some("decoding"));
        assert_eq!(finding.reference_value, None);
        assert_eq!(
            finding.trace_value.as_deref(),
            Some("stopped at byte 2: 1 byte left over after the last field")
        );
    }

    #[test]
    fn compares_one_version_at_a_time() {
        let packets = [tick_end()];
        let error = diff(
            &load(777, &packets),
            &load(776, &packets),
            &Options::default(),
        )
        .unwrap_err();
        assert_eq!(
            error,
            Error::Protocols {
                reference: 777,
                trace: 776
            }
        );
        let error = diff(&load(777, &[]), &load(777, &packets), &Options::default()).unwrap_err();
        assert_eq!(error, Error::NoPackets);
    }

    #[test]
    fn older_protocols_compare_approximate_windows() {
        let keep_alive = (
            Serverbound,
            Phase::Game,
            "keep_alive",
            5i64.to_be_bytes().to_vec(),
        );
        let packets = [keep_alive];
        let diff = diff(
            &load(767, &packets),
            &load(767, &packets),
            &Options::default(),
        )
        .unwrap();
        assert_eq!(diff.grouping, Grouping::Approximate);
        assert_eq!(diff.groups[0].key, GroupKey::Window(0));
    }

    #[test]
    fn serializes_for_the_viewer() {
        let reference = [input(1), tick_end()];
        let trace = [tick_end()];
        let json = serde_json::to_value(run(&reference, &trace, Options::default())).unwrap();
        assert_eq!(
            json["groups"][0]["key"],
            serde_json::json!({ "kind": "tick", "index": 1 })
        );
        assert_eq!(
            json["groups"][0]["rows"][0],
            serde_json::json!({ "kind": "missing", "reference": 0 })
        );
        let finding = &json["findings"][0];
        assert_eq!(finding["kind"], "missing");
        assert_eq!(finding["name"], "player_input");
        assert!(finding.get("path").is_none());
        let options: Options = serde_json::from_str(r#"{"offset": 2}"#).unwrap();
        assert_eq!((options.offset, options.all_directions), (2, false));
    }
}
