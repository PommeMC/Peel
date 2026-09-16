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

pre-pr: wasm
    @cargo fmt --all -- --check
    @cargo clippy --workspace --all-targets -- -D warnings
    @cargo test --workspace
    @pnpm --filter peel-viewer pre-pr
