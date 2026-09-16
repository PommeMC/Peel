//! Per-connection protocol tracking: frames, compression and phases.
//!
//! Each direction is labelled the way its receiver decodes it. The receiver
//! switches its inbound protocol right after a terminal packet
//! (`Packet.isTerminal()` from 1.20.5, `nextProtocol()` in 1.20.2-1.20.4), so
//! the switches below apply from the next frame in that direction. 1.20.1
//! and older have one shared protocol and no configuration phase.

use anyhow::{Context, Result, ensure};
use peel_trace::{Direction, Phase, Record};
use pomme_protocol::PacketTable;
use pomme_protocol::wire::read_varint;

use crate::names;
use crate::wire::{self, FrameSplitter, Intent, Intention};

pub enum Notice {
    /// The client's handshake. `known` is whether pomme-protocol has tables
    /// for its protocol; without them frames are recorded without names.
    Handshake { intention: Intention, known: bool },
    /// The server asked for encryption: it runs in online mode.
    EncryptionRequested,
    /// Tracking stopped for the rest of the connection.
    Untracked(String),
}

#[derive(Default)]
pub struct Fed {
    pub records: Vec<Record>,
    pub notices: Vec<Notice>,
}

pub struct Session {
    splitters: [FrameSplitter; 2],
    handshake: Option<(i32, Option<&'static PacketTable>)>,
    phases: [Phase; 2],
    compressed: bool,
    tracking: bool,
}

impl Session {
    pub fn new() -> Self {
        Self {
            splitters: Default::default(),
            handshake: None,
            phases: [Phase::Handshake; 2],
            compressed: false,
            tracking: true,
        }
    }

    /// Parses bytes read from one direction. Once tracking stops, further
    /// bytes are ignored.
    pub fn feed(&mut self, direction: Direction, bytes: &[u8], timestamp_us: u64) -> Fed {
        let mut fed = Fed::default();
        if !self.tracking {
            return fed;
        }
        if direction == Direction::Clientbound && self.handshake.is_none() {
            self.untrack(
                &mut fed,
                "the server sent data before the client's handshake".into(),
            );
            return fed;
        }
        self.splitters[direction as usize].push(bytes);
        while self.tracking {
            let frame = self.splitters[direction as usize].next_frame();
            let result = frame.and_then(|frame| match frame {
                Some(frame) => self
                    .on_frame(direction, &frame, timestamp_us, &mut fed)
                    .map(|()| true),
                None => Ok(false),
            });
            match result {
                Ok(true) => {}
                Ok(false) => break,
                Err(err) => self.untrack(&mut fed, format!("{err:#}")),
            }
        }
        fed
    }

    fn untrack(&mut self, fed: &mut Fed, reason: String) {
        self.tracking = false;
        self.splitters = Default::default();
        fed.notices.push(Notice::Untracked(reason));
    }

    fn on_frame(
        &mut self,
        direction: Direction,
        frame: &[u8],
        timestamp_us: u64,
        fed: &mut Fed,
    ) -> Result<()> {
        let packet = if self.compressed {
            wire::decompress(frame)?
        } else {
            frame.to_vec()
        };
        let mut body_at = 0;
        let packet_id = read_varint(&packet, &mut body_at).context("frame has no packet id")?;
        let phase = self.phases[direction as usize];

        let (protocol, table) = match self.handshake {
            Some(handshake) => handshake,
            None => {
                let intention = wire::parse_intention(&packet[body_at..])?;
                let table = PacketTable::for_protocol(intention.protocol);
                if let Some(table) = table {
                    ensure!(
                        names::packet_name(table, phase, direction, packet_id) == Some("intention"),
                        "the first packet is not a handshake intention"
                    );
                }
                self.phases = [match intention.intent {
                    Intent::Status => Phase::Status,
                    Intent::Login | Intent::Transfer => Phase::Login,
                }; 2];
                self.handshake = Some((intention.protocol, table));
                fed.notices.push(Notice::Handshake {
                    intention,
                    known: table.is_some(),
                });
                (intention.protocol, table)
            }
        };

        fed.records.push(Record {
            timestamp_us,
            protocol,
            direction,
            phase,
            packet_id,
            frame: packet,
        });

        let Some(table) = table else {
            if (direction, phase) == (Direction::Clientbound, Phase::Login) {
                self.untrack(
                    fed,
                    format!(
                        "protocol {protocol} is not a version this build of Peel supports, so \
                         its compression and phase changes can't be followed past the first \
                         clientbound login packet"
                    ),
                );
            }
            return Ok(());
        };
        let body = &fed.records.last().expect("just pushed").frame[body_at..];
        let name = names::packet_name(table, phase, direction, packet_id);
        let receiver = &mut self.phases[direction as usize];
        match (direction, phase, name) {
            (Direction::Serverbound, Phase::Login, Some("login_acknowledged")) => {
                *receiver = Phase::Configuration;
            }
            (Direction::Serverbound, Phase::Configuration, Some("finish_configuration")) => {
                *receiver = Phase::Game;
            }
            (Direction::Serverbound, Phase::Game, Some("configuration_acknowledged")) => {
                *receiver = Phase::Configuration;
            }
            (Direction::Clientbound, Phase::Login, Some("hello")) => {
                self.tracking = false;
                fed.notices.push(Notice::EncryptionRequested);
            }
            (Direction::Clientbound, Phase::Login, Some("login_compression")) => {
                self.compressed = wire::read_threshold(body)? >= 0;
            }
            (Direction::Clientbound, Phase::Login, Some("login_finished" | "game_profile")) => {
                if names::packet_id(
                    table,
                    Phase::Login,
                    Direction::Serverbound,
                    "login_acknowledged",
                )
                .is_some()
                {
                    *receiver = Phase::Configuration;
                } else {
                    self.phases = [Phase::Game; 2];
                }
            }
            (Direction::Clientbound, Phase::Configuration, Some("finish_configuration")) => {
                *receiver = Phase::Game;
            }
            (Direction::Clientbound, Phase::Game, Some("start_configuration")) => {
                *receiver = Phase::Configuration;
            }
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use Direction::{Clientbound as Cb, Serverbound as Sb};

    use super::*;
    use crate::wire::test_frames::{
        LOGIN, STATUS, TRANSFER, frame, handshake, intention, named, packet,
    };

    struct Harness {
        session: Session,
        protocol: i32,
        table: &'static PacketTable,
        threshold: Option<usize>,
    }

    impl Harness {
        fn new(protocol: i32) -> Self {
            Self {
                session: Session::new(),
                protocol,
                table: PacketTable::for_protocol(protocol).unwrap(),
                threshold: None,
            }
        }

        /// Sends one framed packet, looked up by name in `phase`, and
        /// asserts the session recorded it byte-exact in that phase.
        fn send(&mut self, direction: Direction, phase: Phase, name: &str, body: &[u8]) -> Fed {
            let (id, packet) = named(self.table, phase, direction, name, body);
            let fed = self
                .session
                .feed(direction, &frame(&packet, self.threshold), 0);
            let untracked = fed.notices.iter().find_map(|n| match n {
                Notice::Untracked(reason) => Some(reason),
                _ => None,
            });
            assert_eq!(untracked, None, "{} {name}", self.protocol);
            let [record] = fed.records.as_slice() else {
                panic!("{} {name}: {} records", self.protocol, fed.records.len());
            };
            assert_eq!(
                (
                    record.direction,
                    record.phase,
                    record.packet_id,
                    record.protocol
                ),
                (direction, phase, id, self.protocol),
                "{name}"
            );
            assert_eq!(record.frame, packet, "{name}");
            fed
        }
    }

    fn login_flow(protocol: i32) {
        const THRESHOLD: usize = 16;
        let mut h = Harness::new(protocol);
        let fed = h.send(
            Sb,
            Phase::Handshake,
            "intention",
            &intention(protocol, LOGIN),
        );
        assert!(matches!(
            fed.notices.as_slice(),
            [Notice::Handshake { known: true, .. }]
        ));
        h.send(Sb, Phase::Login, "hello", b"player");
        h.send(Cb, Phase::Login, "login_compression", &[THRESHOLD as u8]);
        h.threshold = Some(THRESHOLD);
        let success = ["login_finished", "game_profile"]
            .into_iter()
            .find(|name| names::packet_id(h.table, Phase::Login, Cb, name).is_some())
            .unwrap();
        h.send(Cb, Phase::Login, success, &[1; 40]);

        let has_configuration =
            names::packet_id(h.table, Phase::Login, Sb, "login_acknowledged").is_some();
        assert_eq!(has_configuration, protocol != 763);
        if !has_configuration {
            h.send(Sb, Phase::Game, "keep_alive", &[0; 8]);
            h.send(Cb, Phase::Game, "login", &[2; 100]);
            return;
        }

        h.send(Sb, Phase::Login, "login_acknowledged", &[]);
        h.send(Cb, Phase::Configuration, "finish_configuration", &[]);
        h.send(Sb, Phase::Configuration, "finish_configuration", &[]);
        h.send(Cb, Phase::Game, "login", &[2; 100]);
        h.send(Sb, Phase::Game, "keep_alive", &[0; 8]);

        h.send(Cb, Phase::Game, "start_configuration", &[]);
        // The server keeps decoding game packets until the acknowledgement.
        h.send(Sb, Phase::Game, "keep_alive", &[0; 8]);
        h.send(Cb, Phase::Configuration, "keep_alive", &[0; 8]);
        h.send(Sb, Phase::Game, "configuration_acknowledged", &[]);
        h.send(Sb, Phase::Configuration, "keep_alive", &[0; 8]);
        h.send(Cb, Phase::Configuration, "finish_configuration", &[]);
        h.send(Sb, Phase::Configuration, "finish_configuration", &[]);
        h.send(Sb, Phase::Game, "keep_alive", &[0; 8]);
        h.send(Cb, Phase::Game, "keep_alive", &[0; 8]);
    }

    #[test]
    fn login_flow_763() {
        login_flow(763);
    }

    #[test]
    fn login_flow_764() {
        login_flow(764);
    }

    #[test]
    fn login_flow_768() {
        login_flow(768);
    }

    #[test]
    fn login_flow_776() {
        login_flow(776);
    }

    #[test]
    fn login_flow_777() {
        login_flow(777);
    }

    #[test]
    fn transfer_goes_to_login() {
        let mut h = Harness::new(776);
        h.send(Sb, Phase::Handshake, "intention", &intention(776, TRANSFER));
        h.send(Sb, Phase::Login, "hello", b"player");
    }

    #[test]
    fn status_connection() {
        let mut h = Harness::new(776);
        h.send(Sb, Phase::Handshake, "intention", &intention(776, STATUS));
        h.send(Sb, Phase::Status, "status_request", &[]);
        h.send(Cb, Phase::Status, "status_response", b"\x02{}");
        h.send(Sb, Phase::Status, "ping_request", &[0; 8]);
        h.send(Cb, Phase::Status, "pong_response", &[0; 8]);
    }

    #[test]
    fn encryption_request_stops() {
        let mut h = Harness::new(776);
        h.send(Sb, Phase::Handshake, "intention", &intention(776, LOGIN));
        h.send(Sb, Phase::Login, "hello", b"player");
        let fed = h.send(Cb, Phase::Login, "hello", &[0; 16]);
        assert!(matches!(
            fed.notices.as_slice(),
            [Notice::EncryptionRequested]
        ));
        let after = h.session.feed(Sb, &frame(&packet(1, &[0; 8]), None), 0);
        assert!(after.records.is_empty() && after.notices.is_empty());
    }

    /// A protocol pomme-protocol has no tables for, so ids are raw.
    const UNKNOWN: i32 = 12_345;

    fn unknown_protocol_session() -> impl FnMut(Direction, u32, &[u8]) -> Fed {
        assert!(PacketTable::for_protocol(UNKNOWN).is_none());
        let mut session = Session::new();
        move |direction, id, body| session.feed(direction, &frame(&packet(id, body), None), 0)
    }

    #[test]
    fn unknown_protocol_records_until_uncertain() {
        let mut feed = unknown_protocol_session();

        let fed = feed(Sb, 0, &intention(UNKNOWN, LOGIN));
        assert!(matches!(
            fed.notices.as_slice(),
            [Notice::Handshake { known: false, .. }]
        ));
        assert_eq!(fed.records[0].phase, Phase::Handshake);
        let fed = feed(Sb, 0, b"player");
        assert_eq!(fed.records[0].phase, Phase::Login);
        let fed = feed(Cb, 3, &[0]);
        assert_eq!(fed.records.len(), 1);
        assert!(matches!(fed.notices.as_slice(), [Notice::Untracked(_)]));
        assert!(feed(Cb, 2, &[0; 4]).records.is_empty());
    }

    #[test]
    fn unknown_protocol_status_is_recorded() {
        let mut feed = unknown_protocol_session();
        feed(Sb, 0, &intention(UNKNOWN, STATUS));
        for direction in [Sb, Cb, Sb, Cb] {
            let fed = feed(direction, 0, &[0; 8]);
            assert_eq!(fed.records[0].phase, Phase::Status);
            assert!(fed.notices.is_empty());
        }
    }

    #[test]
    fn untracked_before_or_without_a_handshake() {
        // A legacy server-list ping reply arrives before any handshake.
        let mut session = Session::new();
        let fed = session.feed(Cb, &[0xFF, 0x00, 0x17], 0);
        assert!(fed.records.is_empty());
        assert!(matches!(fed.notices.as_slice(), [Notice::Untracked(_)]));

        let mut session = Session::new();
        let fed = session.feed(Sb, &frame(&handshake(PacketTable::native(), 9), None), 0);
        assert!(fed.records.is_empty());
        assert!(matches!(fed.notices.as_slice(), [Notice::Untracked(_)]));
    }

    #[test]
    fn several_frames_in_one_feed_follow_compression() {
        let table = PacketTable::native();
        let mut session = Session::new();
        session.feed(Sb, &frame(&handshake(table, LOGIN), None), 0);
        let (_, compression) = named(table, Phase::Login, Cb, "login_compression", &[8]);
        let (_, finished) = named(table, Phase::Login, Cb, "login_finished", &[3; 64]);
        let mut bytes = frame(&compression, None);
        bytes.extend(frame(&finished, Some(8)));
        let fed = session.feed(Cb, &bytes, 0);
        assert!(fed.notices.is_empty());
        assert_eq!(fed.records.len(), 2);
        assert_eq!(fed.records[1].frame.len(), 65);
    }
}
