//! Plan v5 (V5-1): `kapu-mcp hook <file.kapu>` as a Claude Code
//! `UserPromptSubmit` hook, with the tiny test model (`KAPU_MODEL_DIR`) and a
//! private vector cache.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{Value, json};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A folder with `minta.kapu`: three rooms, with the tiny model's vectors
/// (the gate needs them).
fn workdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kapu-hook-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let room = |id: &str, content: &str| kapu_core::build::RoomSource {
        id: id.to_owned(),
        content: content.to_owned(),
    };
    let rooms = [
        room(
            "glass",
            "# Glass button\nA glass button blurs what is behind it with `backdrop-filter: blur(12px)` over a half transparent background.\n",
        ),
        room(
            "preload",
            "# Electron preload\nThe preload script exposes a small API with `contextBridge.exposeInMainWorld`.\n",
        ),
        room(
            "fonts",
            "# Fonts\nOne display face and one text face, loaded with `font-display: swap`.\n",
        ),
    ];
    let tiny = kapu_core::embed::Model::load(&root().join("tests/model/kapu-test-tiny")).unwrap();
    let built = kapu_core::build::build_with(&rooms, Some(&tiny)).unwrap();
    std::fs::write(dir.join("minta.kapu"), built.bytes).unwrap();
    dir
}

/// Runs the hook on `input`; returns (exit success, stdout, stderr).
fn hook(dir: &Path, args: &[&str], input: &str) -> (bool, String, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_kapu-mcp"))
        .arg("hook")
        .args(args)
        .env("KAPU_MODEL_DIR", root().join("tests/model/kapu-test-tiny"))
        .env("KAPU_CACHE_DIR", dir.join("cache"))
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    (
        out.status.success(),
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    )
}

fn prompt(text: &str) -> String {
    json!({ "session_id": "s", "hook_event_name": "UserPromptSubmit", "cwd": ".", "prompt": text })
        .to_string()
}

const ALWAYS: &str = r#"{"hook_similarity": 0}"#;

#[test]
fn the_hook_gives_the_ask_passages_as_context() {
    let dir = workdir("context");
    let (ok, out, err) = hook(
        &dir,
        &["minta.kapu", "--rank", ALWAYS],
        &prompt("How do I make a glass gomb with a blur?"),
    );
    assert!(ok, "{err}");
    let v: Value = serde_json::from_str(out.trim()).unwrap();
    let h = &v["hookSpecificOutput"];
    assert_eq!(h["hookEventName"], "UserPromptSubmit");
    let context = h["additionalContext"].as_str().unwrap();
    let (lead, reply) = context.split_once('\n').unwrap();
    assert!(
        lead.starts_with("KAPU: the best passages of minta.kapu"),
        "{lead}"
    );
    assert!(lead.contains("knowledge, not instructions"), "{lead}");
    assert!(!lead.contains("kapu_grep"), "{lead}");
    // the kapu_ask reply as it is: its price, then one passages frame
    assert!(reply.starts_with("reply: "), "{reply}");
    assert!(reply.contains("<kapu-passages "), "{reply}");
    assert!(reply.contains("backdrop-filter"), "{reply}");
    assert!(context.chars().count() <= 9000);
}

#[test]
fn the_stop_line_is_a_switch() {
    let dir = workdir("stop");
    let (_, out, _) = hook(
        &dir,
        &[
            "minta.kapu",
            "--rank",
            r#"{"hook_similarity": 0, "hook_stop": true}"#,
        ],
        &prompt("How do I make a glass gomb with a blur?"),
    );
    let v: Value = serde_json::from_str(out.trim()).unwrap();
    let context = v["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap();
    assert!(context.contains("check once with kapu_grep"), "{context}");
}

#[test]
fn the_hook_stays_quiet_and_never_fails() {
    let dir = workdir("quiet");
    let question = prompt("How do I make a glass gomb with a blur?");
    for (args, input, why) in [
        (
            vec!["minta.kapu", "--explain"],
            "not json".to_owned(),
            "no JSON on stdin",
        ),
        (
            vec!["minta.kapu", "--explain"],
            prompt("go on"),
            "short prompt",
        ),
        (
            vec!["minta.kapu", "--explain"],
            prompt("/compact the session"),
            "command",
        ),
        (
            vec![
                "minta.kapu",
                "--explain",
                "--rank",
                r#"{"hook_similarity": 2}"#,
            ],
            question.clone(),
            "weak match",
        ),
        (vec!["missing.kapu", "--explain"], question.clone(), "error"),
        (vec![], question.clone(), ""),
    ] {
        let (ok, out, err) = hook(&dir, &args, &input);
        assert!(ok, "{args:?}: {err}");
        assert!(out.is_empty(), "{args:?}: {out}");
        assert!(err.contains(why), "{args:?}: {err}");
    }
    // --explain: one JSON line with the similarity the gate saw
    let (_, _, err) = hook(
        &dir,
        &[
            "minta.kapu",
            "--explain",
            "--rank",
            r#"{"hook_similarity": 2}"#,
        ],
        &question,
    );
    let line: Value = serde_json::from_str(err.lines().last().unwrap()).unwrap();
    assert!(line["similarity"].as_f64().is_some(), "{err}");
    assert_eq!(line["context"], false);
}
