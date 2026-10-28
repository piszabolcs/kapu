//! Plan v3 V2 (D171): the Rust BERT encoder against ONNX Runtime and its `NumPy`
//! re-implementation (`tools/e5_reference.py`). Runs only when
//! `models/multilingual-e5-small-q8` is present (it is not in git, D29);
//! otherwise the tests pass without checking anything and say so.
#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation
)]

use std::fmt::Write as _;
use std::path::PathBuf;

use kapu_core::embed::{Model, cosine};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn model() -> Option<Model> {
    let dir = root().join("models/multilingual-e5-small-q8");
    if !dir.join("model.safetensors").is_file() {
        eprintln!("model not present, skipping: {}", dir.display());
        return None;
    }
    Some(Model::load(&dir).unwrap())
}

fn floats(v: &serde_json::Value) -> Vec<f32> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_f64().unwrap() as f32)
        .collect()
}

#[test]
fn vectors_match_onnx_runtime_and_the_numpy_reference() {
    let Some(model) = model() else { return };
    let reference: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root().join("tests/model/e5_reference.json")).unwrap(),
    )
    .unwrap();
    assert!(model.is_contextual());
    assert_eq!(model.dim(), 384);
    for case in reference["cases"].as_array().unwrap() {
        let text = case["text"].as_str().unwrap();
        let want_ids: Vec<u32> = case["ids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as u32)
            .collect();
        assert_eq!(
            model.token_ids(text).unwrap(),
            want_ids,
            "ids of {text:.40}"
        );
        // the reference texts carry their prefix, which `embed` adds itself
        let got = match text.strip_prefix("passage: ") {
            Some(passage) => model.embed(passage).unwrap(),
            None => model
                .embed_query(text.strip_prefix("query: ").unwrap())
                .unwrap(),
        };
        let numpy = cosine(&got, &floats(&case["numpy"]));
        let ort = cosine(&got, &floats(&case["onnxruntime"]));
        eprintln!(
            "{:4} ids  numpy {numpy:.7}  onnxruntime {ort:.7}",
            want_ids.len()
        );
        // D171: every activation is rounded to 8 bits, so a last-bit difference
        // (this encoder's own exp, erf and sums) can move a value to the next
        // step; ONNX Runtime on another CPU differs from itself as much
        assert!(numpy > 0.995, "{text:.40}: {numpy}");
        assert!(ort > 0.995, "{text:.40}: {ort}");
    }
}

#[test]
fn a_vector_has_the_same_bits_every_time() {
    let Some(model) = model() else { return };
    let text = "A társasági adó bevallásának határideje május 31.";
    assert_eq!(model.embed(text).unwrap(), model.embed(text).unwrap());
}

#[test]
fn long_passages_get_overlapping_windows() {
    let Some(model) = model() else { return };
    let text = "Hosszú szöveg a tokenkorláthoz, sok szóval és írásjellel. ".repeat(200);
    let start = std::time::Instant::now();
    let windows = model.embed_windows(&text).unwrap();
    let took = start.elapsed();
    eprintln!("{} windows in {took:?}", windows.len());
    assert!(windows.len() >= 5, "{}", windows.len());
    // a window is a real part of the text, not the prefix alone
    let first = model.embed_windows(&text[..text.len() / 2]).unwrap();
    assert!(
        first.len() < windows.len(),
        "{} {}",
        first.len(),
        windows.len()
    );
    assert_ne!(windows[0], model.embed("").unwrap());
    assert!(windows.iter().all(|w| w.len() == 384));
}

/// BLAKE3 of the file [`rooms`] gives with `multilingual-e5-small-q8` (D171).
const GOLDEN: &str = "08eaa1ab3581e18a64d8bb1b1db16ff07f3cc08bd7cdd8db9eb6c54dc8c46a00";

fn rooms() -> Vec<kapu_core::build::RoomSource> {
    let room = |id: &str, content: &str| kapu_core::build::RoomSource {
        id: id.to_owned(),
        content: content.to_owned(),
    };
    vec![
        room(
            "tax",
            "# Társasági adó\n\nA társasági adó bevallását május 31-ig kell beadni.\n\n## Előleg\n\nAz adóelőleget negyedévente kell megfizetni.\n\n## Késedelem\n\nKésedelmes bevallás után mulasztási bírság jár.\n",
        ),
        room(
            "css",
            "# Centering in CSS\n\nUse `display: flex; justify-content: center` to center a child horizontally.\n",
        ),
        room(
            "electron",
            "# Electron preload\n\nThe preload script exposes an API with `contextBridge.exposeInMainWorld`.\n",
        ),
    ]
}

#[test]
fn a_contextual_model_stores_whole_vectors_and_builds_the_same_bytes_twice() {
    // D171: format 1.3, `#VEC` with 4 bits, a room with sub-doors keeps its own
    // text, the window log makes the second build compute nothing
    let Some(mut model) = model() else { return };
    let cache = std::env::temp_dir().join(format!("kapu-bert-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&cache);
    model.set_log_root(&cache);
    let first = kapu_core::build::build_with(&rooms(), Some(&model)).unwrap();
    let log = std::fs::read_dir(&cache)
        .unwrap()
        .flatten()
        .map(|d| d.path().join("windows.bin"))
        .find(|p| p.is_file())
        .unwrap();
    let logged = std::fs::metadata(&log).unwrap().len();
    let start = std::time::Instant::now();
    let second = kapu_core::build::build_with(&rooms(), Some(&model)).unwrap();
    assert!(start.elapsed().as_secs_f64() < 2.0, "{:?}", start.elapsed());
    assert_eq!(first.bytes, second.bytes);
    // D46: the same bits from an SSE2, AVX2 and AVX-512 build (checked on
    // 2026-10-28); change only on purpose, with a decision-log entry
    let mut hash = String::new();
    for b in kapu_core::blake3_of(&first.bytes).0 {
        write!(hash, "{b:02x}").unwrap();
    }
    assert_eq!(hash, GOLDEN);
    assert_eq!(std::fs::metadata(&log).unwrap().len(), logged);
    assert!(first.bytes.starts_with(b"%KAPU 1.3\n"));
    let s = kapu_core::Sections::parse(&first.bytes).unwrap();
    let vec = s.vec.unwrap();
    assert!(vec.starts_with("dim=384 bits=4\n"), "{}", &vec[..20]);
    let report =
        kapu_core::validate_with(&first.bytes, kapu_core::Level::Full, Some(&model)).unwrap();
    assert!(
        report.rebuild_checked && report.warnings.is_empty(),
        "{report:?}"
    );
    let _ = std::fs::remove_dir_all(&cache);
    let cache = kapu_core::embed::VectorCache::in_memory(&model);
    let options = kapu_core::search::SearchOptions::default();
    for (query, want) in [
        ("how do I center a div", "css"),
        ("mikor kell beadni az adóbevallást", "tax"),
        ("expose an api to the renderer", "electron"),
    ] {
        let found = kapu_core::search::search(&first.bytes, query, &options, Some(&cache)).unwrap();
        assert_eq!(found.vector, kapu_core::search::VectorStatus::Available);
        let top = found.hits.first().unwrap();
        assert_eq!(
            top.id.split('/').next(),
            Some(want),
            "{query}: {:?}",
            found.hits
        );
    }
}
