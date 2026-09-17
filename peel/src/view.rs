//! `peel view`: serves the embedded viewer on 127.0.0.1 and opens a trace in
//! it, compared against a reference when one is given. The server only
//! answers local requests for the viewer's own files and those traces; it
//! never connects anywhere.

use std::io::{self, BufRead, BufReader, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};

use crate::cli;
use crate::status::note;

mod assets {
    include!(concat!(env!("OUT_DIR"), "/viewer.rs"));
}

#[derive(clap::Args)]
pub struct Args {
    /// The trace file to open.
    trace: PathBuf,
    /// A vanilla reference trace to compare it against.
    #[arg(long, value_name = "REFERENCE")]
    against: Option<PathBuf>,
    /// Port to serve on; 0 picks a free one.
    #[arg(long, default_value_t = 0)]
    port: u16,
    /// Print the address without opening a browser.
    #[arg(long)]
    no_open: bool,
}

/// How long a request may take before the connection is dropped, so a
/// stalled peer can't hold a thread.
const TIMEOUT: Duration = Duration::from_secs(5);
/// The longest request line or header, and how many headers are read.
const MAX_HEADER_BYTES: u64 = 8 << 10;
const MAX_HEADERS: usize = 64;

/// The trace files being served.
struct Served {
    trace: Vec<u8>,
    reference: Option<Vec<u8>>,
}

pub fn run(args: Args) -> Result<()> {
    let (trace, _) = cli::read_trace(&args.trace)?;
    let mut query = format!("trace={}", file_name(&args.trace)?);
    let reference = match &args.against {
        Some(path) => {
            query += &format!("&reference={}", file_name(path)?);
            Some(cli::read_trace(path)?.0)
        }
        None => None,
    };

    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, args.port))
        .with_context(|| format!("listening on port {}", args.port))?;
    let url = format!("http://{}/?{query}", listener.local_addr()?);
    note!("viewing {} at {url} until Ctrl+C", args.trace.display());
    if !args.no_open
        && let Err(err) = open_browser(&url)
    {
        note!("couldn't open a browser ({err}); open the address above yourself");
    }

    let port = listener.local_addr()?.port();
    let served = Arc::new(Served { trace, reference });
    for stream in listener.incoming() {
        let served = served.clone();
        thread::spawn(move || stream.and_then(|stream| respond(stream, &served, port)));
    }
    Ok(())
}

/// A path's file name, percent-encoded for the viewer's query string.
fn file_name(path: &Path) -> Result<String> {
    let name = path
        .file_name()
        .with_context(|| format!("{} has no file name", path.display()))?;
    Ok(query_encode(&name.to_string_lossy()))
}

/// Answers one HTTP/1.1 request: `GET /trace` and `GET /reference` return
/// the traces, other `GET`s an embedded viewer file.
fn respond(mut stream: TcpStream, served: &Served, port: u16) -> io::Result<()> {
    stream.set_read_timeout(Some(TIMEOUT))?;
    stream.set_write_timeout(Some(TIMEOUT))?;
    let mut reader = BufReader::new(&stream);
    let mut request_line = String::new();
    read_line(&mut reader, &mut request_line)?;
    let mut host = String::new();
    let mut header = String::new();
    for _ in 0..MAX_HEADERS {
        if read_line(&mut reader, &mut header)? <= 2 {
            break;
        }
        if let Some(value) = header_value(&header, "host") {
            host = value;
        }
    }

    let mut parts = request_line.split_whitespace();
    let method = parts.next();
    let path = parts
        .next()
        .and_then(|target| target.split(['?', '#']).next())
        .unwrap_or("/");
    let (status, content_type, body): (&str, &str, &[u8]) = if !ours(&host, port) {
        (
            "403 Forbidden",
            "text/plain; charset=utf-8",
            b"this address is not the one Peel serves",
        )
    } else {
        match (method, path) {
            (Some("GET"), "/trace") => ("200 OK", "application/octet-stream", &served.trace),
            (Some("GET"), "/reference") if served.reference.is_some() => (
                "200 OK",
                "application/octet-stream",
                served.reference.as_deref().unwrap_or_default(),
            ),
            (Some("GET"), path) => match asset(path) {
                Some((content_type, body)) => ("200 OK", content_type, body),
                None => ("404 Not Found", "text/plain; charset=utf-8", b"not found"),
            },
            _ => (
                "405 Method Not Allowed",
                "text/plain; charset=utf-8",
                b"GET only",
            ),
        }
    };
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\n\
         Cache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\n\
         X-Frame-Options: DENY\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)
}

/// One line of the request, refusing one longer than a browser ever sends
/// so a local process can't grow the buffer without end.
fn read_line(reader: &mut impl BufRead, line: &mut String) -> io::Result<usize> {
    line.clear();
    let read = io::Read::take(&mut *reader, MAX_HEADER_BYTES).read_line(line)?;
    if !line.ends_with('\n') && read as u64 == MAX_HEADER_BYTES {
        return Err(io::Error::other("request header is too long"));
    }
    Ok(read)
}

fn header_value(header: &str, name: &str) -> Option<String> {
    let (field, value) = header.split_once(':')?;
    field
        .eq_ignore_ascii_case(name)
        .then(|| value.trim().to_string())
}

/// Whether `Host` is the address Peel serves. Another name pointing at
/// 127.0.0.1 means a web page is reaching the local server through its own
/// domain, which is how a site would read someone's trace.
fn ours(host: &str, port: u16) -> bool {
    let host = host.to_ascii_lowercase();
    host == format!("127.0.0.1:{port}") || host == format!("localhost:{port}")
}

fn asset(path: &str) -> Option<(&'static str, &'static [u8])> {
    let file = match path.trim_start_matches('/') {
        "" => "index.html",
        file => file,
    };
    assets::ASSETS
        .iter()
        .find(|(name, _)| *name == file)
        .map(|(name, body)| (content_type(name), *body))
}

fn content_type(file: &str) -> &'static str {
    match file.rsplit('.').next() {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("wasm") => "application/wasm",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        Some("json") => "application/json",
        _ => "application/octet-stream",
    }
}

/// Percent-encodes everything but unreserved characters, for a query value.
fn query_encode(value: &str) -> String {
    value
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                char::from(b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

fn open_browser(url: &str) -> io::Result<()> {
    let mut command = if cfg!(windows) {
        let mut command = Command::new("rundll32");
        command.arg("url.dll,FileProtocolHandler");
        command
    } else if cfg!(target_os = "macos") {
        Command::new("open")
    } else {
        Command::new("xdg-open")
    };
    command.arg(url).spawn().map(drop)
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use super::*;

    fn get(path: &str, served: Served) -> (String, Vec<u8>) {
        request(path, served, |addr| format!("Host: {addr}\r\n"))
    }

    /// One request with the headers `headers` builds from our own address.
    fn request(
        path: &str,
        served: Served,
        headers: impl FnOnce(&str) -> String,
    ) -> (String, Vec<u8>) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        let port = addr.port();
        let server = thread::spawn(move || respond(listener.accept().unwrap().0, &served, port));
        let mut client = TcpStream::connect(addr).unwrap();
        let headers = headers(&addr.to_string());
        write!(client, "GET {path} HTTP/1.1\r\n{headers}\r\n").unwrap();
        let mut response = Vec::new();
        client.read_to_end(&mut response).unwrap();
        server.join().unwrap().unwrap();
        let split = response.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
        let head = String::from_utf8(response[..split].to_vec()).unwrap();
        (head, response[split + 4..].to_vec())
    }

    fn served(reference: Option<&[u8]>) -> Served {
        Served {
            trace: b"PEEL-trace".to_vec(),
            reference: reference.map(<[u8]>::to_vec),
        }
    }

    #[test]
    fn serves_the_traces_and_the_viewer() {
        let (head, body) = get("/trace", served(None));
        assert!(head.starts_with("HTTP/1.1 200 OK"), "{head}");
        assert!(
            head.contains("Content-Type: application/octet-stream"),
            "{head}"
        );
        assert_eq!(body, b"PEEL-trace");

        let (head, _) = get("/reference", served(None));
        assert!(head.starts_with("HTTP/1.1 404"), "{head}");
        let (_, body) = get("/reference", served(Some(b"PEEL-reference")));
        assert_eq!(body, b"PEEL-reference");

        let (head, body) = get("/?trace=walk.trace", served(None));
        assert!(head.contains("Content-Type: text/html"), "{head}");
        assert_eq!(body, asset("/").unwrap().1);

        for path in ["/../Cargo.toml", "/_headers"] {
            let (head, _) = get(path, served(None));
            assert!(head.starts_with("HTTP/1.1 404"), "{path}: {head}");
        }
    }

    /// A page on another domain can point it at 127.0.0.1 and reach this
    /// server, so anything but our own address is refused.
    #[test]
    fn only_our_own_host_is_served() {
        let (head, _) = request("/trace", served(None), |_| {
            "Host: evil.example
"
            .to_string()
        });
        assert!(head.starts_with("HTTP/1.1 403"), "{head}");

        let (head, body) = request("/trace", served(None), |addr| {
            format!(
                "Host: localhost:{}
",
                addr.rsplit(':').next().unwrap()
            )
        });
        assert!(head.starts_with("HTTP/1.1 200"), "{head}");
        assert_eq!(body, b"PEEL-trace");

        let (head, _) = get("/trace", served(None));
        assert!(head.contains("X-Content-Type-Options: nosniff"), "{head}");
        assert!(head.contains("X-Frame-Options: DENY"), "{head}");
    }

    #[test]
    fn a_header_longer_than_any_browser_sends_is_refused() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        let port = addr.port();
        let served = served(None);
        let server = thread::spawn(move || respond(listener.accept().unwrap().0, &served, port));
        let mut client = TcpStream::connect(addr).unwrap();
        write!(
            client,
            "GET /trace HTTP/1.1
Host: {addr}
X: "
        )
        .unwrap();
        let long = "x".repeat(MAX_HEADER_BYTES as usize + 1);
        let _ = client.write_all(long.as_bytes());
        assert!(server.join().unwrap().is_err());
    }

    #[test]
    fn content_types_and_query_encoding() {
        assert_eq!(
            content_type("assets/peel_wasm_bg-1.wasm"),
            "application/wasm"
        );
        assert_eq!(content_type("index.html"), "text/html; charset=utf-8");
        assert_eq!(
            query_encode("my walk (2).trace"),
            "my%20walk%20%282%29.trace"
        );
    }
}
