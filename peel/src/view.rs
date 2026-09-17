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

    let served = Arc::new(Served { trace, reference });
    for stream in listener.incoming() {
        let served = served.clone();
        thread::spawn(move || stream.and_then(|stream| respond(stream, &served)));
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
fn respond(mut stream: TcpStream, served: &Served) -> io::Result<()> {
    let mut reader = BufReader::new(&stream);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let mut header = String::new();
    while reader.read_line(&mut header)? > 2 {
        header.clear();
    }

    let mut parts = request_line.split_whitespace();
    let method = parts.next();
    let path = parts
        .next()
        .and_then(|target| target.split(['?', '#']).next())
        .unwrap_or("/");
    let (status, content_type, body): (&str, &str, &[u8]) = match (method, path) {
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
    };
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\n\
         Cache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)
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
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || respond(listener.accept().unwrap().0, &served));
        let mut client = TcpStream::connect(addr).unwrap();
        write!(client, "GET {path} HTTP/1.1\r\nHost: {addr}\r\n\r\n").unwrap();
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
