//! Embeds the built viewer (`viewer/dist`) for `peel view`. Without a build,
//! a placeholder page says how to make one, so a Rust-only checkout still
//! compiles.

use std::path::{Path, PathBuf};
use std::{env, fs};

const PLACEHOLDER: &str = "<!doctype html><title>Peel</title>\
    <p>This build of Peel has no viewer embedded. Run <code>just build</code> \
    to build the viewer and Peel together.</p>";

fn main() {
    let dist = Path::new(env!("CARGO_MANIFEST_DIR")).join("../viewer/dist");
    println!("cargo::rerun-if-changed={}", dist.display());
    let out = PathBuf::from(env::var("OUT_DIR").expect("cargo sets OUT_DIR"));

    let mut assets = Vec::new();
    if dist.is_dir() {
        collect(&dist, &dist, &mut assets);
    }
    if assets.is_empty() {
        let placeholder = out.join("placeholder.html");
        fs::write(&placeholder, PLACEHOLDER).expect("writing the placeholder page");
        assets.push(("index.html".to_owned(), placeholder));
    }

    let entries: String = assets
        .iter()
        .map(|(path, file)| {
            format!(
                "    ({path:?}, include_bytes!({:?})),\n",
                file.display().to_string()
            )
        })
        .collect();
    fs::write(
        out.join("viewer.rs"),
        format!("pub static ASSETS: &[(&str, &[u8])] = &[\n{entries}];\n"),
    )
    .expect("writing the viewer asset table");
}

/// Every file under `dir`, keyed by its URL path relative to `root`.
fn collect(root: &Path, dir: &Path, assets: &mut Vec<(String, PathBuf)>) {
    for entry in fs::read_dir(dir).expect("reading viewer/dist") {
        let path = entry.expect("reading viewer/dist").path();
        if path.is_dir() {
            collect(root, &path, assets);
        } else {
            let relative = path.strip_prefix(root).expect("path under viewer/dist");
            let url = relative
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            assets.push((url, path));
        }
    }
}
