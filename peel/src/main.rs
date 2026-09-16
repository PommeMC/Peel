mod capture;
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
    Capture(capture::Args),
    /// Print a trace grouped by tick.
    Dump(dump::Args),
    /// Open a trace in the viewer.
    View(view::Args),
}

fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Command::Capture(args) => capture::run(args),
        Command::Dump(args) => dump::run(args),
        Command::View(args) => view::run(args),
    }
}
