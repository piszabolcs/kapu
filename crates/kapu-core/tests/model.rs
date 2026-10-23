//! M4: the Rust model loader against Python `model2vec` (`tools/model_reference.py`).
//! Runs only when `models/potion-multilingual-128M` is present (it is not in
//! git, D29); otherwise the tests pass without checking anything and say so.
#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation
)]

use std::path::PathBuf;

use kapu_core::embed::{Model, cosine};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn model() -> Option<Model> {
    let dir = root().join("models/potion-multilingual-128M");
    if !dir.join("model.safetensors").is_file() {
        eprintln!("model not present, skipping: {}", dir.display());
        return None;
    }
    Some(Model::load(&dir).unwrap())
}

#[test]
fn token_ids_and_vectors_match_python_model2vec() {
    let Some(model) = model() else { return };
    let reference: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root().join("tests/model/potion_reference.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(model.dim(), 256);
    assert_eq!(model.name(), "potion-multilingual-128M");
    for case in reference["cases"].as_array().unwrap() {
        let text = case["text"].as_str().unwrap();
        let want_ids: Vec<u32> = case["ids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as u32)
            .collect();
        let ids = model.token_ids(text).unwrap();
        assert_eq!(
            ids,
            want_ids,
            "token ids of {:?}",
            &text[..text.len().min(40)]
        );
        let want: Vec<f32> = case["vector"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap() as f32)
            .collect();
        let got = model.embed(text).unwrap();
        if want.iter().all(|x| *x == 0.0) {
            assert!(
                got.iter().all(|x| *x == 0.0),
                "empty text gives the zero vector"
            );
            continue;
        }
        let sim = cosine(&got, &want);
        let max_diff = got
            .iter()
            .zip(&want)
            .map(|(a, b)| (a - b).abs())
            .fold(0f32, f32::max);
        assert!(
            sim > 0.999_999 && max_diff < 1e-5,
            "cosine {sim}, max diff {max_diff} for {:?}",
            &text[..text.len().min(40)]
        );
    }
}

#[test]
fn embedding_is_deterministic() {
    let Some(model) = model() else { return };
    let text = "Az üvegszerű gomb elmosott hátteret kap.";
    let a = model.embed(text).unwrap();
    let b = model.embed(text).unwrap();
    assert_eq!(
        a.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
        b.iter().map(|x| x.to_bits()).collect::<Vec<_>>()
    );
}

#[test]
fn similar_meanings_are_closer_than_unrelated_ones() {
    let Some(model) = model() else { return };
    let glass = model.embed("üvegszerű, homályos gomb").unwrap();
    let blur = model
        .embed(".btn { backdrop-filter: blur(12px); background: rgba(255,255,255,0.08); }")
        .unwrap();
    let tax = model
        .embed("A társasági adó bevallásának határideje május 31.")
        .unwrap();
    assert!(
        cosine(&glass, &blur) > cosine(&glass, &tax),
        "{} vs {}",
        cosine(&glass, &blur),
        cosine(&glass, &tax)
    );
}
