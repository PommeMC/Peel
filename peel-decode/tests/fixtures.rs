//! Checks the vanilla reference traces in `fixtures/`.

use std::fs;
use std::path::{Path, PathBuf};

use peel_decode::names::TickMarker;
use peel_decode::{Loaded, load};
use peel_trace::Phase;
use pomme_protocol::ProtocolVersion;

struct Fixture {
    path: String,
    version: ProtocolVersion,
    loaded: Loaded,
}

/// Every `fixtures/<version>/*.trace`.
fn fixtures() -> Vec<Fixture> {
    let mut fixtures = Vec::new();
    for dir in entries(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../fixtures")) {
        if !dir.is_dir() {
            continue;
        }
        let name = dir.file_name().unwrap().to_string_lossy();
        let version = ProtocolVersion::from_name(&name)
            .unwrap_or_else(|| panic!("fixtures/{name} isn't a supported version"));
        for path in entries(&dir) {
            if path.extension().is_some_and(|ext| ext == "trace") {
                let loaded = load(fs::read(&path).unwrap().as_slice()).unwrap();
                fixtures.push(Fixture {
                    path: path.display().to_string(),
                    version,
                    loaded,
                });
            }
        }
    }
    assert!(!fixtures.is_empty(), "no reference traces in fixtures/");
    fixtures
}

fn entries(dir: &Path) -> impl Iterator<Item = PathBuf> {
    fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
}

#[test]
fn fixtures_are_whole_sessions_in_their_version() {
    for Fixture {
        path,
        version,
        loaded,
    } in fixtures()
    {
        assert!(loaded.error.is_none(), "{path}: {:?}", loaded.warning());
        let records = &loaded.records;
        assert!(
            records.iter().all(|r| r.protocol == version.protocol),
            "{path} isn't all protocol {}",
            version.protocol
        );
        assert!(
            records.iter().any(|r| r.phase == Phase::Game),
            "{path} never reaches the game phase"
        );
        if let Some(marker) = TickMarker::for_protocol(version.protocol) {
            assert!(
                records.iter().any(|r| marker.ends_tick(r)),
                "{path} has no tick markers"
            );
        }
    }
}

#[test]
fn every_fixture_packet_is_named_and_decodes() {
    for Fixture { path, loaded, .. } in fixtures() {
        let mut timeline = loaded.timeline();
        assert!(
            timeline.packets().all(|p| p.name.is_some()),
            "{path} has unnamed packets"
        );
        timeline.decode_fields(&loaded.records);
        let counts = timeline.field_counts();
        // TODO: also require `partly == 0` once registry-decoders lands.
        assert_eq!(counts.failed, 0, "{path}: {counts:?}");
    }
}
