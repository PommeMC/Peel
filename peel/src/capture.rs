//! `peel capture`: a transparent TCP proxy that records one login session.
//!
//! Bytes are forwarded exactly as read. Each read is parsed from a copy
//! before it is forwarded, so phase and compression changes are applied
//! before the peer can react to them; nothing is rewritten, injected,
//! dropped, reordered or held back beyond that parse.

use std::fs::File;
use std::io::{self, BufWriter, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow};
use peel_trace::{Direction, Header, Record, TraceWriter};
use pomme_protocol::ProtocolVersion;

use crate::session::{Notice, Session};
use crate::wire::{Intent, Intention};

const OFFLINE_ONLY: &str = "the server requested encryption, so it is running with \
    online-mode=true. Peel only captures offline-mode servers: it never handles Microsoft or \
    Mojang credentials or session keys. Set online-mode=false in the test server's \
    server.properties and connect again.";

#[derive(clap::Args)]
pub struct Args {
    /// Address to point the Minecraft client at.
    #[arg(long, default_value = "127.0.0.1:25566")]
    listen: SocketAddr,
    /// The offline-mode server to forward to.
    #[arg(long, default_value = "127.0.0.1:25565")]
    server: String,
    /// Trace file to write.
    #[arg(long)]
    out: PathBuf,
}

pub fn run(args: Args) -> Result<()> {
    let listener =
        TcpListener::bind(args.listen).with_context(|| format!("listening on {}", args.listen))?;
    let out =
        File::create(&args.out).with_context(|| format!("creating {}", args.out.display()))?;
    eprintln!(
        "peel: connect the client to {}; forwarding to {}",
        args.listen, args.server
    );
    let (_, failure) = serve(listener, &args.server, BufWriter::new(out))?;
    eprintln!("peel: trace written to {}", args.out.display());
    failure.map_or(Ok(()), Err)
}

struct Capture<W: Write> {
    started: Instant,
    recorder: Mutex<Recorder<W>>,
    finished: mpsc::Sender<Option<anyhow::Error>>,
}

struct Recorder<W: Write> {
    writer: Option<TraceWriter<W>>,
    claimed: bool,
}

/// Accepts connections until the first login connection, the one recorded,
/// closes. Returns the trace's flushed writer and, if the recorded connection
/// ended capture early, why.
pub fn serve<W: Write + Send + 'static>(
    listener: TcpListener,
    server: &str,
    out: W,
) -> Result<(W, Option<anyhow::Error>)> {
    let started_at_unix_ms = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;
    let header = Header {
        started_at_unix_ms,
        recorder: format!("peel {}", env!("CARGO_PKG_VERSION")),
    };
    let (finished, done) = mpsc::channel();
    let capture = Arc::new(Capture {
        started: Instant::now(),
        recorder: Mutex::new(Recorder {
            writer: Some(TraceWriter::new(out, &header)?),
            claimed: false,
        }),
        finished,
    });

    listener.set_nonblocking(true)?;
    let failure = loop {
        match listener.accept() {
            Ok((client, peer)) => {
                let capture = capture.clone();
                let server = server.to_owned();
                thread::spawn(move || {
                    if let Err(err) = connection(client, peer, &server, &capture) {
                        eprintln!("peel: {peer}: {err:#}");
                    }
                });
            }
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                match done.recv_timeout(Duration::from_millis(20)) {
                    Ok(failure) => break failure,
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => unreachable!("capture holds a sender"),
                }
            }
            Err(err) => return Err(err).context("accepting a connection"),
        }
    };

    let writer = capture.recorder.lock().unwrap().writer.take();
    Ok((writer.expect("taken once").into_inner()?, failure))
}

struct Connection {
    peer: SocketAddr,
    session: Session,
    recorded: bool,
    failure: Option<anyhow::Error>,
}

fn connection<W: Write + Send>(
    client: TcpStream,
    peer: SocketAddr,
    server: &str,
    capture: &Capture<W>,
) -> Result<()> {
    // Accepted sockets can inherit the listener's non-blocking mode.
    client.set_nonblocking(false)?;
    let server = TcpStream::connect(server).with_context(|| format!("connecting to {server}"))?;
    client.set_nodelay(true)?;
    server.set_nodelay(true)?;

    let conn = Mutex::new(Connection {
        peer,
        session: Session::new(),
        recorded: false,
        failure: None,
    });
    thread::scope(|scope| {
        scope.spawn(|| pump(Direction::Serverbound, &client, &server, &conn, capture));
        pump(Direction::Clientbound, &server, &client, &conn, capture);
    });

    let conn = conn.into_inner().unwrap();
    if conn.recorded {
        capture
            .finished
            .send(conn.failure)
            .expect("serve waits for the recorded connection");
    } else if let Some(err) = conn.failure {
        eprintln!("peel: {peer}: {err:#}");
    }
    Ok(())
}

fn pump<W: Write>(
    direction: Direction,
    mut from: &TcpStream,
    mut to: &TcpStream,
    conn: &Mutex<Connection>,
    capture: &Capture<W>,
) {
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = match from.read(&mut buf) {
            Ok(0) => {
                let _ = to.shutdown(Shutdown::Write);
                return;
            }
            Ok(n) => n,
            Err(_) => break,
        };
        let forward = conn.lock().unwrap().observe(direction, &buf[..n], capture);
        if !forward || to.write_all(&buf[..n]).is_err() {
            break;
        }
    }
    let _ = from.shutdown(Shutdown::Both);
    let _ = to.shutdown(Shutdown::Both);
}

impl Connection {
    /// Parses and records a copy of `bytes`; returns whether to forward them.
    fn observe<W: Write>(
        &mut self,
        direction: Direction,
        bytes: &[u8],
        capture: &Capture<W>,
    ) -> bool {
        let timestamp_us = capture.started.elapsed().as_micros() as u64;
        let fed = self.session.feed(direction, bytes, timestamp_us);
        let mut forward = true;
        for notice in fed.notices {
            match notice {
                Notice::Handshake { intention, known } => {
                    self.on_handshake(intention, known, capture)
                }
                Notice::Untracked(reason) => eprintln!(
                    "peel: {}: {reason}; forwarding continues without recording",
                    self.peer
                ),
                Notice::EncryptionRequested => forward = false,
            }
        }
        if self.recorded {
            self.record(&fed.records, capture);
        }
        if !forward {
            self.failure = Some(anyhow!(OFFLINE_ONLY));
        }
        forward
    }

    fn on_handshake<W: Write>(&mut self, intention: Intention, known: bool, capture: &Capture<W>) {
        let peer = self.peer;
        let protocol = intention.protocol;
        if !known {
            eprintln!(
                "peel: {peer}: protocol {protocol} is not a version this build of Peel supports; \
                 forwarding transparently, packets have no names"
            );
        }
        if intention.intent == Intent::Status {
            eprintln!("peel: {peer}: status ping, forwarded but not recorded");
            return;
        }
        let mut recorder = capture.recorder.lock().unwrap();
        if recorder.claimed {
            eprintln!("peel: {peer}: a login is already being recorded; forwarding this one only");
            return;
        }
        recorder.claimed = true;
        self.recorded = true;
        let version = ProtocolVersion::from_protocol(protocol).map_or("unsupported", |v| v.name);
        eprintln!("peel: {peer}: recording login, protocol {protocol} ({version})");
    }

    fn record<W: Write>(&mut self, records: &[Record], capture: &Capture<W>) {
        if records.is_empty() || self.failure.is_some() {
            return;
        }
        let mut recorder = capture.recorder.lock().unwrap();
        let writer = recorder
            .writer
            .as_mut()
            .expect("writer lives until capture ends");
        let written = records
            .iter()
            .try_for_each(|record| writer.write(record))
            .and_then(|()| writer.flush());
        if let Err(err) = written {
            eprintln!("peel: {}: writing the trace failed: {err}", self.peer);
            self.failure = Some(anyhow::Error::new(err).context("writing the trace"));
        }
    }
}

#[cfg(test)]
mod tests {
    use peel_trace::{Phase, TraceReader};
    use pomme_protocol::PacketTable;

    use super::*;
    use crate::wire::test_frames::{LOGIN, STATUS, frame, handshake, intention, named};

    type Serve = thread::JoinHandle<Result<(Vec<u8>, Option<anyhow::Error>)>>;

    fn start() -> (SocketAddr, TcpListener, Serve) {
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let proxy = TcpListener::bind("127.0.0.1:0").unwrap();
        let proxy_addr = proxy.local_addr().unwrap();
        let server_addr = server.local_addr().unwrap().to_string();
        let handle = thread::spawn(move || serve(proxy, &server_addr, Vec::new()));
        (proxy_addr, server, handle)
    }

    struct Script {
        table: &'static PacketTable,
        expected: Vec<(Direction, Phase, u32, Vec<u8>)>,
        threshold: Option<usize>,
    }

    impl Script {
        /// Frames `name` as the sender would and notes the record it should
        /// produce.
        fn frame(
            &mut self,
            direction: Direction,
            phase: Phase,
            name: &str,
            body: &[u8],
        ) -> Vec<u8> {
            let (id, packet) = named(self.table, phase, direction, name, body);
            let framed = frame(&packet, self.threshold);
            self.expected.push((direction, phase, id, packet));
            framed
        }
    }

    fn expect_bytes(stream: &mut TcpStream, expected: &[u8]) {
        let mut got = vec![0u8; expected.len()];
        stream.read_exact(&mut got).unwrap();
        assert!(got == expected, "forwarded bytes differ");
    }

    fn write_split(stream: &mut TcpStream, chunks: &[Vec<u8>]) {
        for chunk in chunks {
            stream.write_all(chunk).unwrap();
        }
    }

    fn assert_closed(stream: &mut TcpStream) {
        let mut rest = Vec::new();
        let _ = stream.read_to_end(&mut rest);
        assert!(rest.is_empty(), "{} unexpected bytes", rest.len());
    }

    fn read_trace(trace: &[u8]) -> Vec<Record> {
        TraceReader::new(trace)
            .unwrap()
            .collect::<peel_trace::Result<_>>()
            .unwrap()
    }

    #[test]
    fn records_login_and_forwards_byte_identical() {
        use Direction::{Clientbound as Cb, Serverbound as Sb};
        const THRESHOLD: usize = 64;

        let (proxy, server, handle) = start();
        let table = PacketTable::native();
        let mut s = Script {
            table,
            expected: Vec::new(),
            threshold: None,
        };

        // A server-list ping first, which is forwarded but not recorded.
        let (_, status_response) = named(table, Phase::Status, Cb, "status_response", b"\x02{}");
        let status_response = frame(&status_response, None);
        let (_, status_request) = named(table, Phase::Status, Sb, "status_request", &[]);
        let mut request = frame(&handshake(table, STATUS), None);
        request.extend(frame(&status_request, None));
        thread::scope(|scope| {
            let fake_server = scope.spawn(|| {
                let (mut conn, _) = server.accept().unwrap();
                let mut received = Vec::new();
                conn.read_to_end(&mut received).unwrap();
                conn.write_all(&status_response).unwrap();
                received
            });
            let mut client = TcpStream::connect(proxy).unwrap();
            client.write_all(&request).unwrap();
            client.shutdown(Shutdown::Write).unwrap();
            let mut response = Vec::new();
            client.read_to_end(&mut response).unwrap();
            assert_eq!(response, status_response);
            assert_eq!(fake_server.join().unwrap(), request);
        });

        let mut client = TcpStream::connect(proxy).unwrap();
        let (mut upstream, _) = server.accept().unwrap();
        client.set_nodelay(true).unwrap();

        // Several frames in one write.
        let mut hello = s.frame(Sb, Phase::Handshake, "intention", &intention(776, LOGIN));
        hello.extend(s.frame(Sb, Phase::Login, "hello", b"\x06player"));
        client.write_all(&hello).unwrap();
        expect_bytes(&mut upstream, &hello);

        // Compression starts partway through one read.
        let mut bytes = s.frame(Cb, Phase::Login, "login_compression", &[THRESHOLD as u8]);
        s.threshold = Some(THRESHOLD);
        bytes.extend(s.frame(Cb, Phase::Login, "login_finished", &[1; 20]));
        upstream.write_all(&bytes).unwrap();
        expect_bytes(&mut client, &bytes);

        // One frame written a byte at a time.
        let ack = s.frame(Sb, Phase::Login, "login_acknowledged", &[]);
        write_split(
            &mut client,
            &ack.iter().map(|&b| vec![b]).collect::<Vec<_>>(),
        );
        expect_bytes(&mut upstream, &ack);

        let bytes = s.frame(Cb, Phase::Configuration, "finish_configuration", &[]);
        upstream.write_all(&bytes).unwrap();
        expect_bytes(&mut client, &bytes);
        let bytes = s.frame(Sb, Phase::Configuration, "finish_configuration", &[]);
        client.write_all(&bytes).unwrap();
        expect_bytes(&mut upstream, &bytes);

        // A large zlib frame split across writes, then a small one.
        let chunk: Vec<u8> = (0..20_000u32).map(|i| (i * 7 % 251) as u8).collect();
        let mut bytes = s.frame(Cb, Phase::Game, "level_chunk_with_light", &chunk);
        bytes.extend(s.frame(Cb, Phase::Game, "keep_alive", &[0; 8]));
        let (head, tail) = bytes.split_at(bytes.len() / 2);
        write_split(&mut upstream, &[head.to_vec(), tail.to_vec()]);
        expect_bytes(&mut client, &bytes);
        let bytes = s.frame(Sb, Phase::Game, "client_tick_end", &[]);
        client.write_all(&bytes).unwrap();
        expect_bytes(&mut upstream, &bytes);

        upstream.shutdown(Shutdown::Write).unwrap();
        assert_closed(&mut client);
        drop(client);
        assert_closed(&mut upstream);

        let (trace, failure) = handle.join().unwrap().unwrap();
        assert!(failure.is_none(), "{failure:?}");
        let records = read_trace(&trace);
        assert!(
            records
                .windows(2)
                .all(|w| w[0].timestamp_us <= w[1].timestamp_us)
        );
        assert!(records.iter().all(|r| r.protocol == 776));
        let got: Vec<_> = records
            .into_iter()
            .map(|r| (r.direction, r.phase, r.packet_id, r.frame))
            .collect();
        assert!(
            got == s.expected,
            "recorded {} of {} packets, or they differ",
            got.len(),
            s.expected.len()
        );
    }

    #[test]
    fn encryption_request_stops_capture() {
        let (proxy, server, handle) = start();
        let table = PacketTable::native();

        let mut client = TcpStream::connect(proxy).unwrap();
        let (mut upstream, _) = server.accept().unwrap();
        let (_, hello) = named(
            table,
            Phase::Login,
            Direction::Serverbound,
            "hello",
            b"\x06player",
        );
        let mut request = frame(&handshake(table, LOGIN), None);
        request.extend(frame(&hello, None));
        client.write_all(&request).unwrap();
        expect_bytes(&mut upstream, &request);

        let (_, encryption) = named(
            table,
            Phase::Login,
            Direction::Clientbound,
            "hello",
            &[0; 32],
        );
        let encryption = frame(&encryption, None);
        upstream.write_all(&encryption).unwrap();
        assert_closed(&mut client);
        assert_closed(&mut upstream);
        drop((client, upstream));

        let (trace, failure) = handle.join().unwrap().unwrap();
        let err = failure.unwrap();
        assert!(err.to_string().contains("online-mode=false"), "{err}");
        // The request itself is recorded, so the trace shows why it ended.
        let records = read_trace(&trace);
        let last = records.last().unwrap();
        assert_eq!(records.len(), 3);
        assert_eq!(
            (last.direction, last.phase, last.frame.as_slice()),
            (Direction::Clientbound, Phase::Login, &encryption[1..])
        );
    }
}
