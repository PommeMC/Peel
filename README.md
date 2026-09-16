<p align="center">
  <h1 align="center">Peel</h1>
  <p align="center">A Minecraft packet analyser from the Pomme team</p>
  <p align="center">
    <a href="https://discord.gg/ucBA55bHPR">Discord</a> · <a href="https://github.com/PommeMC/Peel/issues">Issues</a>
  </p>
</p>

---

Peel records the packets a Minecraft: Java Edition client and server exchange,
and shows them tick by tick in any supported version. It is a protocol
debugging and conformance tool for plugin and server developers, client and
mod developers, anticheat developers, and proxy developers: when something
behaves differently from vanilla, Peel shows exactly which packets differ and
when.

It is the sister project of the [Pomme client](https://github.com/PommeMC/Client)
and is built on that repo's `pomme-protocol` crate.

> **Status:** early. Capturing, dumping and viewing traces work today; diffing
> against vanilla is next.

## Features

- **Transparent capture proxy**: sits between a client and a local offline-mode server and forwards every byte unmodified, while recording a copy
- **Every version from 1.20 through 26.3**: packet names, phases and compression are followed per version from tables generated from the decompiled game, including the configuration phase (1.20.2+)
- **Tick-accurate timelines**: from 1.21.2 (protocol 768) packets are grouped by the client's own tick marker, `client_tick_end`; older versions are grouped by timestamp and clearly labelled approximate
- **Small, shareable traces**: raw packets in a compressed file; chunk data compresses about 7x. Names and fields are computed when a trace is read, so improvements to Peel apply to old traces too
- **Web viewer**: a filterable tick timeline with a byte-level view of every packet, opened with `peel view`. Traces are decoded in your browser by the same Rust code as `peel dump`, and never uploaded
- **Text or JSON output** for reading traces yourself or scripting against them

Planned:

- **Diff against vanilla**: compare a trace with reference traces captured from the real vanilla client
- **Field decoders**: every packet's fields, ported from the vanilla codecs
- **Hosted viewer**: the same viewer on the Pomme website, for opening a shared trace with nothing installed

## Usage

Peel is a command-line program, `peel`.

### Capture a session

1. Start a local test server with `online-mode=false` in `server.properties`.
   Peel only works with offline-mode servers; see [Privacy and scope](#privacy-and-scope).
2. Start the proxy:

   ```bash
   peel capture --out walk.trace
   ```

   By default it listens on `127.0.0.1:25566` and forwards to a server on
   `127.0.0.1:25565`. Change either with `--listen` and `--server`.
3. In Minecraft, add a server at `127.0.0.1:25566` and join it. While you play,
   Peel shows a live line with the packets, ticks and trace size recorded so far.
4. Disconnect. Peel finishes the trace and exits.

A trace holds one login session. Server-list pings, and any other connection
made while one is recording (such as a server transfer's reconnect), are
forwarded but not recorded. Peel refuses to overwrite an existing trace unless
you pass `--force`, and a capture that is killed partway still leaves a
readable trace.

### Read a trace

```bash
peel dump walk.trace
```

```text
tick 2
    +0.061645s  to client  game           level_chunk_with_light            0x2d  5121 B
    +0.061945s  to server  game           move_player_pos                   0x1e  26 B
    +0.061945s  to server  game           client_tick_end                   0x0d  1 B
```

Each line shows the time since the session started, the direction, the phase,
the packet's name and id, and its uncompressed length. Add `--json` for one
JSON document you can script against.

### View a trace

```bash
peel view walk.trace
```

Opens the trace in your browser: a timeline grouped by tick with repeated
packets folded together, an activity strip for jumping around, filters by
packet name, direction and phase, and the raw bytes of any packet you select.
Step through packets with the arrow keys, and link to one with `#record=N`. The viewer is served from inside `peel`
on `127.0.0.1` and works offline; stop it with Ctrl+C.

## Privacy and scope

- **Traces are private data.** They contain usernames, chat and positions, so
  only share them with people you trust. Peel makes no network connections of
  its own: the proxy only forwards the traffic you send through it, and
  `peel view` only serves your own machine.
- **Offline-mode servers only.** Peel never handles Microsoft or Mojang
  credentials, access tokens or session keys. If a server asks for encryption,
  Peel stops and tells you why.
- **Observe, never modify.** The proxy never rewrites, injects, drops, reorders
  or delays packets, and can't be used to manipulate traffic.

## Architecture

```bash
peel/         # The `peel` binary (crate `pomme-peel`): capture proxy, dump, view
peel-trace/   # The trace file format: types, reader, writer. Shared with Pomme's built-in recorder
peel-decode/  # Packet names and the tick timeline, shared by `peel dump` and the viewer
peel-wasm/    # The viewer's WebAssembly bridge to peel-decode
viewer/       # The web viewer (React, TypeScript, Vite)
```

## Building

Peel is written in Rust, with a TypeScript web viewer. You need:

- [rustup](https://rustup.rs/); the pinned toolchain and WebAssembly target in
  `rust-toolchain.toml` install automatically
- [just](https://github.com/casey/just), [Node.js](https://nodejs.org/) and
  [pnpm](https://pnpm.io/)
- `wasm-bindgen-cli` at exactly the `wasm-bindgen` version pinned in
  `Cargo.toml`: `cargo install wasm-bindgen-cli --version =<version> --locked`

`pomme-protocol` is not on crates.io yet, so for now you also need a checkout of
the Pomme client next to Peel, and a local override pointing cargo at it:

```bash
git clone https://github.com/PommeMC/Client.git
git clone https://github.com/PommeMC/Peel.git
cd Peel
mkdir -p .cargo
printf '[patch.crates-io]\npomme-protocol = { path = "../Client/pomme-protocol" }\n' > .cargo/config.toml
```

`.cargo/config.toml` is gitignored; never commit it. Then:

```bash
pnpm install
just build --release
```

`just build` builds the viewer and embeds it in the binary, which is
`target/release/peel` (`peel.exe` on Windows). A plain `cargo build` also works,
but `peel view` then shows a page saying the viewer wasn't built.

## Contributing

Contributions are welcome.
Please open an issue first to discuss what you'd like to change, and see
[CONTRIBUTING.md](./CONTRIBUTING.md) for the workflow.

## License

This project is licensed under the GNU General Public License v3.0 or later (GPL-3.0-or-later).

It is not affiliated with or endorsed by Mojang Studios or Microsoft.
Minecraft is a trademark of Mojang Studios.

## Community

[![Discord](https://img.shields.io/discord/1483578136544280618?color=5865F2&label=Discord&logo=discord&logoColor=white)](https://discord.gg/ucBA55bHPR)
[![Sponsor](https://img.shields.io/badge/Sponsor-Purdze-ea4aaa?logo=githubsponsors&logoColor=white)](https://github.com/sponsors/Purdze)
