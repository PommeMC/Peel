//! `peel capture`: a transparent TCP proxy that records one login session.
//!
//! Bytes are forwarded exactly as read. Each read is parsed from a copy
//! before it is forwarded, so phase and compression changes are applied
//! before the peer can react to them; nothing is rewritten, injected,
//! dropped, reordered or held back beyond that parse. Writing and
//! compressing the trace happens on its own thread, off that path.

use std::fs::{File, OpenOptions};
use std::io::{self, BufWriter, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use peel_trace::{Direction, Header, Record, TraceWriter};
use pomme_protocol::ProtocolVersion;

use crate::names::TickMarker;
use crate::session::{Notice, Session};
use crate::status::{self, note};
use crate::wire::{Intent, Intention};

const OFFLINE_ONLY: &str = "the server requested encryption, so it is running with \
    online-mode=true. Peel only captures offline-mode servers: it never handles Microsoft or \
    Mojang credentials or session keys. Set online-mode=false in the test server's \
    server.properties and connect again.";

/// How often the recorder flushes the trace and redraws the status line. A
/// killed capture loses at most this much.
const FLUSH_INTERVAL: Duration = Duration::from_millis(100);

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
    /// Overwrite the trace file if it already exists.
    #[arg(long)]
    force: bool,
}

pub fn run(args: Args) -> Result<()> {
    if args.out.exists() && !args.force {
        bail!(
            "{} already exists; pass --force to overwrite it",
            args.out.display()
        );
    }
    let listener =
        TcpListener::bind(args.listen).with_context(|| format!("listening on {}", args.listen))?;
    note!(
        "connect the client to {}; forwarding to {}",
        args.listen,
        args.server
    );
    let (_, failure) = serve(listener, &args.server, opener(args.out.clone(), args.force))?;
    note!("trace written to {}", args.out.display());
    failure.map_or(Ok(()), Err)
}

/// Creates the trace file once a login is recorded. Without `force` it never
/// replaces a file, including one created after capture started.
fn opener(path: PathBuf, force: bool) -> impl FnOnce() -> io::Result<BufWriter<File>> {
    move || {
        let mut options = OpenOptions::new();
        if force {
            options.write(true).create(true).truncate(true);
        } else {
            options.write(true).create_new(true);
        }
        options
            .open(&path)
            .map(BufWriter::new)
            .map_err(|err| io::Error::new(err.kind(), format!("{}: {err}", path.display())))
    }
}

type Opener<W> = Box<dyn FnOnce() -> io::Result<W> + Send>;

struct Capture<W> {
    /// Taken by the first login, which records the trace.
    open: Mutex<Option<Opener<W>>>,
    finished: mpsc::Sender<Finished<W>>,
}

/// The recorded connection's end: its recorder thread, and why the
/// connection ended capture early, if it did.
struct Finished<W> {
    recorder: JoinHandle<io::Result<W>>,
    failure: Option<anyhow::Error>,
}

/// Accepts connections until the first login connection, the one recorded,
/// closes. Its trace starts when it was accepted, and `open` is only called
/// then. Returns the trace's finished writer and, if the recorded connection
/// ended capture early, why.
pub fn serve<W: Write + Send + 'static>(
    listener: TcpListener,
    server: &str,
    open: impl FnOnce() -> io::Result<W> + Send + 'static,
) -> Result<(W, Option<anyhow::Error>)> {
    let (finished, done) = mpsc::channel();
    let capture = Arc::new(Capture {
        open: Mutex::new(Some(Box::new(open))),
        finished,
    });

    listener.set_nonblocking(true)?;
    let Finished { recorder, failure } = loop {
        match listener.accept() {
            Ok((client, peer)) => {
                let capture = capture.clone();
                let server = server.to_owned();
                thread::spawn(move || {
                    if let Err(err) = connection(client, peer, &server, &capture) {
                        note!("{peer}: {err:#}");
                    }
                });
            }
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                match done.recv_timeout(Duration::from_millis(20)) {
                    Ok(finished) => break finished,
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => unreachable!("capture holds a sender"),
                }
            }
            Err(err) => return Err(err).context("accepting a connection"),
        }
    };

    let out = recorder
        .join()
        .expect("recorder thread panicked")
        .context("writing the trace")?;
    Ok((out, failure))
}

struct Connection<W> {
    peer: SocketAddr,
    accepted: Instant,
    accepted_unix_ms: u64,
    session: Session,
    recording: Option<Recording<W>>,
    failure: Option<anyhow::Error>,
}

struct Recording<W> {
    records: mpsc::Sender<Vec<Record>>,
    recorder: JoinHandle<io::Result<W>>,
}

fn connection<W: Write + Send + 'static>(
    client: TcpStream,
    peer: SocketAddr,
    server: &str,
    capture: &Capture<W>,
) -> Result<()> {
    let accepted = Instant::now();
    let accepted_unix_ms = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;
    // Accepted sockets can inherit the listener's non-blocking mode.
    client.set_nonblocking(false)?;
    let server = TcpStream::connect(server).with_context(|| format!("connecting to {server}"))?;
    client.set_nodelay(true)?;
    server.set_nodelay(true)?;

    let conn = Mutex::new(Connection {
        peer,
        accepted,
        accepted_unix_ms,
        session: Session::new(),
        recording: None,
        failure: None,
    });
    thread::scope(|scope| {
        scope.spawn(|| pump(Direction::Serverbound, &client, &server, &conn, capture));
        pump(Direction::Clientbound, &server, &client, &conn, capture);
    });

    let conn = conn.into_inner().unwrap();
    match conn.recording {
        Some(Recording { recorder, .. }) => capture
            .finished
            .send(Finished {
                recorder,
                failure: conn.failure,
            })
            .expect("serve waits for the recorded connection"),
        None => {
            if let Some(err) = conn.failure {
                note!("{peer}: {err:#}");
            }
        }
    }
    Ok(())
}

fn pump<W: Write + Send + 'static>(
    direction: Direction,
    mut from: &TcpStream,
    mut to: &TcpStream,
    conn: &Mutex<Connection<W>>,
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

impl<W: Write + Send + 'static> Connection<W> {
    /// Parses and records a copy of `bytes`; returns whether to forward them.
    fn observe(&mut self, direction: Direction, bytes: &[u8], capture: &Capture<W>) -> bool {
        let timestamp_us = self.accepted.elapsed().as_micros() as u64;
        let fed = self.session.feed(direction, bytes, timestamp_us);
        let mut forward = true;
        for notice in fed.notices {
            match notice {
                Notice::Handshake { intention, known } => {
                    self.on_handshake(intention, known, capture)
                }
                Notice::Untracked(reason) => note!(
                    "{}: {reason}; forwarding continues without recording",
                    self.peer
                ),
                Notice::EncryptionRequested => forward = false,
            }
        }
        if let Some(recording) = &self.recording
            && !fed.records.is_empty()
        {
            // A failed send means the recorder already stopped and said why.
            let _ = recording.records.send(fed.records);
        }
        if !forward {
            self.failure = Some(anyhow!(OFFLINE_ONLY));
        }
        forward
    }

    fn on_handshake(&mut self, intention: Intention, known: bool, capture: &Capture<W>) {
        let peer = self.peer;
        let protocol = intention.protocol;
        if !known {
            note!(
                "{peer}: protocol {protocol} is not a version this build of Peel supports; \
                 forwarding transparently, packets have no names"
            );
        }
        if intention.intent == Intent::Status {
            note!("{peer}: status ping, forwarded but not recorded");
            return;
        }
        let Some(open) = capture.open.lock().unwrap().take() else {
            note!("{peer}: a login is already being recorded; forwarding this one only");
            return;
        };
        let version = ProtocolVersion::from_protocol(protocol).map_or("unsupported", |v| v.name);
        note!("{peer}: recording login, protocol {protocol} ({version})");
        let header = Header {
            started_at_unix_ms: self.accepted_unix_ms,
            recorder: format!("peel {}", env!("CARGO_PKG_VERSION")),
        };
        let marker = TickMarker::for_protocol(protocol);
        let (records, received) = mpsc::channel();
        let recorder = thread::spawn(move || record(open, &header, marker, &received));
        self.recording = Some(Recording { records, recorder });
    }
}

/// The recorder thread: writes batches as they arrive, flushing and showing
/// progress every [`FLUSH_INTERVAL`], until the recorded connection hangs up.
fn record<W: Write>(
    open: Opener<W>,
    header: &Header,
    marker: Option<TickMarker>,
    received: &mpsc::Receiver<Vec<Record>>,
) -> io::Result<W> {
    let result = (|| {
        let mut writer = TraceWriter::new(open()?, header)?;
        let (mut packets, mut ticks) = (0, 0);
        let mut flushed = Instant::now();
        loop {
            let batch = received.recv_timeout(FLUSH_INTERVAL);
            for record in batch.iter().flatten() {
                writer.write(record)?;
                packets += 1;
                ticks += u64::from(marker.is_some_and(|m| m.ends_tick(record)));
            }
            let hung_up = matches!(batch, Err(RecvTimeoutError::Disconnected));
            if hung_up || flushed.elapsed() >= FLUSH_INTERVAL {
                writer.flush()?;
                flushed = Instant::now();
                let progress =
                    status::progress(packets, marker.map(|_| ticks), writer.compressed_len());
                if hung_up {
                    note!("recorded {progress}");
                    return writer.into_inner();
                }
                status::show(&format!("recording: {progress}"));
            }
        }
    })();
    if let Err(err) = &result {
        note!("writing the trace failed: {err}");
    }
    result
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::atomic::{AtomicBool, Ordering};

    use peel_trace::{Phase, TraceReader};
    use pomme_protocol::PacketTable;
    use pomme_protocol::version::LATEST;

    use super::*;
    use crate::wire::test_frames::{LOGIN, STATUS, frame, handshake, intention, latest, named};

    struct Proxy {
        proxy: SocketAddr,
        server: TcpListener,
        handle: JoinHandle<Result<(Vec<u8>, Option<anyhow::Error>)>>,
        /// Whether a login has opened the trace yet.
        opened: Arc<AtomicBool>,
    }

    fn start() -> Proxy {
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let proxy = TcpListener::bind("127.0.0.1:0").unwrap();
        let proxy_addr = proxy.local_addr().unwrap();
        let server_addr = server.local_addr().unwrap().to_string();
        let opened = Arc::new(AtomicBool::new(false));
        let flag = opened.clone();
        let handle = thread::spawn(move || {
            serve(proxy, &server_addr, move || {
                flag.store(true, Ordering::SeqCst);
                Ok(Vec::new())
            })
        });
        Proxy {
            proxy: proxy_addr,
            server,
            handle,
            opened,
        }
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

    /// A login handshake followed by `hello`, as one write.
    fn login_request(table: &PacketTable, hello: &[u8]) -> Vec<u8> {
        let (_, hello) = named(table, Phase::Login, Direction::Serverbound, "hello", hello);
        let mut request = frame(&handshake(table, LOGIN), None);
        request.extend(frame(&hello, None));
        request
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

        let Proxy {
            proxy,
            server,
            handle,
            opened,
        } = start();
        let table = latest();
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
        assert!(
            !opened.load(Ordering::SeqCst),
            "a status ping opened the trace"
        );

        // The trace clock starts at this connection, not when capture began.
        const IDLE: Duration = Duration::from_millis(300);
        thread::sleep(IDLE);
        let mut client = TcpStream::connect(proxy).unwrap();
        let (mut upstream, _) = server.accept().unwrap();
        client.set_nodelay(true).unwrap();

        // Several frames in one write.
        let mut hello = s.frame(
            Sb,
            Phase::Handshake,
            "intention",
            &intention(LATEST.protocol, LOGIN),
        );
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
        assert!(u128::from(records[0].timestamp_us) < IDLE.as_micros());
        assert!(
            records
                .windows(2)
                .all(|w| w[0].timestamp_us <= w[1].timestamp_us)
        );
        assert!(records.iter().all(|r| r.protocol == LATEST.protocol));
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
        let Proxy {
            proxy,
            server,
            handle,
            ..
        } = start();
        let table = latest();

        let mut client = TcpStream::connect(proxy).unwrap();
        let (mut upstream, _) = server.accept().unwrap();
        let request = login_request(table, b"\x06player");
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

    #[test]
    fn later_logins_are_forwarded_not_recorded() {
        let Proxy {
            proxy,
            server,
            handle,
            ..
        } = start();
        let table = latest();

        let first = frame(&handshake(table, LOGIN), None);
        let mut recorded = TcpStream::connect(proxy).unwrap();
        let (mut recorded_upstream, _) = server.accept().unwrap();
        recorded.write_all(&first).unwrap();
        expect_bytes(&mut recorded_upstream, &first);

        let request = login_request(table, b"\x06second");
        let mut second = TcpStream::connect(proxy).unwrap();
        let (mut second_upstream, _) = server.accept().unwrap();
        second.write_all(&request).unwrap();
        expect_bytes(&mut second_upstream, &request);
        drop((second, second_upstream));

        drop(recorded);
        assert_closed(&mut recorded_upstream);
        drop(recorded_upstream);

        let (trace, failure) = handle.join().unwrap().unwrap();
        assert!(failure.is_none(), "{failure:?}");
        assert_eq!(read_trace(&trace).len(), 1);
    }

    #[test]
    fn opener_refuses_to_overwrite_without_force() {
        let dir = std::env::temp_dir().join(format!("peel-opener-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("session.trace");
        let _ = fs::remove_file(&path);

        opener(path.clone(), false)()
            .unwrap()
            .write_all(b"old")
            .unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"old");
        let err = opener(path.clone(), false)().unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
        assert!(err.to_string().contains("session.trace"), "{err}");

        drop(opener(path.clone(), true)().unwrap());
        assert!(fs::read(&path).unwrap().is_empty());
        fs::remove_dir_all(&dir).unwrap();
    }
}
