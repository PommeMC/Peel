//! `peel capture`: a transparent TCP proxy that records one login session.
//!
//! Bytes are forwarded exactly as read. Each read is parsed from a copy
//! before it is forwarded, so phase and compression changes are applied
//! before the peer can react to them; nothing is rewritten, injected,
//! dropped, reordered or held back beyond that parse. Writing and
//! compressing the trace happens on its own thread, off that path.
//!
//! Capture can also run unattended: it reports the address it listens on,
//! runs a command against itself, and gives up after `--connect-timeout` or
//! `--max-duration`. Stopping only ever closes connections; what was
//! forwarded until then was forwarded untouched.

use std::ffi::OsString;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, Shutdown, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use peel_decode::names::TickMarker;
use peel_trace::{Direction, Header, Record, TraceWriter};
use pomme_protocol::ProtocolVersion;
use serde::Serialize;

use crate::cli::exit;
use crate::session::{Notice, Session};
use crate::status::{self, note};
use crate::wire::{Intent, Intention};

/// Why the recorded connection made capture stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    EncryptionRequested,
    Unsupported(i32),
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::EncryptionRequested => f.write_str(
                "the server requested encryption, so it is running with online-mode=true. Peel \
                 only captures offline-mode servers: it never handles Microsoft or Mojang \
                 credentials or session keys. Set online-mode=false in the test server's \
                 server.properties and connect again.",
            ),
            Self::Unsupported(protocol) => write!(
                f,
                "protocol {protocol} isn't a version this build of Peel supports, so it can't \
                 tell an online-mode login from an offline one. Update Peel, or capture a \
                 supported version."
            ),
        }
    }
}

/// Batches of records waiting to be written. Bounded so a fast server can't
/// outrun the recorder into memory the capture never gets back.
const PENDING_BATCHES: usize = 64;

/// How often the recorder flushes the trace and redraws the status line. A
/// killed capture loses at most this much.
const FLUSH_INTERVAL: Duration = Duration::from_millis(100);

/// How often capture looks for a new connection, a timeout, or the end of
/// the command it runs.
const POLL: Duration = Duration::from_millis(20);

/// How long the recorded connection gets to close by itself once the command
/// has exited, before capture closes it.
const EXIT_GRACE: Duration = Duration::from_secs(5);

/// How long a connection capture has closed gets to finish its trace.
const CLOSE_WAIT: Duration = Duration::from_secs(10);

#[derive(clap::Args)]
pub struct Args {
    /// Address to point the Minecraft client at. Port 0 picks a free port,
    /// reported on stdout and in `--ready-file`.
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
    /// Write the address being listened on to this file once the proxy
    /// accepts connections. A file already there is removed first.
    #[arg(long, value_name = "PATH")]
    ready_file: Option<PathBuf>,
    /// Stop and exit 124 if no client has logged in after this many seconds.
    #[arg(long, value_name = "SECS", value_parser = seconds)]
    connect_timeout: Option<Duration>,
    /// Stop and exit 124 if capture is still running this many seconds after
    /// the proxy started listening. The trace keeps what was recorded.
    #[arg(long, value_name = "SECS", value_parser = seconds)]
    max_duration: Option<Duration>,
    /// Print only errors on stderr.
    #[arg(long)]
    quiet: bool,
    /// Print stdout as JSON lines: a `listening` event, then a `finished`
    /// summary.
    #[arg(long)]
    json: bool,
    /// Run this command with the proxy's address in PEEL_ADDR, PEEL_HOST and
    /// PEEL_PORT, and in place of {addr}, {host} and {port} in its arguments.
    /// Capture stops when it exits, and exits with its status. Its stdout
    /// goes to stderr.
    #[arg(last = true, value_name = "COMMAND")]
    command: Vec<OsString>,
}

fn seconds(text: &str) -> Result<Duration, String> {
    let secs: f64 = text.parse().map_err(|_| "not a number of seconds")?;
    if secs <= 0.0 {
        return Err("must be more than 0".into());
    }
    Duration::try_from_secs_f64(secs).map_err(|err| err.to_string())
}

/// What `--json` prints when capture ends, however it ends.
#[derive(Debug, Serialize)]
pub struct Report {
    event: &'static str,
    /// The trace file, `null` when no login was recorded.
    trace: Option<PathBuf>,
    protocol: Option<i32>,
    /// `null` for a protocol this build has no tables for.
    version: Option<&'static str>,
    packets: Packets,
    /// How long the recorded connection was open.
    duration_ms: u64,
    stopped: &'static str,
    command_exit_code: Option<i32>,
    exit_code: i32,
    error: Option<String>,
}

/// Packets recorded in each direction.
#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize)]
pub struct Packets {
    serverbound: u64,
    clientbound: u64,
}

/// The process exit code.
pub fn run(args: Args) -> i32 {
    status::set_quiet(args.quiet);
    let mut stdout = io::stdout();
    let report = capture(&args, &mut stdout);
    if let Some(error) = &report.error {
        status::clear();
        eprintln!("Error: {error}");
    }
    if args.json {
        // A script that stopped reading is not a capture failure.
        let _ = write_line(&mut stdout, &report);
    }
    report.exit_code
}

fn write_line(out: &mut impl Write, value: &impl Serialize) -> io::Result<()> {
    serde_json::to_writer(&mut *out, value).map_err(io::Error::other)?;
    writeln!(out)?;
    out.flush()
}

fn capture(args: &Args, stdout: &mut impl Write) -> Report {
    let mut report = Report {
        event: "finished",
        trace: None,
        protocol: None,
        version: None,
        packets: Packets::default(),
        duration_ms: 0,
        stopped: "error",
        command_exit_code: None,
        exit_code: exit::CAPTURE_FAILED,
        error: None,
    };
    match proxy(args, stdout, &mut report) {
        Ok(code) => report.exit_code = code,
        Err(err) => report.error = Some(format!("{err:#}")),
    }
    report
}

/// Everything from binding the listener to the command's exit. An `Err` is a
/// capture failure; the timeouts and the recorded connection's own failures
/// set `report.error` and return their exit code.
fn proxy(args: &Args, stdout: &mut impl Write, report: &mut Report) -> Result<i32> {
    if args.out.exists() && !args.force {
        bail!(
            "{} already exists; pass --force to overwrite it",
            args.out.display()
        );
    }
    if let Some(path) = &args.ready_file {
        match fs::remove_file(path) {
            Err(err) if err.kind() != io::ErrorKind::NotFound => {
                return Err(err).with_context(|| format!("removing {}", path.display()));
            }
            _ => {}
        }
    }
    let listener =
        TcpListener::bind(args.listen).with_context(|| format!("listening on {}", args.listen))?;
    let bound = listener.local_addr()?;
    let started = Instant::now();
    note!(
        "connect the client to {bound}; forwarding to {}",
        args.server
    );
    announce(bound, args, stdout)?;

    // Dropping this on any way out kills a command that is still running.
    let mut command = match args.command.split_first() {
        Some((program, rest)) => Some(Running::spawn(program, rest, bound)?),
        None => None,
    };
    let limits = Limits {
        connect_timeout: args.connect_timeout,
        max_duration: args.max_duration,
    };
    let opened = Arc::new(AtomicBool::new(false));
    let open = opener(args.out.clone(), args.force);
    let outcome = serve(
        listener,
        &args.server,
        {
            let opened = opened.clone();
            move || open().inspect(|_| opened.store(true, Ordering::Relaxed))
        },
        &limits,
        &mut || command.as_mut().is_some_and(Running::exited),
    );
    // Also when writing it failed part way: the file holds what was flushed.
    if opened.load(Ordering::Relaxed) {
        report.trace = Some(args.out.clone());
    }
    let outcome = outcome?;

    let recorded = outcome.out.is_some();
    if let Some(stats) = outcome.stats {
        report.protocol = Some(stats.protocol);
        report.version = ProtocolVersion::from_protocol(stats.protocol).map(|v| v.name);
        report.packets = stats.packets;
        report.duration_ms = stats.duration.as_millis() as u64;
    }
    if recorded {
        note!("trace written to {}", args.out.display());
    }
    report.stopped = outcome.stopped.label();
    let kept = if recorded {
        "the trace holds what was recorded until then"
    } else {
        "nothing was recorded"
    };
    let timed_out = |problem: &str, flag: &str, limit: Option<Duration>| {
        let limit = limit.unwrap_or_default();
        Some(format!("{problem} after {limit:?} ({flag}); {kept}"))
    };
    let outlived = || {
        timed_out(
            "capture was still running",
            "--max-duration",
            args.max_duration,
        )
    };
    let (error, code) = match outcome.stopped {
        Stopped::Disconnected | Stopped::CommandExited => (None, exit::OK),
        Stopped::ConnectTimeout => (
            timed_out(
                "no client had logged in",
                "--connect-timeout",
                args.connect_timeout,
            ),
            exit::TIMEOUT,
        ),
        Stopped::MaxDuration => (outlived(), exit::TIMEOUT),
        Stopped::Failed(failure) => (Some(failure.to_string()), exit::CAPTURE_FAILED),
    };
    if error.is_some() {
        report.error = error;
        return Ok(code);
    }

    let Some(command) = &mut command else {
        return Ok(exit::OK);
    };
    let deadline = args.max_duration.map(|max| started + max);
    let Some(status) = command.wait_until(deadline)? else {
        report.stopped = Stopped::MaxDuration.label();
        report.error = outlived();
        return Ok(exit::TIMEOUT);
    };
    let code = exit_code(status);
    report.command_exit_code = Some(code);
    if code != 0 {
        note!("the command exited with status {code}");
        return Ok(code);
    }
    if !recorded {
        report.error = Some(
            "the command exited without logging in through the proxy, so nothing was recorded"
                .into(),
        );
        return Ok(exit::CAPTURE_FAILED);
    }
    Ok(exit::OK)
}

#[derive(Serialize)]
struct Listening {
    event: &'static str,
    addr: SocketAddr,
    host: IpAddr,
    port: u16,
}

/// Tells a waiting script the proxy is accepting connections, and where: one
/// line on stdout, and `--ready-file`.
fn announce(bound: SocketAddr, args: &Args, stdout: &mut impl Write) -> Result<()> {
    // Nobody reading stdout is fine; the ready file or the command's
    // environment may be what the script uses.
    let _ = if args.json {
        write_line(
            stdout,
            &Listening {
                event: "listening",
                addr: bound,
                host: bound.ip(),
                port: bound.port(),
            },
        )
    } else {
        writeln!(stdout, "{bound}").and_then(|()| stdout.flush())
    };
    if let Some(path) = &args.ready_file {
        write_ready_file(path, bound)
            .with_context(|| format!("writing the ready file {}", path.display()))?;
    }
    Ok(())
}

/// Written beside the path and renamed into place, so a script polling for
/// the file never reads half an address.
fn write_ready_file(path: &Path, bound: SocketAddr) -> io::Result<()> {
    let mut partial = path.as_os_str().to_owned();
    partial.push(".partial");
    fs::write(&partial, format!("{bound}\n"))?;
    fs::rename(&partial, path)
}

/// Where a program on this machine connects to reach a listener bound to
/// `bound`: the wildcard address listens everywhere but is not a destination.
fn reachable(bound: SocketAddr) -> SocketAddr {
    let ip = match bound.ip() {
        IpAddr::V4(ip) if ip.is_unspecified() => Ipv4Addr::LOCALHOST.into(),
        IpAddr::V6(ip) if ip.is_unspecified() => Ipv6Addr::LOCALHOST.into(),
        ip => ip,
    };
    SocketAddr::new(ip, bound.port())
}

/// The proxy's address as the command is given it: each part is `PEEL_<PART>`
/// in its environment and `{part}` in its arguments.
fn address_parts(addr: SocketAddr) -> [(&'static str, String); 3] {
    [
        ("addr", addr.to_string()),
        ("host", addr.ip().to_string()),
        ("port", addr.port().to_string()),
    ]
}

/// `arg` with each `{part}` replaced by its value. An argument that isn't
/// Unicode is passed on as it is.
fn fill(arg: &OsString, parts: &[(&str, String)]) -> OsString {
    let Some(text) = arg.to_str() else {
        return arg.clone();
    };
    parts
        .iter()
        .fold(text.to_string(), |text, (part, value)| {
            text.replace(&format!("{{{part}}}"), value)
        })
        .into()
}

/// The command `peel capture -- <command>` runs, killed when dropped unless
/// it has exited.
struct Running {
    child: Child,
    status: Option<ExitStatus>,
}

impl Running {
    fn spawn(program: &OsString, args: &[OsString], bound: SocketAddr) -> Result<Self> {
        let parts = address_parts(reachable(bound));
        let name = Path::new(program).display();
        let child = Command::new(program)
            .args(args.iter().map(|arg| fill(arg, &parts)))
            .envs(
                parts
                    .iter()
                    .map(|(part, value)| (format!("PEEL_{}", part.to_uppercase()), value)),
            )
            // stdout is Peel's own: the ready line and the `--json` summary.
            .stdout(Stdio::from(io::stderr()))
            .spawn()
            .with_context(|| format!("starting {name}"))?;
        note!("started {name} (pid {})", child.id());
        Ok(Self {
            child,
            status: None,
        })
    }

    fn exited(&mut self) -> bool {
        if self.status.is_none() {
            // An error here means the child can't be waited on; it is treated
            // as still running and left to the timeouts.
            self.status = self.child.try_wait().ok().flatten();
        }
        self.status.is_some()
    }

    /// The command's status, or `None` if `deadline` passes first.
    fn wait_until(&mut self, deadline: Option<Instant>) -> Result<Option<ExitStatus>> {
        let Some(deadline) = deadline else {
            self.status = Some(self.child.wait().context("waiting for the command")?);
            return Ok(self.status);
        };
        while !self.exited() && Instant::now() < deadline {
            thread::sleep(POLL);
        }
        Ok(self.status)
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        if !self.exited() {
            note!("stopping the command (pid {})", self.child.id());
            // TODO: this ends the command itself, not processes it started. A
            // wrapper script should `exec` the client until Peel kills the
            // whole tree (a job object on Windows, a process group on Unix).
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

/// The command's exit code as a shell would report it.
fn exit_code(status: ExitStatus) -> i32 {
    #[cfg(unix)]
    if let Some(signal) = std::os::unix::process::ExitStatusExt::signal(&status) {
        return 128 + signal;
    }
    status.code().unwrap_or(1)
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
    /// Taken by the first login, which records the trace, or by capture
    /// stopping before one.
    open: Mutex<Option<Opener<W>>>,
    finished: mpsc::Sender<Finished<W>>,
    live: Mutex<Live<W>>,
}

/// Every open connection, so that stopping capture can end them.
struct Live<W> {
    closing: bool,
    sockets: Vec<(SocketAddr, TcpStream)>,
    connections: Vec<(SocketAddr, Arc<Mutex<Connection<W>>>)>,
}

impl<W> Capture<W> {
    /// Keeps a handle to one of `peer`'s sockets. `false` once capture is
    /// stopping, when the connection should end instead.
    fn track(&self, peer: SocketAddr, socket: &TcpStream) -> io::Result<bool> {
        let mut live = self.live.lock().unwrap();
        if !live.closing {
            live.sockets.push((peer, socket.try_clone()?));
        }
        Ok(!live.closing)
    }

    fn untrack(&self, peer: SocketAddr) {
        let mut live = self.live.lock().unwrap();
        live.sockets.retain(|(tracked, _)| *tracked != peer);
        live.connections.retain(|(tracked, _)| *tracked != peer);
    }

    fn logged_in(&self) -> bool {
        self.open.lock().unwrap().is_none()
    }

    fn connected(&self) -> bool {
        !self.live.lock().unwrap().sockets.is_empty()
    }

    /// Ends `conn`'s recording, if it has one, and tells `serve`.
    fn finish(&self, conn: &mut Connection<W>) {
        if let Some(finished) = conn.finish() {
            // `serve` has gone only if it gave up waiting for this.
            let _ = self.finished.send(finished);
        }
    }

    /// Stops capture: no later login is recorded, and every connection is
    /// closed. Returns the recorded connection's end, if there was a login.
    fn close(&self, done: &mpsc::Receiver<Finished<W>>) -> Result<Option<Finished<W>>> {
        let recording = self.open.lock().unwrap().take().is_none();
        let mut live = self.live.lock().unwrap();
        live.closing = true;
        for (_, socket) in live.sockets.drain(..) {
            let _ = socket.shutdown(Shutdown::Both);
        }
        if !recording {
            return Ok(None);
        }
        // Shutting a socket down does not wake a read already blocked on it
        // on every platform (not on Windows), so the recording is ended
        // here instead of waiting for the connection's threads to notice.
        let finished = live
            .connections
            .drain(..)
            .find_map(|(_, conn)| conn.lock().unwrap().finish());
        drop(live);
        if finished.is_some() {
            return Ok(finished);
        }
        // It closed by itself just before, and its end is on its way.
        done.recv_timeout(CLOSE_WAIT)
            .map(Some)
            .map_err(|_| anyhow!("the recorded connection did not close when capture stopped"))
    }
}

/// The recorded connection's end: its recorder thread, what it carried, and
/// why the connection ended capture early, if it did.
struct Finished<W> {
    recorder: JoinHandle<io::Result<W>>,
    stats: Stats,
    failure: Option<Failure>,
}

/// What the recorded connection carried.
#[derive(Debug, Clone, Copy)]
pub struct Stats {
    pub protocol: i32,
    pub packets: Packets,
    /// From accepting the connection to its close.
    pub duration: Duration,
}

/// The waits `serve` gives up after, both counted from when it starts.
#[derive(Default)]
pub struct Limits {
    pub connect_timeout: Option<Duration>,
    pub max_duration: Option<Duration>,
}

/// Why capture stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stopped {
    Disconnected,
    CommandExited,
    ConnectTimeout,
    MaxDuration,
    Failed(Failure),
}

impl Stopped {
    /// The `stopped` value in the `--json` summary.
    fn label(self) -> &'static str {
        match self {
            Self::Disconnected => "disconnected",
            Self::CommandExited => "command_exited",
            Self::ConnectTimeout => "connect_timeout",
            Self::MaxDuration => "max_duration",
            Self::Failed(Failure::EncryptionRequested) => "encryption_requested",
            Self::Failed(Failure::Unsupported(_)) => "unsupported_protocol",
        }
    }
}

pub struct Outcome<W> {
    /// The trace's finished writer, `None` when no login was recorded.
    pub out: Option<W>,
    pub stats: Option<Stats>,
    pub stopped: Stopped,
}

/// Accepts connections until the first login connection, the one recorded,
/// closes, a limit passes, or `command_exited` says the command capture runs
/// is gone. The trace starts when the recorded connection was accepted, and
/// `open` is only called then.
pub fn serve<W: Write + Send + 'static>(
    listener: TcpListener,
    server: &str,
    open: impl FnOnce() -> io::Result<W> + Send + 'static,
    limits: &Limits,
    command_exited: &mut dyn FnMut() -> bool,
) -> Result<Outcome<W>> {
    let started = Instant::now();
    let (finished, done) = mpsc::channel();
    let capture = Arc::new(Capture {
        open: Mutex::new(Some(Box::new(open))),
        finished,
        live: Mutex::new(Live {
            closing: false,
            sockets: Vec::new(),
            connections: Vec::new(),
        }),
    });

    listener.set_nonblocking(true)?;
    let mut exited_at: Option<Instant> = None;
    let (mut stopped, finished) = loop {
        match listener.accept() {
            Ok((client, peer)) => {
                // Here rather than in its thread, so a command that exits
                // right after connecting is seen to have connected.
                capture
                    .track(peer, &client)
                    .context("accepting a connection")?;
                let capture = capture.clone();
                let server = server.to_owned();
                thread::spawn(move || {
                    if let Err(err) = connection(client, peer, &server, &capture) {
                        note!("{peer}: {err:#}");
                    }
                });
                continue;
            }
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => {}
            Err(err) => return Err(err).context("accepting a connection"),
        }
        match done.recv_timeout(POLL) {
            Ok(finished) => {
                let stopped = if exited_at.is_some() || command_exited() {
                    Stopped::CommandExited
                } else {
                    Stopped::Disconnected
                };
                break (stopped, Some(finished));
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => unreachable!("capture holds a sender"),
        }

        if exited_at.is_none() && command_exited() {
            exited_at = Some(Instant::now());
            // Accept what it connected with before judging whether it did.
            continue;
        }
        let past = |limit: Option<Duration>| limit.is_some_and(|limit| started.elapsed() >= limit);
        let stopped = if past(limits.max_duration) {
            Stopped::MaxDuration
        } else if past(limits.connect_timeout) && !capture.logged_in() {
            Stopped::ConnectTimeout
        } else if exited_at.is_some_and(|at| !capture.connected() || at.elapsed() >= EXIT_GRACE) {
            // Its sockets closed with it, so a connection still open after
            // the grace belongs to something the command left running.
            Stopped::CommandExited
        } else {
            continue;
        };
        break (stopped, capture.close(&done)?);
    };

    let Some(finished) = finished else {
        return Ok(Outcome {
            out: None,
            stats: None,
            stopped,
        });
    };
    if let Some(failure) = finished.failure {
        stopped = Stopped::Failed(failure);
    }
    let out = finished
        .recorder
        .join()
        .expect("recorder thread panicked")
        .context("writing the trace")?;
    Ok(Outcome {
        out: Some(out),
        stats: Some(finished.stats),
        stopped,
    })
}

struct Connection<W> {
    peer: SocketAddr,
    accepted: Instant,
    accepted_unix_ms: u64,
    session: Session,
    recording: Option<Recording<W>>,
    failure: Option<Failure>,
}

struct Recording<W> {
    records: mpsc::SyncSender<Vec<Record>>,
    recorder: JoinHandle<io::Result<W>>,
    protocol: i32,
    packets: Packets,
}

fn connection<W: Write + Send + 'static>(
    client: TcpStream,
    peer: SocketAddr,
    server: &str,
    capture: &Capture<W>,
) -> Result<()> {
    let result = forward(&client, peer, server, capture);
    capture.untrack(peer);
    result
}

fn forward<W: Write + Send + 'static>(
    client: &TcpStream,
    peer: SocketAddr,
    server: &str,
    capture: &Capture<W>,
) -> Result<()> {
    let accepted = Instant::now();
    let accepted_unix_ms = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;
    // Accepted sockets can inherit the listener's non-blocking mode.
    client.set_nonblocking(false)?;
    let server = TcpStream::connect(server).with_context(|| format!("connecting to {server}"))?;
    if !capture.track(peer, &server)? {
        return Ok(());
    }
    client.set_nodelay(true)?;
    server.set_nodelay(true)?;

    let conn = Arc::new(Mutex::new(Connection {
        peer,
        accepted,
        accepted_unix_ms,
        session: Session::new(),
        recording: None,
        failure: None,
    }));
    capture
        .live
        .lock()
        .unwrap()
        .connections
        .push((peer, conn.clone()));
    thread::scope(|scope| {
        scope.spawn(|| pump(Direction::Serverbound, client, &server, &conn, capture));
        pump(Direction::Clientbound, &server, client, &conn, capture);
    });

    let mut conn = conn.lock().unwrap();
    capture.finish(&mut conn);
    // Still there only if this was not the recorded connection.
    if let Some(failure) = conn.failure {
        note!("{peer}: {failure}");
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

impl<W> Connection<W> {
    /// Ends the recording, if this connection has one that hasn't been
    /// ended: the recorder writes what it was sent and stops. Packets
    /// forwarded after this are not recorded.
    fn finish(&mut self) -> Option<Finished<W>> {
        let recording = self.recording.take()?;
        Some(Finished {
            recorder: recording.recorder,
            stats: Stats {
                protocol: recording.protocol,
                packets: recording.packets,
                duration: self.accepted.elapsed(),
            },
            failure: self.failure.take(),
        })
    }
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
                Notice::EncryptionRequested => {
                    forward = false;
                    self.failure = Some(Failure::EncryptionRequested);
                }
                Notice::Unsupported(protocol) => {
                    forward = false;
                    self.failure = Some(Failure::Unsupported(protocol));
                }
            }
        }
        if let Some(recording) = &mut self.recording
            && !fed.records.is_empty()
        {
            for record in &fed.records {
                *match record.direction {
                    Direction::Serverbound => &mut recording.packets.serverbound,
                    Direction::Clientbound => &mut recording.packets.clientbound,
                } += 1;
            }
            // A failed send means the recorder already stopped and said why.
            let _ = recording.records.send(fed.records);
        }
        if !forward {
            // Not left to the connection closing: a client that never hangs
            // up would keep a failed capture running.
            capture.finish(self);
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
        let (records, received) = mpsc::sync_channel(PENDING_BATCHES);
        let recorder = thread::spawn(move || record(open, &header, marker, &received));
        self.recording = Some(Recording {
            records,
            recorder,
            protocol,
            packets: Packets::default(),
        });
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

    use peel_decode::test_util::{latest, named, packet};
    use peel_trace::{Phase, TraceReader};
    use pomme_protocol::PacketTable;
    use pomme_protocol::version::LATEST;

    use super::*;
    use crate::wire::test_frames::{LOGIN, STATUS, frame, handshake, intention};

    struct Proxy {
        proxy: SocketAddr,
        server: TcpListener,
        handle: JoinHandle<Result<Outcome<Vec<u8>>>>,
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
            serve(
                proxy,
                &server_addr,
                move || {
                    flag.store(true, Ordering::SeqCst);
                    Ok(Vec::new())
                },
                &Limits::default(),
                &mut || false,
            )
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

        let outcome = handle.join().unwrap().unwrap();
        assert_eq!(outcome.stopped, Stopped::Disconnected);
        let stats = outcome.stats.unwrap();
        assert_eq!(stats.protocol, LATEST.protocol);
        let sent = |direction| s.expected.iter().filter(|e| e.0 == direction).count() as u64;
        assert_eq!(
            stats.packets,
            Packets {
                serverbound: sent(Sb),
                clientbound: sent(Cb)
            }
        );
        let records = read_trace(&outcome.out.unwrap());
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

        // Neither side has hung up, and capture ends all the same.
        let outcome = handle.join().unwrap().unwrap();
        let failure = Failure::EncryptionRequested;
        assert_eq!(outcome.stopped, Stopped::Failed(failure));
        assert!(failure.to_string().contains("online-mode=false"));
        // The request itself is recorded, so the trace shows why it ended.
        let records = read_trace(&outcome.out.unwrap());
        let last = records.last().unwrap();
        assert_eq!(records.len(), 3);
        assert_eq!(
            (last.direction, last.phase, last.frame.as_slice()),
            (Direction::Clientbound, Phase::Login, &encryption[1..])
        );
    }

    #[test]
    fn an_unsupported_protocol_stops_capture() {
        let Proxy {
            proxy,
            server,
            handle,
            ..
        } = start();
        let unknown = 12_345;
        assert!(PacketTable::for_protocol(unknown).is_none());

        let mut client = TcpStream::connect(proxy).unwrap();
        let (mut upstream, _) = server.accept().unwrap();
        let mut request = frame(&packet(0, &intention(unknown, LOGIN)), None);
        request.extend(frame(&packet(0, b"\x06player"), None));
        client.write_all(&request).unwrap();
        expect_bytes(&mut upstream, &request);

        upstream
            .write_all(&frame(&packet(2, &[0; 4]), None))
            .unwrap();
        assert_closed(&mut client);
        assert_closed(&mut upstream);

        let outcome = handle.join().unwrap().unwrap();
        let failure = Failure::Unsupported(unknown);
        assert_eq!(outcome.stopped, Stopped::Failed(failure));
        assert!(failure.to_string().contains("supported version"));
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

        let outcome = handle.join().unwrap().unwrap();
        assert_eq!(outcome.stopped, Stopped::Disconnected);
        assert_eq!(read_trace(&outcome.out.unwrap()).len(), 1);
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

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("peel-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A server that accepts logins and closes each when the client does.
    fn sink_server() -> String {
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = server.local_addr().unwrap().to_string();
        thread::spawn(move || {
            for mut conn in server.incoming().flatten() {
                thread::spawn(move || {
                    let _ = conn.read_to_end(&mut Vec::new());
                });
            }
        });
        addr
    }

    /// The address in the ready file, once capture has written it.
    fn ready_addr(ready: &Path) -> SocketAddr {
        let waited = Instant::now();
        loop {
            if let Some(addr) = fs::read_to_string(ready)
                .ok()
                .and_then(|text| text.trim_end().parse().ok())
            {
                return addr;
            }
            assert!(waited.elapsed() < Duration::from_secs(10), "never ready");
            thread::sleep(Duration::from_millis(5));
        }
    }

    /// A client that has sent its login to the proxy at `addr`.
    fn log_in(addr: impl std::net::ToSocketAddrs) -> TcpStream {
        let mut client = TcpStream::connect(addr).unwrap();
        client
            .write_all(&login_request(latest(), b"\x06player"))
            .unwrap();
        client
    }

    fn args(dir: &Path, server: String) -> Args {
        Args {
            listen: "127.0.0.1:0".parse().unwrap(),
            server,
            out: dir.join("session.trace"),
            force: false,
            ready_file: None,
            connect_timeout: None,
            max_duration: None,
            quiet: false,
            json: false,
            command: Vec::new(),
        }
    }

    /// Marks the argument telling `command_helper` what to do.
    const BEHAVIOUR: &str = "behaviour=";

    /// Captures with this test binary as the command, re-run to do
    /// `behaviour` in `command_helper` and nothing else. The arguments after
    /// the test's name are read by the helper; as test filters they match
    /// nothing.
    fn capture_with_helper(args: &mut Args, behaviour: &str) -> Report {
        args.command = vec![
            std::env::current_exe().unwrap().into(),
            "--exact".into(),
            "capture::tests::command_helper".into(),
            "--nocapture".into(),
            format!("{BEHAVIOUR}{behaviour}").into(),
            "{addr}".into(),
        ];
        capture(args, &mut Vec::new())
    }

    /// Not a test of its own: the command the tests below have capture run.
    /// Only that child is given a behaviour, so an ordinary run does nothing.
    #[test]
    fn command_helper() {
        let Some(behaviour) =
            std::env::args().find_map(|arg| arg.strip_prefix(BEHAVIOUR).map(str::to_owned))
        else {
            return;
        };
        let (what, code) = behaviour.split_once(':').unwrap();
        match what {
            "login" => {
                let addr = std::env::var("PEEL_ADDR").unwrap();
                let host = std::env::var("PEEL_HOST").unwrap();
                let port = std::env::var("PEEL_PORT").unwrap();
                assert_eq!(addr, format!("{host}:{port}"));
                assert_eq!(std::env::args().next_back(), Some(addr.clone()));
                // Closed at once: the login is recorded however soon the
                // command is gone.
                drop(log_in(addr));
            }
            "exit" => {}
            "hang" => thread::sleep(Duration::from_secs(120)),
            other => panic!("unknown behaviour {other}"),
        }
        std::process::exit(code.parse().unwrap());
    }

    #[test]
    fn port_zero_reports_the_bound_address() {
        let dir = scratch("ready");
        let ready = dir.join("ready");
        fs::write(&ready, "stale").unwrap();
        let mut args = args(&dir, sink_server());
        args.ready_file = Some(ready.clone());
        let out = args.out.clone();

        let handle = thread::spawn(move || {
            let mut stdout = Vec::new();
            let report = capture(&args, &mut stdout);
            (report, String::from_utf8(stdout).unwrap())
        });

        let bound = ready_addr(&ready);
        assert_ne!(bound.port(), 0);

        let mut client = log_in(bound);
        client.shutdown(Shutdown::Write).unwrap();
        assert_closed(&mut client);

        let (report, stdout) = handle.join().unwrap();
        assert_eq!(stdout, format!("{bound}\n"), "stdout is the one ready line");
        assert_eq!(report.exit_code, exit::OK, "{report:?}");
        assert_eq!(report.stopped, "disconnected");
        assert_eq!(report.protocol, Some(LATEST.protocol));
        assert_eq!(report.version, Some(LATEST.name));
        assert_eq!(
            report.packets,
            Packets {
                serverbound: 2,
                clientbound: 0
            }
        );
        assert_eq!(report.trace.as_deref(), Some(out.as_path()));
        assert_eq!(read_trace(&fs::read(&out).unwrap()).len(), 2);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn json_ready_line_and_summary() {
        let dir = scratch("json");
        let mut args = args(&dir, sink_server());
        args.json = true;
        args.connect_timeout = Some(Duration::from_millis(100));
        let mut stdout = Vec::new();
        let report = capture(&args, &mut stdout);

        let listening: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
        assert_eq!(listening["event"], "listening");
        let addr: SocketAddr = listening["addr"].as_str().unwrap().parse().unwrap();
        assert_eq!(listening["host"], "127.0.0.1");
        assert_eq!(listening["port"], addr.port());

        let summary = serde_json::to_value(&report).unwrap();
        assert_eq!(summary["event"], "finished");
        assert_eq!(summary["stopped"], "connect_timeout");
        assert_eq!(summary["exit_code"], exit::TIMEOUT);
        assert_eq!(summary["trace"], serde_json::Value::Null);
        assert_eq!(summary["packets"]["serverbound"], 0);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn connect_timeout_ends_a_capture_nobody_joins() {
        let dir = scratch("connect-timeout");
        let mut args = args(&dir, sink_server());
        args.connect_timeout = Some(Duration::from_millis(150));

        // A hung command is stopped with it rather than waited for.
        let started = Instant::now();
        let report = capture_with_helper(&mut args, "hang:0");
        assert!(started.elapsed() < Duration::from_secs(60));
        assert_eq!(report.exit_code, exit::TIMEOUT, "{report:?}");
        assert_eq!(report.stopped, "connect_timeout");
        assert!(
            report.error.as_ref().unwrap().contains("--connect-timeout"),
            "{report:?}"
        );
        assert_eq!(report.command_exit_code, None);
        assert!(!args.out.exists(), "a trace was created without a login");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn max_duration_ends_a_session_that_outlives_it() {
        let dir = scratch("max-duration");
        let ready = dir.join("ready");
        let mut args = args(&dir, sink_server());
        args.ready_file = Some(ready.clone());
        // A login in time, so only the session's length can stop it. Both
        // are long enough for a busy machine to get the login in first.
        args.connect_timeout = Some(Duration::from_secs(2));
        args.max_duration = Some(Duration::from_millis(2500));
        let out = args.out.clone();

        let handle = thread::spawn(move || capture(&args, &mut Vec::new()));
        let mut client = log_in(ready_addr(&ready));

        let report = handle.join().unwrap();
        assert_eq!(report.exit_code, exit::TIMEOUT, "{report:?}");
        assert_eq!(report.stopped, "max_duration");
        assert!(
            report.error.as_ref().unwrap().contains("--max-duration"),
            "{report:?}"
        );
        // The proxy closed the client's connection, and the trace is whole.
        assert_closed(&mut client);
        assert_eq!(read_trace(&fs::read(&out).unwrap()).len(), 2);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_command_logs_in_and_its_status_passes_through() {
        let dir = scratch("command");
        let mut args = args(&dir, sink_server());
        args.max_duration = Some(Duration::from_secs(60));

        let report = capture_with_helper(&mut args, "login:0");
        assert_eq!(report.exit_code, exit::OK, "{report:?}");
        assert_eq!(report.command_exit_code, Some(0));
        assert_eq!(read_trace(&fs::read(&args.out).unwrap()).len(), 2);

        args.force = true;
        let report = capture_with_helper(&mut args, "login:3");
        assert_eq!(report.exit_code, 3, "{report:?}");
        assert_eq!(report.command_exit_code, Some(3));
        assert_eq!(report.error, None);
        assert_eq!(report.trace.as_deref(), Some(args.out.as_path()));
        assert_eq!(read_trace(&fs::read(&args.out).unwrap()).len(), 2);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_command_that_never_logs_in_fails_the_capture() {
        let dir = scratch("no-login");
        let mut args = args(&dir, sink_server());

        let report = capture_with_helper(&mut args, "exit:0");
        assert_eq!(report.exit_code, exit::CAPTURE_FAILED, "{report:?}");
        assert_eq!(report.stopped, "command_exited");
        assert!(report.error.is_some());

        // Its own failure is the one reported when it has one.
        let report = capture_with_helper(&mut args, "exit:7");
        assert_eq!(report.exit_code, 7, "{report:?}");
        assert!(!args.out.exists());

        args.command = vec!["peel-test-no-such-program".into()];
        let report = capture(&args, &mut Vec::new());
        assert_eq!(report.exit_code, exit::CAPTURE_FAILED, "{report:?}");
        assert_eq!(report.stopped, "error");
        assert!(report.error.unwrap().contains("peel-test-no-such-program"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_encryption_request_kills_the_command() {
        let dir = scratch("encryption");
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut args = args(&dir, server.local_addr().unwrap().to_string());
        let table = latest();
        thread::spawn(move || {
            let (mut conn, _) = server.accept().unwrap();
            let mut request = vec![0; login_request(table, b"\x06player").len()];
            conn.read_exact(&mut request).unwrap();
            let (_, hello) = named(
                table,
                Phase::Login,
                Direction::Clientbound,
                "hello",
                &[0; 32],
            );
            conn.write_all(&frame(&hello, None)).unwrap();
            let _ = conn.read_to_end(&mut Vec::new());
        });

        let report = capture_with_helper(&mut args, "login:0");
        assert_eq!(report.exit_code, exit::CAPTURE_FAILED, "{report:?}");
        assert_eq!(report.stopped, "encryption_requested");
        assert!(report.error.unwrap().contains("online-mode=false"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn seconds_are_positive_numbers() {
        assert_eq!(seconds("30"), Ok(Duration::from_secs(30)));
        assert_eq!(seconds("0.5"), Ok(Duration::from_millis(500)));
        for bad in ["0", "-1", "soon", "inf", "NaN"] {
            assert!(seconds(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_wildcard_address_is_reached_through_loopback() {
        let reach = |addr: &str| reachable(addr.parse().unwrap()).to_string();
        assert_eq!(reach("0.0.0.0:25566"), "127.0.0.1:25566");
        assert_eq!(reach("[::]:25566"), "[::1]:25566");
        assert_eq!(reach("192.168.1.5:25566"), "192.168.1.5:25566");
    }

    #[test]
    fn placeholders_in_arguments_become_the_address() {
        let parts = address_parts("127.0.0.1:25566".parse().unwrap());
        let fill = |arg: &str| fill(&arg.into(), &parts);
        assert_eq!(fill("--server={addr}"), "--server=127.0.0.1:25566");
        assert_eq!(fill("{host} {port} {port}"), "127.0.0.1 25566 25566");
        assert_eq!(fill("{other} addr"), "{other} addr");
    }
}
