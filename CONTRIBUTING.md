# Contributing to Peel

Thanks for your interest in contributing to Peel!

## Getting Started

1. Fork the repository
2. Clone your fork and set up the `pomme-protocol` override as described under
   [Building](./README.md#building), using your fork's URL
3. Build and run:

   ```bash
   pnpm install
   just build
   cargo run -- view path/to/session.trace
   ```

   For viewer work, `just viewer-dev` starts Vite's dev server with hot reload;
   open a trace from its landing page.

## Before Submitting a PR

This must pass:

```bash
just pre-pr
```

It runs `cargo fmt --check`, clippy with `-D warnings`, `cargo test`, the
WebAssembly build, and the viewer's Prettier, ESLint, TypeScript and Vitest
checks.

For changes to capture, also run `peel capture` against a local offline-mode
server with the vanilla client, and for viewer changes open a real trace in
`peel view`. Say in the PR what you checked. Passing tests and a real run are
different things; don't call a change working until it has been run.

## Development Guidelines

- **The toolchain is pinned** in `rust-toolchain.toml`, to the same nightly as
  the Pomme client so `pomme-protocol` builds identically in both. Never
  override it locally; the two repos bump it together
- **Peel depends on the released `pomme-protocol`.** If Peel needs something it
  lacks, add it in the client repo first, as its own PR; never fork or vendor it
- No unnecessary comments. Code should be self-explanatory
- No DRY violations. Don't duplicate logic, extract shared helpers
- Keep changes focused. One feature or fix per PR; plan multi-part work as a
  stack of PRs
- Use `feat/`, `fix/`, `perf/`, `refactor/`, `chore/`, `docs/` branch prefixes

### Scope

The guarantees under [Privacy and scope](./README.md#privacy-and-scope) are
hard limits; PRs that break them won't be merged. In particular, never build
online-mode interception. Beyond those:

- **No Mojang code or assets in the repo.** Don't commit jars, assets or
  decompiled source
- **Frame features as conformance, not evasion.** A difference from vanilla
  means the client should match vanilla, never that the difference should be
  hidden. Anticheat developers are Peel's users too

### Protocol work

- **Wire formats come from the decompiled vanilla source**, never from memory,
  other projects or community protocol wikis. A packet's layout is its record
  and `STREAM_CODEC`; the byte-level truth is each codec's `write()`. The
  decompiled reference is built in the client repo; its `CLAUDE.md` explains
  how
- **Never hardcode a packet id.** Look packets up by name through
  `pomme_protocol::PacketTable` for the trace's protocol, including in tests
- **Every decoder gets byte-exact tests**, one per protocol range where its
  layout differs. A packet without a decoder still shows its name and raw
  bytes; never guess at fields
- **Decoder protocol ranges come from diffing the codec across the reference
  versions.** Name each boundary with a version constant in
  `peel-decode/src/fields/packets/mod.rs`
- **Version support comes only from `pomme-protocol`.** Never add or patch
  packet tables in Peel

### Viewer

- **Decoding stays in Rust.** Anything that turns records into names, phases,
  ticks or fields belongs in `peel-decode`, exposed through `peel-wasm`; never
  reimplement it in TypeScript
- **Works offline.** No CDNs, external fonts, analytics or network calls; a
  trace never leaves the browser. Built pages carry a Content Security Policy
  (`viewer/vite.config.ts`) that only allows the page's own origin, so anything
  new must be served with the viewer
- **Polish is in scope.** Screenshots of the viewer are the project's main
  marketing

### Reference traces

Vanilla reference traces live in `fixtures/`, and
[`fixtures/README.md`](./fixtures/README.md) explains how to capture them. When
you recapture or add a scenario, follow its steps exactly and update the README
in the same PR. `cargo test` checks that every packet in them decodes.

### Trace format

- **Store raw frames, never decoded fields**, so decoder fixes apply to old
  traces
- **Every format change bumps `FORMAT_VERSION`**, and the reader must keep
  opening every older version. Add a golden test for the old layout
- **`peel-trace` is shared with Pomme's built-in recorder.** Keep its API small
  and its dependencies to serde and flate2

## Pull Request Format

Describe what changed and why. For protocol changes, name the classes in the
decompiled reference you checked the change against; reviewers compare against
the same classes.

For bug fixes, also include:

```markdown
- What the issue was
- What caused it
- How it was fixed
```

## Project Structure

The workspace crates are listed under [Architecture](./README.md#architecture)
in the README. Inside them:

```bash
peel/
└── src/
    ├── main.rs             # Entry point and CLI
    ├── capture.rs          # Proxy, connection handling, recorder thread
    ├── session.rs          # Per-connection framing, compression and phase tracking
    ├── wire.rs             # Frame splitting, decompression, handshake fields
    ├── cli.rs              # Trace loading and output shared by the commands
    ├── dump.rs             # `peel dump`: text and JSON output
    ├── coverage.rs         # `peel coverage`: what decoded, and what stopped
    ├── diff.rs             # `peel diff`: text and JSON output
    ├── view.rs             # `peel view`: local server for the embedded viewer
    └── status.rs           # Log lines and capture's live status line
peel-decode/
└── src/
    ├── lib.rs              # Loading a whole trace
    ├── names.rs            # Bridges trace types to pomme-protocol's tables
    ├── timeline.rs         # Tick grouping, shared by dump and the viewer
    ├── coverage.rs         # What a trace decodes, shared by coverage and the viewer
    ├── diff.rs             # Tick-by-tick comparison, shared by diff and the viewer
    └── fields/
        ├── mod.rs          # Decoder lookup by phase, direction, name and protocol
        ├── reader.rs       # Vanilla's primitive types
        ├── decoder.rs      # Records each field's value and byte range
        └── packets/        # One module per part of the protocol, plus NBT and text components
peel-wasm/
└── src/lib.rs              # The viewer's WebAssembly API
viewer/
└── src/
    ├── App.tsx             # Opening a trace, and a reference to compare it against
    ├── components/         # The trace view and diff view, and the pieces they share
    └── lib/                # The wasm wrapper, filtering, rows, diff lines and formatting, with tests
peel-trace/
└── src/
    ├── lib.rs              # Trace types, errors, format version
    ├── write.rs            # TraceWriter
    ├── read.rs             # TraceReader, every format version
    └── varint.rs           # Varint encoding
fixtures/
├── README.md               # Setup, capture steps and scenarios
├── <version>/              # One reference trace per scenario
├── server.properties       # The vanilla server's settings for captures
├── options.txt             # The vanilla client's settings for captures
└── vanilla.mjs             # Starts the installed vanilla client as `Player`
```

## Reporting Issues

Include reproduction steps, your OS, your Peel version (`peel --version`) and
the Minecraft version. A trace of the problem helps a lot, but traces contain
usernames, chat and positions: only attach one you are happy to make public,
ideally from a test server and account.

## Code of Conduct

Be respectful. We're all here to build something cool.
