mod capture;
mod cli;
mod coverage;
mod diff;
mod dump;
mod session;
mod status;
mod view;
mod wire;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "peel", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Proxy a local offline-mode server and record the session to a trace.
    /// Exits 124 on a timeout and 125 when capture fails; with a command, its
    /// status otherwise.
    Capture(capture::Args),
    /// Print a trace grouped by tick.
    Dump(dump::Args),
    /// Report what a trace decodes, and where decoding stops.
    Coverage(coverage::Args),
    /// Compare a trace against a vanilla reference. Exits 1 when they differ, 2
    /// on errors.
    Diff(diff::Args),
    /// Open a trace in the viewer.
    View(view::Args),
}

fn main() {
    let code = match Cli::parse().command {
        Command::Capture(args) => Ok(capture::run(args)),
        Command::Dump(args) => dump::run(args).map(|()| cli::exit::OK),
        Command::Coverage(args) => coverage::run(args).map(|()| cli::exit::OK),
        Command::Diff(args) => diff::run(args),
        Command::View(args) => view::run(args).map(|()| cli::exit::OK),
    };
    std::process::exit(code.unwrap_or_else(|err| {
        eprintln!("Error: {err:?}");
        cli::exit::ERROR
    }));
}
