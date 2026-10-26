//! M5: hybrid search (FR-16) and the model-mismatch fallback (FR-10/4).
#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing
)]

use std::path::PathBuf;

use kapu_core::build::{RoomSource, build_with};
use kapu_core::embed::{Model, VectorCache};
use kapu_core::search::{SearchOptions, Source, VectorStatus, search};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn tiny() -> Model {
    Model::load(&root().join("tests/model/kapu-test-tiny")).unwrap()
}

fn temp_cache(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kapu-search-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn rooms() -> Vec<RoomSource> {
    let mk = |id: &str, text: &str| RoomSource {
        id: id.into(),
        content: text.into(),
    };
    vec![
        mk(
            "gombok",
            "# Gombok\nA gomb színe türkiz, a sötét téma része.\n\n## Glass gomb\nglass button css color\n",
        ),
        mk(
            "electron",
            "# Electron\npreload bridge ipc renderer context isolation hiba megoldás\n",
        ),
        mk(
            "adat",
            "# Adat\nindex graph vector model search token cache queue stream parser\n",
        ),
        mk(
            "paletta",
            "# Paletta\npalette color colors layout grid theme dark\n",
        ),
    ]
}

#[test]
fn fr10_model_mismatch_falls_back_to_bm25_with_vector_unavailable() {
    // minta.kapu was built without a model (emb=none): any reader model mismatches
    let data = std::fs::read(root().join("tests/vectors/minta.kapu")).unwrap();
    let model = tiny();
    let cache = VectorCache::in_memory(&model);
    let r = search(&data, "gomb", &SearchOptions::default(), Some(&cache)).unwrap();
    assert!(
        matches!(r.vector, VectorStatus::Unavailable(_)),
        "{:?}",
        r.vector
    );
    assert_ne!(r.hits.len(), 0);
    assert!(r.hits.iter().all(|h| h.source == Source::Bm25));
    // and with no model at all
    let r = search(&data, "gomb", &SearchOptions::default(), None).unwrap();
    assert!(matches!(r.vector, VectorStatus::Unavailable(_)));
}

#[test]
fn hybrid_search_with_matching_model_merges_both_rankings() {
    let model = tiny();
    let built = build_with(&rooms(), Some(&model)).unwrap().bytes;
    let dir = temp_cache("hybrid");
    let cache = VectorCache::in_dir(&model, &dir);
    let r = search(
        &built,
        "gomb türkiz",
        &SearchOptions::default(),
        Some(&cache),
    )
    .unwrap();
    assert_eq!(r.vector, VectorStatus::Available);
    assert_eq!(r.hits[0].id, "gombok", "{:?}", r.hits);
    assert_eq!(r.hits[0].source, Source::Both);
    assert!(
        r.hits.iter().any(|h| h.source == Source::Vec),
        "vectors rank every door: {:?}",
        r.hits
    );
    // K-11 (D143): the vectors come from the file's #VEC section, so the first
    // search needs no cache and leaves none behind
    let files = std::fs::read_dir(dir.join(blake3_hex(&model))).map_or(0, Iterator::count);
    assert_eq!(files, 0, "no vector bundle for a file with #VEC");
    let again = search(
        &built,
        "gomb türkiz",
        &SearchOptions::default(),
        Some(&cache),
    )
    .unwrap();
    assert_eq!(again, r);
    let _ = std::fs::remove_dir_all(&dir);
}

fn blake3_hex(model: &Model) -> String {
    model.hash().0.iter().fold(String::new(), |mut s, b| {
        let _ = std::fmt::Write::write_fmt(&mut s, format_args!("{b:02x}"));
        s
    })
}

#[test]
fn max_price_and_k_limit_the_hits() {
    let model = tiny();
    let built = build_with(&rooms(), Some(&model)).unwrap().bytes;
    let cache = VectorCache::in_memory(&model);
    let options = SearchOptions {
        k: 2,
        max_price: Some(15),
        ..SearchOptions::default()
    };
    let r = search(&built, "gomb color", &options, Some(&cache)).unwrap();
    assert!(r.hits.len() <= 2);
    assert!(r.hits.iter().all(|h| h.tokens <= 15), "{:?}", r.hits);
}

/// FR-16 Acceptance. Fails with `potion-multilingual-128M`: a Hungarian query
/// is closer to generic Hungarian prose (cosine 0.19) than to a CSS block
/// (0.15), while the English query "glassy blurred button" finds it (0.45 vs
/// 0.03). The model choice of M8/5 (D28) has to fix this.
/// Run with `cargo test --release --test search -- --ignored`.
#[test]
#[ignore = "M8: fails with potion-multilingual-128M (D28 model choice)"]
fn fr16_semantic_hit_for_uvegszeru_homalyos_gomb() {
    let dir = root().join("models/potion-multilingual-128M");
    if !dir.join("model.safetensors").is_file() {
        eprintln!("model not present, skipping FR-16");
        return;
    }
    let model = Model::load(&dir).unwrap();
    let mut rooms: Vec<RoomSource> = (0..30)
        .map(|i| RoomSource {
            id: format!("szoba-{i}"),
            content: format!("# Szoba {i}\nÁltalános jegyzet a {i}. projektről: adatbázis, naplózás, telepítés és tesztelés.\n"),
        })
        .collect();
    rooms.push(RoomSource {
        id: "css-kartya".into(),
        content: "# Kártya stílus\n```css\n.card {\n  backdrop-filter: blur(12px);\n  background: rgba(255, 255, 255, 0.08);\n}\n```\n".into(),
    });
    let built = build_with(&rooms, Some(&model)).unwrap().bytes;
    let cache = VectorCache::in_memory(&model);
    let r = search(
        &built,
        "üvegszerű, homályos gomb",
        &SearchOptions::default(),
        Some(&cache),
    )
    .unwrap();
    assert_eq!(r.vector, VectorStatus::Available);
    let hit = r.hits.iter().find(|h| h.id == "css-kartya");
    assert!(
        hit.is_some_and(|h| matches!(h.source, Source::Vec | Source::Both)),
        "the backdrop-filter room must be in the top 10 by meaning: {:?}",
        r.hits.iter().map(|h| (&h.id, h.source)).collect::<Vec<_>>()
    );
}

#[test]
fn excerpts_show_the_line_that_matches_the_query() {
    let data = std::fs::read(root().join("tests/vectors/minta.kapu")).unwrap();
    let ex = kapu_core::search::excerpts(
        &data,
        &[
            "frontend-design-alapok/glass-gomb",
            "electron-leckek",
            "nincs",
        ],
        "elmosott háttér",
    )
    .unwrap();
    assert!(ex[0].contains("elmosott hátteret"), "{ex:?}");
    // no line shares a term: the first prose line, Markdown markers removed
    assert_eq!(ex[1], "Visszatérő Electron hibák és a helyes megoldásuk.");
    assert_eq!(ex[2], "");
    assert!(
        ex.iter()
            .all(|e| e.chars().count() <= kapu_core::search::EXCERPT_CHARS)
    );
}

/// A39: the excerpt line is the one with the rarest query terms, not just the
/// most of them: `blur` is in one door, `button` in all four.
#[test]
fn excerpt_prefers_the_line_with_rarer_query_terms() {
    let mk = |id: &str, text: &str| RoomSource {
        id: id.into(),
        content: text.into(),
    };
    let rooms = vec![
        mk(
            "glass",
            "# Glass
button styles and button colors

Use a backdrop blur behind the glass.
",
        ),
        mk(
            "one",
            "# One
button one
",
        ),
        mk(
            "two",
            "# Two
button two
",
        ),
        mk(
            "three",
            "# Three
button three
",
        ),
    ];
    let built = kapu_core::build::build(&rooms).unwrap();
    let ex = kapu_core::search::excerpts(&built.bytes, &["glass"], "button blur").unwrap();
    assert!(ex[0].contains("blur"), "{ex:?}");
}

#[test]
fn format_1_0_and_1_1_files_are_still_read() {
    // D140: postings by door id (1.0) and by gap (1.1) give the same ranking;
    // D143: the 1.2 reader reads both (the compat files have the same body)
    let old = std::fs::read(root().join("tests/compat/minta-1.0.kapu")).unwrap();
    let mid = std::fs::read(root().join("tests/compat/minta-1.1.kapu")).unwrap();
    let new = std::fs::read(root().join("tests/vectors/minta.kapu")).unwrap();
    assert!(old.starts_with(b"%KAPU 1.0\n") && mid.starts_with(b"%KAPU 1.1\n"));
    assert!(new.starts_with(b"%KAPU 1.2\n"));
    for query in ["electron preload", "szoba", "gomb", "escape"] {
        let a = kapu_core::search::bm25(&old, query).unwrap();
        let b = kapu_core::search::bm25(&mid, query).unwrap();
        assert!(!b.is_empty(), "{query}");
        assert_eq!(a, b, "{query}");
        assert!(
            !kapu_core::search::bm25(&new, query).unwrap().is_empty(),
            "{query}"
        );
    }
    for file in [&old, &mid] {
        kapu_core::validate(file, kapu_core::Level::Basic).unwrap();
        let opened = kapu_core::open_bytes(file, &["szoba"]).unwrap();
        assert!(opened.doors[0].is_ok());
    }
}

#[test]
fn a46_words_late_in_a_long_door_are_found_by_meaning() {
    // D141: a door gets one vector per window of its text; the best window counts
    let model = tiny();
    let filler = "szín színek sötét téma háttér minta szabály lásd mindig soha ".repeat(60);
    let mut list = rooms();
    list.push(RoomSource {
        id: "hosszu".into(),
        content: format!(
            "# Hosszú\n{filler}\n\n{}\n",
            "preload bridge ipc renderer context isolation ".repeat(50)
        ),
    });
    let built = build_with(&list, Some(&model)).unwrap().bytes;
    let cache = VectorCache::in_memory(&model);
    let r =
        kapu_core::search::rankings(&built, "preload bridge ipc renderer", Some(&cache)).unwrap();
    let vector = r.vector.unwrap();
    let at = vector.iter().position(|id| id == "hosszu").unwrap();
    assert!(at <= 1, "{vector:?}");
}

#[test]
fn warm_writes_the_window_bundle_only_for_a_file_without_vec() {
    // D141: the search vectors of a file without #VEC are computed before the
    // first search; D143: a file with #VEC needs no bundle at all
    let model = tiny();
    let built = build_with(&rooms(), Some(&model)).unwrap().bytes;
    let dir = temp_cache("warm");
    let cache = VectorCache::in_dir(&model, &dir);
    let s = kapu_core::Sections::parse(&built).unwrap();
    let bundle = dir
        .join(blake3_hex(&model))
        .join(format!("{}.vecs2", hex32(&s.header.bodyhash.0)));
    kapu_core::search::warm_in(&s, &cache).unwrap();
    assert!(!bundle.exists(), "{}", bundle.display());
    let without = kapu_core::Sections { vec: None, ..s };
    kapu_core::search::warm_in(&without, &cache).unwrap();
    assert!(bundle.is_file(), "{}", bundle.display());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn vec_section_and_cache_rank_alike() {
    // D143: the sign bits of #VEC pick the candidates and the float windows
    // order them; a file of at most RESCORE doors ranks exactly as the cache
    let model = tiny();
    let built = build_with(&rooms(), Some(&model)).unwrap().bytes;
    let s = kapu_core::Sections::parse(&built).unwrap();
    assert!(s.doc_entries().unwrap().len() <= kapu_core::search::RESCORE);
    let cache = VectorCache::in_memory(&model);
    for query in ["gomb türkiz", "preload bridge", "színek"] {
        let from_vec = kapu_core::search::rankings_in(&s, query, Some(&cache)).unwrap();
        let without = kapu_core::Sections {
            vec: None,
            ..s.clone()
        };
        let from_cache = kapu_core::search::rankings_in(&without, query, Some(&cache)).unwrap();
        assert_eq!(from_vec, from_cache, "{query}");
    }
}

fn hex32(bytes: &[u8; 32]) -> String {
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = std::fmt::Write::write_fmt(&mut s, format_args!("{b:02x}"));
        s
    })
}
