//! NFR-02 / M4: the same body and model give the same file on every platform.
//!
//! Builds a small corpus with the committed tiny test model
//! (`tests/model/kapu-test-tiny`, made by `tools/make_test_model.py`), so the
//! build exercises `sim` edges, Leiden and the model header, and compares the
//! BLAKE3 of the output with a fixed value. CI runs this on Windows, Linux and
//! macOS; a different hash on any of them is a determinism bug.
#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing
)]

use std::fmt::Write as _;
use std::path::PathBuf;

use kapu_core::build::{RoomSource, build_with};
use kapu_core::embed::Model;
use kapu_core::{Level, Sections, validate_with};

/// BLAKE3 of the corpus built with the tiny model (since D93). Change only on purpose,
/// together with a decision-log entry.
const GOLDEN: &str = "b7b272aa89d10bdeec2a4fbf4eff809c9462fd29a169ed0c5ee7ba3c0b5c099c";

fn tiny() -> Model {
    Model::load(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/model/kapu-test-tiny"))
        .unwrap()
}

fn corpus() -> Vec<RoomSource> {
    let topics: [(&str, &[&str]); 3] = [
        (
            "design",
            &[
                "gomb", "szín", "sötét", "téma", "türkiz", "bronz", "glass", "button", "color",
                "css", "palette", "layout",
            ],
        ),
        (
            "electron",
            &[
                "electron",
                "preload",
                "bridge",
                "ipc",
                "renderer",
                "context",
                "isolation",
                "hiba",
                "megoldás",
                "function",
            ],
        ),
        (
            "data",
            &[
                "index", "graph", "vector", "model", "search", "token", "cache", "queue", "stream",
                "parser", "file", "data",
            ],
        ),
    ];
    let mut rooms = Vec::new();
    for i in 0..24usize {
        let (topic, words) = topics[i % 3];
        let mut text = format!("# {topic} {i}\n\n");
        for k in 0..30 {
            text.push_str(words[(i * 7 + k * 5) % words.len()]);
            text.push(if k % 9 == 8 { '\n' } else { ' ' });
        }
        let _ = write!(
            text,
            "\n\n## részlet {i}\n{} {}\n",
            words[i % words.len()],
            words[(i + 3) % words.len()]
        );
        if i % 5 == 0 {
            let _ = write!(text, "\n[tovább](#{topic}-{})\n", (i + 3) % 24);
        }
        rooms.push(RoomSource {
            id: format!("{topic}-{i}"),
            content: text,
        });
    }
    rooms
}

#[test]
fn build_with_tiny_model_is_identical_on_every_platform() {
    let model = tiny();
    let built = build_with(&corpus(), Some(&model)).unwrap();
    let s = Sections::parse(&built.bytes).unwrap();
    assert_eq!(s.header.emb, "kapu-test-tiny");
    let kinds: Vec<&str> = s
        .graph
        .lines()
        .map(|l| l.split('\t').nth(2).unwrap())
        .collect();
    assert!(kinds.contains(&"sim"), "the corpus must produce sim edges");
    assert!(kinds.contains(&"link"));
    assert!(s.header.groups >= 2, "Leiden must find more than one group");
    // the full validator rebuilds with the same model and compares bytes
    let report = validate_with(&built.bytes, Level::Full, Some(&model)).unwrap();
    assert!(report.rebuild_checked);
    // without the model the rebuild comparison is skipped (W001), not failed
    assert!(
        !validate_with(&built.bytes, Level::Full, None)
            .unwrap()
            .rebuild_checked
    );

    let hash = blake3::hash(&built.bytes).to_hex().to_string();
    println!(
        "tiny-model corpus: {} bytes, {} groups, BLAKE3 {hash}",
        built.bytes.len(),
        s.header.groups
    );
    assert_eq!(
        hash, GOLDEN,
        "the built file changed or differs on this platform"
    );
}

#[test]
fn build_twice_with_model_is_byte_identical() {
    let model = tiny();
    let a = build_with(&corpus(), Some(&model)).unwrap().bytes;
    let b = build_with(&corpus(), Some(&model)).unwrap().bytes;
    assert_eq!(a, b);
}
