wasm_out := "target/wasm32-unknown-unknown/release/peel_wasm.wasm"

# Build the viewer's WebAssembly core and its JS bindings into viewer/src/wasm.
wasm:
    @cargo build -p peel-wasm --release --target wasm32-unknown-unknown
    @wasm-bindgen --target web --out-dir viewer/src/wasm {{ wasm_out }}

viewer-dev: wasm
    @pnpm --filter peel-viewer dev

viewer-build: wasm
    @pnpm --filter peel-viewer build

# Build peel with a freshly built viewer embedded for `peel view`.
build *args: viewer-build
    @cargo build {{ args }}

# Build the viewer and publish it to peel.pomme.rs (Cloudflare Pages project `peel`).
deploy: viewer-build
    @pnpm --filter peel-viewer exec wrangler pages deploy

fixture_version := "26.3"
fixture_dir := "fixtures/" + fixture_version
fixture_server := "target/fixture-server"
vanilla := "node fixtures/vanilla.mjs"

# Start a vanilla server on a fresh flat world for capturing reference traces.
fixture-server server_jar:
    @mkdir -p {{ fixture_server }} && rm -rf {{ fixture_server }}/world
    @cp fixtures/server.properties {{ fixture_server }}/
    @java="$({{ vanilla }} java {{ fixture_version }})" && cd {{ fixture_server }} && "$java" -jar "{{ absolute_path(server_jar) }}" nogui

# Record a reference trace of a scenario from fixtures/README.md.
fixture-capture scenario:
    @mkdir -p {{ fixture_dir }}
    @cargo run --release -p pomme-peel -- capture --out {{ fixture_dir }}/{{ scenario }}.trace --force

# Start the installed vanilla client as `Player`, joining the capture.
fixture-client:
    @{{ vanilla }} client {{ fixture_version }}

pre-pr: wasm
    @cargo fmt --all -- --check
    @cargo clippy --workspace --all-targets -- -D warnings
    @cargo test --workspace
    @pnpm --filter peel-viewer pre-pr
