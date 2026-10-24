//! M3 builder tests against the M1 vectors (spec chapter 13, M3 checks).
#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::cast_precision_loss
)]

use std::path::PathBuf;

use kapu_core::build::{RoomSource, build, parse_body, rebuild};
use kapu_core::{Level, Sections, validate};

fn vectors_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/vectors")
}

fn vector(name: &str) -> Vec<u8> {
    std::fs::read(vectors_dir().join(name)).unwrap()
}

fn sample_rooms() -> Vec<RoomSource> {
    let data = vector("minta.kapu");
    let s = Sections::parse(&data).unwrap();
    parse_body(s.body).unwrap()
}

/// Shows the first differing line of two files, section by section.
fn first_difference(a: &[u8], b: &[u8]) -> String {
    let (a, b) = (String::from_utf8_lossy(a), String::from_utf8_lossy(b));
    for (n, (x, y)) in a.lines().zip(b.lines()).enumerate() {
        if x != y {
            return format!("line {}:\n rust: {x:?}\n  ref: {y:?}", n + 1);
        }
    }
    format!("lengths {} vs {}", a.len(), b.len())
}

#[test]
fn m3_rust_build_reproduces_minta_kapu_byte_for_byte() {
    let reference = vector("minta.kapu");
    let built = build(&sample_rooms()).unwrap();
    assert!(
        built.bytes == reference,
        "{}",
        first_difference(&built.bytes, &reference)
    );
}

#[test]
fn index_postings_are_gaps_between_door_positions() {
    // D140: a posting is `<gap>:<tf>`, the gap counted in #DOCS lines
    let built = build(&sample_rooms()).unwrap();
    let s = Sections::parse(&built.bytes).unwrap();
    let entries = s.doc_entries().unwrap();
    for line in s.index.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        let decoded = kapu_core::docs::postings(f[2], &entries, &s.header).unwrap();
        assert_eq!(decoded.len().to_string(), f[1], "{line}");
        let mut last = None;
        for (posting, (id, tf)) in f[2].split(',').zip(&decoded) {
            let (gap, t) = posting.split_once(':').unwrap();
            let gap: usize = gap.parse().unwrap();
            let at = last.map_or(gap, |l: usize| l + gap);
            assert!(last.is_none() || gap > 0, "{line}");
            assert_eq!(entries[at].id, *id);
            assert_eq!(t.parse::<u64>().unwrap(), *tf);
            last = Some(at);
        }
    }
}

#[test]
fn build_twice_is_byte_identical_and_validates_full() {
    let a = build(&sample_rooms()).unwrap().bytes;
    let b = rebuild(&a).unwrap().bytes;
    assert_eq!(a, b);
    validate(&a, Level::Full).unwrap();
}

#[test]
fn rebuild_repairs_both_stale_vectors() {
    for name in [
        "edge-stale-same-length.kapu",
        "edge-stale-shifted.kapu",
        "E007-doorhash.kapu",
        "E009-dangling.kapu",
    ] {
        let fixed = rebuild(&vector(name)).unwrap().bytes;
        validate(&fixed, Level::Full).unwrap_or_else(|e| panic!("{name}: {e}"));
    }
}

#[test]
fn fr02_esc_in_body_gives_e004_with_position_and_no_output_file() {
    let rooms = vec![RoomSource {
        id: "a".into(),
        content: "első sor\nmás\u{1b}odik".into(),
    }];
    let err = build(&rooms).unwrap_err();
    assert_eq!(err.code.as_str(), "E004");
    assert!(err.detail.contains("line 2 column 4"), "{}", err.detail);
}

#[test]
fn fr04_room_marker_in_content_round_trips_without_new_room() {
    let content = "# Escape\n@@@ hamis\n\\@@@ szintén\n".to_owned();
    let built = build(&[RoomSource {
        id: "e".into(),
        content: content.clone(),
    }])
    .unwrap();
    let opened = kapu_core::open_bytes(&built.bytes, &["e", "hamis"]).unwrap();
    assert_eq!(opened.doors[0].as_ref().unwrap().text, content);
    assert!(opened.doors[1].is_err(), "no room `hamis` was created");
    let s = Sections::parse(&built.bytes).unwrap();
    assert_eq!(s.header.rooms, 1);
}

#[test]
fn fr05_unfilled_template_room_gets_captions_from_text() {
    let template = kapu_core::build::TEMPLATE;
    let content = format!(
        "{template}Az üvegszerű gomb elmosott hátteret kap, a gomb színe türkiz. A gomb kerek.\n"
    );
    let built = build(&[RoomSource {
        id: "t".into(),
        content,
    }])
    .unwrap();
    let d = kapu_core::describe_bytes(&built.bytes, &["t"]).unwrap();
    let d = d[0].as_ref().unwrap();
    assert!(!d.title.is_empty(), "title from the text");
    assert!(
        d.description.starts_with("Az üvegszerű gomb"),
        "{}",
        d.description
    );
    assert!(!d.tags.is_empty(), "tags from YAKE");
}

#[test]
fn fr06_auto_description_of_4000_token_room_is_180_to_200_tokens() {
    let sentence = "A gomb háttere elmosott, a színe türkiz, és a sötét témában jól látszik. ";
    let mut text = String::from("# Hosszú szoba\n\n");
    while kapu_core::terms::tokens(&text).unwrap() < 4000 {
        text.push_str(sentence);
    }
    let tokens = kapu_core::terms::tokens(&text).unwrap();
    let built = build(&[RoomSource {
        id: "h".into(),
        content: text,
    }])
    .unwrap();
    let d = kapu_core::describe_bytes(&built.bytes, &["h"]).unwrap();
    let desc = &d[0].as_ref().unwrap().description;
    let n = kapu_core::terms::tokens(desc).unwrap();
    let target = (0.05 * tokens as f64).round();
    assert!((180..=200).contains(&n), "{n} tokens, target {target}");
    assert!(
        desc.ends_with(|c: char| !c.is_whitespace()),
        "ends on a word boundary"
    );
}

#[test]
fn fr07_two_subdoors_and_a_heading_inside_a_code_fence() {
    let content = "# Szoba\n## Első\nszöveg\n```\n## nem alajtó\n```\n## Második\nszöveg\n";
    let built = build(&[RoomSource {
        id: "s".into(),
        content: content.into(),
    }])
    .unwrap();
    let s = Sections::parse(&built.bytes).unwrap();
    let ids: Vec<&str> = s.doc_entries().unwrap().iter().map(|e| e.id).collect();
    assert_eq!(ids, ["s", "s/elso", "s/masodik"]);
}

#[test]
fn fr08_price_matches_o200k_base_reference() {
    let exp: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(vectors_dir().join("expected.json")).unwrap(),
    )
    .unwrap();
    for d in exp["minta.kapu"]["doors"].as_array().unwrap() {
        let served = d["served"].as_str().unwrap();
        assert_eq!(
            kapu_core::terms::tokens(served).unwrap(),
            d["tokens"].as_u64().unwrap(),
            "{}",
            d["id"]
        );
    }
}

#[test]
fn fr09_search_gomb_finds_uvegszeru_gomb_and_gombok() {
    let rooms = vec![
        RoomSource {
            id: "a".into(),
            content: "# Üvegszerű gomb\nLeírás.\n".into(),
        },
        RoomSource {
            id: "b".into(),
            content: "# Lista\nA gombok sorban állnak.\n".into(),
        },
        RoomSource {
            id: "c".into(),
            content: "# Más\nSemmi köze.\n".into(),
        },
    ];
    let built = build(&rooms).unwrap();
    let hits = kapu_core::search::bm25(&built.bytes, "gomb").unwrap();
    let ids: Vec<&str> = hits.iter().map(|h| h.id.as_str()).collect();
    assert!(ids.contains(&"a") && ids.contains(&"b"), "{ids:?}");
    assert!(!ids.contains(&"c"));
}

#[test]
fn fr18_notepad_edited_room_builds_to_a_valid_file() {
    let mut data = vector("minta.kapu");
    // a Notepad user appends a sentence to the last room
    data.extend_from_slice("Kézzel írt új mondat.\n".as_bytes());
    assert!(validate(&data, Level::Full).is_err());
    let fixed = rebuild(&data).unwrap().bytes;
    validate(&fixed, Level::Full).unwrap();
    let opened = kapu_core::open_bytes(&fixed, &["szoba"]).unwrap();
    assert!(
        opened.doors[0]
            .as_ref()
            .unwrap()
            .text
            .contains("Kézzel írt új mondat.")
    );
}

#[test]
fn fr15_open_four_doors_returns_four_frames_sized_like_their_prices() {
    let rooms: Vec<RoomSource> = (1..=30)
        .map(|i| RoomSource {
            id: format!("szoba-{i}"),
            content: format!(
                "# Szoba {i}\n> A {i}. szoba rövid leírása.\ntags: minta, {i}\n\n{}\n\n## Részlet {i}\n{}\n",
                "Ez egy hosszabb bekezdés a gombokról és a színekről. ".repeat(i % 7 + 3),
                "Az alajtó saját szövege a témáról. ".repeat(i % 5 + 2)
            ),
        })
        .collect();
    let built = build(&rooms).unwrap();
    let ids = ["szoba-3", "szoba-11", "szoba-20/reszlet-20", "szoba-30"];
    let opened = kapu_core::open_bytes(&built.bytes, &ids).unwrap();
    assert!(!opened.index_stale);
    let frames: Vec<String> = opened
        .doors
        .iter()
        .map(|d| kapu_core::frame("skill.kapu", d.as_ref().unwrap()))
        .collect();
    assert_eq!(frames.len(), 4);
    let response = frames.concat();
    assert_eq!(response.matches("<kapu-room ").count(), 4);
    let price: u64 = opened
        .doors
        .iter()
        .map(|d| d.as_ref().unwrap().tokens)
        .sum();
    let size = kapu_core::terms::tokens(&response).unwrap();
    let body_tokens = kapu_core::terms::tokens(
        std::str::from_utf8(Sections::parse(&built.bytes).unwrap().body).unwrap(),
    )
    .unwrap();
    // same order of magnitude as the four prices, far below the whole body
    assert!(
        size >= price && size <= price * 2 + 4 * 30,
        "response {size} tokens, prices {price}"
    );
    assert!(
        size * 3 < body_tokens,
        "response {size} vs body {body_tokens}"
    );
}

#[test]
fn map_fields_round_trip_backslashes_and_pipes() {
    // D107: a title with `\`, `|` and `\|` comes back exactly from #MAP
    let title = r"C:\temp | a\|b \ end\";
    let built = build(&[RoomSource {
        id: "esc".into(),
        content: format!("# {title}\nszöveg\n"),
    }])
    .unwrap();
    let v = kapu_core::view::map_view(&built.bytes, None).unwrap();
    assert_eq!(v.doors[0].title, title.trim_end());
    let d = kapu_core::describe_bytes(&built.bytes, &["esc"]).unwrap();
    assert_eq!(d[0].as_ref().unwrap().title, title.trim_end());
    kapu_core::validate(&built.bytes, kapu_core::Level::Full).unwrap();
}
