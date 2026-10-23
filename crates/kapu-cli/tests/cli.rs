//! M6: integration test of every `kapu` command on the sample file, with the
//! `--json` output shape pinned. Runs without a vector model (`--no-model`),
//! so it is the same on every platform.
#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing
)]

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn sample() -> PathBuf {
    root().join("tests/vectors/minta.kapu")
}

fn temp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kapu-cli-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Runs `kapu --json --no-model <args>`; returns (exit success, parsed JSON).
fn kapu(args: &[&str]) -> (bool, Value) {
    let out = Command::new(env!("CARGO_BIN_EXE_kapu"))
        .args(["--json", "--no-model"])
        .args(args)
        .env(
            "KAPU_CACHE_DIR",
            std::env::temp_dir().join("kapu-cli-cache"),
        )
        .output()
        .unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    let json: Value =
        serde_json::from_str(stdout.trim()).unwrap_or_else(|e| panic!("not JSON ({e}): {stdout}"));
    assert!(
        json.get("timing_ms").is_some(),
        "every response carries timing_ms: {json}"
    );
    (out.status.success(), json)
}

fn s(p: &Path) -> &str {
    p.to_str().unwrap()
}

#[test]
fn validate_map_search_describe_open_on_the_sample() {
    let file = sample();
    let (ok, v) = kapu(&["validate", "--full", s(&file)]);
    assert!(ok, "{v}");
    assert_eq!(v["valid"], true);
    assert_eq!(v["rebuild_checked"], true);

    let (ok, v) = kapu(&["map", s(&file)]);
    assert!(ok);
    assert!(
        v["guide"]
            .as_str()
            .unwrap()
            .starts_with("Ez egy KAPU tudásfájl.")
    );
    assert_eq!(v["flat"], true);
    assert_eq!(v["groups"].as_array().unwrap().len(), 2);
    assert_eq!(v["doors"].as_array().unwrap().len(), 7);
    for key in ["id", "title", "tokens", "group", "tags", "room"] {
        assert!(v["doors"][0].get(key).is_some(), "map door has `{key}`");
    }
    let (ok, v) = kapu(&["map", s(&file), "--group", "g2"]);
    assert!(ok);
    assert!(v["guide"].is_null());
    assert_eq!(v["doors"][0]["id"], "szoba");

    let (ok, v) = kapu(&["search", s(&file), "gomb", "--k", "3"]);
    assert!(ok);
    assert_eq!(v["vector"]["unavailable"], "no vector model");
    let hits = v["hits"].as_array().unwrap();
    assert!(!hits.is_empty() && hits.len() <= 3);
    for key in ["id", "title", "tokens", "score", "source"] {
        assert!(hits[0].get(key).is_some(), "hit has `{key}`");
    }
    let (_, v) = kapu(&["search", s(&file), "gomb", "--max-tokens", "100"]);
    assert!(
        v["hits"]
            .as_array()
            .unwrap()
            .iter()
            .all(|h| h["tokens"].as_u64().unwrap() <= 100)
    );

    let (ok, v) = kapu(&["describe", s(&file), "electron-leckek", "nincs"]);
    assert!(ok, "unknown ids fail per id, not the command");
    assert_eq!(v["doors"][0]["tags"].as_array().unwrap().len(), 4);
    assert_eq!(v["doors"][1]["error"]["code"], "E009");

    let (ok, v) = kapu(&[
        "open",
        s(&file),
        "frontend-design-alapok/glass-gomb",
        "szoba",
    ]);
    assert!(ok);
    assert_eq!(v["index"], "ok");
    let frame = v["doors"][0]["frame"].as_str().unwrap();
    assert!(frame.starts_with(
        "<kapu-room file=\"minta.kapu\" id=\"frontend-design-alapok/glass-gomb\" tokens=\"108\">"
    ));
    assert_eq!(v["doors"][0]["neighbours"][0]["type"], "link");
}

#[test]
fn errors_carry_the_fr20_code_and_a_failing_exit() {
    for (name, code) in [
        ("E001-magic.kapu", "E001"),
        ("E006-bodyhash.kapu", "E006"),
        ("E009-dangling.kapu", "E009"),
    ] {
        let (ok, v) = kapu(&[
            "validate",
            "--full",
            s(&root().join("tests/vectors").join(name)),
        ]);
        assert!(!ok, "{name}");
        assert_eq!(v["error"]["code"], code, "{name}: {v}");
    }
    let (ok, v) = kapu(&[
        "open",
        s(&root().join("tests/vectors/edge-stale-shifted.kapu")),
        "electron-leckek",
    ]);
    assert!(ok);
    assert_eq!(v["index"], "stale");
}

#[test]
fn new_import_add_rm_build_export_round_trip() {
    let dir = temp("write");
    let file = dir.join("uj.kapu");
    let (ok, v) = kapu(&["new", s(&file)]);
    assert!(ok, "{v}");
    assert_eq!(v["emb"], "none");
    assert_eq!(v["rooms"], 1);
    let (ok, _) = kapu(&["new", s(&file)]);
    assert!(!ok, "new refuses to overwrite");

    let md = dir.join("gomb.md");
    std::fs::write(&md, "# Glass gomb\r\nÜveg hatású gomb.\r\n").unwrap();
    let (ok, v) = kapu(&["add", s(&file), s(&md)]);
    assert!(ok, "{v}");
    assert_eq!(v["id"], "glass-gomb");
    assert_eq!(v["rooms"], 2);
    assert_eq!(
        v["warnings"][0]["code"], "W004",
        "CRLF is normalised with a warning"
    );

    let (ok, v) = kapu(&["rm", s(&file), "szoba"]);
    assert!(ok, "{v}");
    assert_eq!(v["rooms"], 1);
    let (ok, v) = kapu(&["rm", s(&file), "nincs"]);
    assert!(!ok);
    assert_eq!(v["error"]["code"], "E009");

    let (ok, _) = kapu(&["build", s(&file)]);
    assert!(ok);
    let (ok, _) = kapu(&["validate", "--full", s(&file)]);
    assert!(ok);

    // export then import gives back the same rooms
    let out = dir.join("export");
    let (ok, v) = kapu(&["export", s(&sample()), s(&out)]);
    assert!(ok);
    assert_eq!(v["written"].as_array().unwrap().len(), 3);
    let again = dir.join("again.kapu");
    let (ok, v) = kapu(&["import", s(&out), s(&again)]);
    assert!(ok, "{v}");
    assert_eq!(v["rooms"], 3);
    let original = std::fs::read_to_string(out.join("electron-leckek.md")).unwrap();
    let out2 = dir.join("export2");
    kapu(&["export", s(&again), s(&out2)]);
    assert_eq!(
        std::fs::read_to_string(out2.join("electron-leckek.md")).unwrap(),
        original
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn bench_reports_recall_and_mrr() {
    let dir = temp("bench");
    let q = dir.join("kerdesek.jsonl");
    std::fs::write(
        &q,
        "{\"kerdes\":\"üvegszerű gomb css\",\"valasz_fajl\":\"frontend-design-alapok.md\",\"valasz_szakasz\":\"Glass gomb\",\"nyelv\":\"hu\"}\n\
         {\"kerdes\":\"contextIsolation preload\",\"valasz_fajl\":\"electron-leckek.md\",\"valasz_szakasz\":\"Preload bridge\",\"nyelv\":\"en\"}\n",
    )
    .unwrap();
    let (ok, v) = kapu(&["bench", s(&sample()), s(&q)]);
    assert!(ok, "{v}");
    assert_eq!(v["questions"], 2);
    assert!(v["vector"].is_null(), "no model: no vector scores");
    for key in [
        "recall@5",
        "recall@10",
        "mrr@10",
        "lenient_recall@5",
        "lenient_recall@10",
        "lenient_mrr@10",
    ] {
        assert!(
            v["bm25"].get(key).is_some() && v["hybrid"].get(key).is_some(),
            "{key}"
        );
    }
    assert!(v["bm25"]["lenient_recall@10"].as_f64().unwrap() > 0.99);
    let _ = std::fs::remove_dir_all(&dir);
}
