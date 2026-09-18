mod capture;
mod cli;
mod coverage;
mod diff;
mod dump;
mod session;
mod status;
mod view;
mod wire;

use std::process::ExitCode;

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

fn main() -> ExitCode {
    let differs = match Cli::parse().command {
        Command::Capture(args) => capture::run(args).map(|()| false),
        Command::Dump(args) => dump::run(args).map(|()| false),
        Command::Coverage(args) => coverage::run(args).map(|()| false),
        Command::Diff(args) => diff::run(args),
        Command::View(args) => view::run(args).map(|()| false),
    };
    match differs {
        Ok(false) => ExitCode::SUCCESS,
        Ok(true) => ExitCode::from(1),
        Err(err) => {
            eprintln!("Error: {err:?}");
            ExitCode::from(2)
        }
    }
}
