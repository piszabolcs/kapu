//! The commands behind the window. One entry point, [`Session::dispatch`], takes a
//! command name and JSON arguments and answers with JSON, so the Tauri window and
//! the development server (`examples/serve.rs`) share every line of it.
//!
//! The window only reads (D115): writing stays with `kapu` and `kapu-mcp`.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Instant;

use kapu_core::build::NO_MODEL;
use kapu_core::catalog::parse_graph;
use kapu_core::embed::{Model, VectorCache, find_model_dir};
use kapu_core::search::{SearchOptions, VectorStatus};
use kapu_core::view::{DoorLine, map_view};
use kapu_core::{Error, Hash, Header, Level, Sections, about};
use serde::Serialize;
use serde_json::{Value, json};

use crate::markdown;

/// A failed command: the FR-20 code (or `GUI` for the window's own errors) and
/// what went wrong.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ApiError {
    /// `E001`..`E012`, or `GUI`.
    pub code: String,
    /// The detail, as the CLI prints it.
    pub detail: String,
}

impl ApiError {
    /// An error of the window itself (bad arguments, no open file).
    #[must_use]
    pub fn gui(detail: impl Into<String>) -> Self {
        Self {
            code: "GUI".to_owned(),
            detail: detail.into(),
        }
    }
}

impl From<Error> for ApiError {
    fn from(e: Error) -> Self {
        Self {
            code: e.code.as_str().to_owned(),
            detail: e.detail,
        }
    }
}

type ApiResult = Result<Value, ApiError>;

/// The file the window has open, read once per `open` or `reload`.
struct OpenFile {
    path: PathBuf,
    bytes: Vec<u8>,
}

/// State behind every command: the open file and the vector model it needs.
pub struct Session {
    initial: Option<PathBuf>,
    file: Mutex<Option<Arc<OpenFile>>>,
    model: Mutex<Option<(String, Arc<Model>)>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

fn hex(h: &Hash) -> String {
    h.0.iter().fold(String::with_capacity(64), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

fn door_json(d: &DoorLine) -> Value {
    json!({ "id": d.id, "title": d.title, "tokens": d.tokens, "room": d.is_room, "tags": d.tags })
}

/// The map grouped: every group with its door lines in body order.
fn grouped(bytes: &[u8]) -> Result<Vec<(Value, Vec<DoorLine>)>, Error> {
    let view = map_view(bytes, None)?;
    view.groups
        .iter()
        .map(|g| {
            let doors = map_view(bytes, Some(&g.gid))?.doors;
            let head =
                json!({ "gid": g.gid, "name": g.name, "rooms": g.rooms, "tokens": g.tokens });
            Ok((head, doors))
        })
        .collect()
}

fn arg_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, ApiError> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| ApiError::gui(format!("missing argument `{key}`")))
}

fn arg_u64(args: &Value, key: &str) -> Option<u64> {
    args.get(key).and_then(Value::as_u64)
}

/// A room's text starts with its title, description and tags lines (FR-05);
/// the reader shows those as metadata, so the rendered text skips them.
fn skip_room_head(text: &str) -> &str {
    let mut rest = text;
    for prefix in ["# ", "> ", "tags:"] {
        let line_end = rest.find('\n').map_or(rest.len(), |i| i + 1);
        let line = rest.get(..line_end).unwrap_or("");
        if line.starts_with(prefix) || line.trim_end() == prefix.trim_end() {
            rest = rest.get(line_end..).unwrap_or("");
        }
    }
    rest.trim_start_matches('\n')
}

fn elapsed_ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

impl Session {
    /// A session; `initial` is the file named on the command line, opened by the
    /// interface when it starts.
    #[must_use]
    pub fn new(initial: Option<PathBuf>) -> Self {
        Self {
            initial,
            file: Mutex::new(None),
            model: Mutex::new(None),
        }
    }

    /// Runs one command.
    ///
    /// | command    | arguments                     | answer |
    /// |------------|-------------------------------|--------|
    /// | `start`    |                               | app info card, file to open first |
    /// | `open`     | `path`                        | header, groups with door lines, quick check |
    /// | `reload`   |                               | as `open`, for the open file |
    /// | `room`     | `id`                          | door with rendered text, description, neighbours |
    /// | `search`   | `query`, `k`?, `max_price`?, `kind`? | hybrid search hits (FR-16) |
    /// | `graph`    |                               | doors and `#GRAPH` edges |
    /// | `validate` | `full`?                       | FR-20 result |
    ///
    /// # Errors
    /// An unknown command, a missing argument, no open file, or the FR-20 error
    /// of the file.
    pub fn dispatch(&self, command: &str, args: &Value) -> ApiResult {
        match command {
            "start" => Ok(self.start()),
            "open" => self.open(Path::new(arg_str(args, "path")?)),
            "reload" => {
                let path = self.current()?.path.clone();
                self.open(&path)
            }
            "room" => self.room(arg_str(args, "id")?),
            "search" => self.search(args),
            "graph" => self.graph(),
            "validate" => self.validate(args.get("full").and_then(Value::as_bool) == Some(true)),
            other => Err(ApiError::gui(format!("unknown command `{other}`"))),
        }
    }

    fn start(&self) -> Value {
        let layers: Vec<Value> = about::LAYERS
            .iter()
            .map(|(name, role)| json!({ "name": name, "role": role }))
            .collect();
        let repositories: Vec<Value> = about::REPOSITORIES
            .iter()
            .map(|(name, url)| json!({ "name": name, "url": url }))
            .collect();
        json!({
            "about": {
                "name": "KAPU",
                "version": about::VERSION,
                "format": kapu_core::FORMAT_VERSION,
                "signature": about::SIGNATURE,
                "purpose": about::PURPOSE,
                "layers": layers,
                "built_with": about::BUILT_WITH,
                "repositories": repositories,
            },
            "initial": self.initial.as_ref().map(|p| p.display().to_string()),
        })
    }

    fn current(&self) -> Result<Arc<OpenFile>, ApiError> {
        lock(&self.file)
            .clone()
            .ok_or_else(|| ApiError::gui("no file is open"))
    }

    fn open(&self, path: &Path) -> ApiResult {
        let bytes = std::fs::read(path).map_err(|e| ApiError {
            code: "E012".to_owned(),
            detail: format!("{}: {e}", path.display()),
        })?;
        let header = Header::parse(&bytes)?;
        let groups = grouped(&bytes)?;
        let check = match kapu_core::validate(&bytes, Level::Basic) {
            Ok(_) => json!({ "valid": true }),
            Err(e) => json!({ "valid": false, "code": e.code.as_str(), "detail": e.detail }),
        };
        let max_door = groups
            .iter()
            .flat_map(|(_, doors)| doors.iter().map(|d| d.tokens))
            .max()
            .unwrap_or(0);
        let tokens: u64 = groups
            .iter()
            .filter_map(|(g, _)| g.get("tokens").and_then(Value::as_u64))
            .sum();
        let groups: Vec<Value> = groups
            .into_iter()
            .map(|(mut head, doors)| {
                if let Some(obj) = head.as_object_mut() {
                    obj.insert("doors".to_owned(), doors.iter().map(door_json).collect());
                }
                head
            })
            .collect();
        let name = path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let answer = json!({
            "path": path.display().to_string(),
            "name": name,
            "bytes": bytes.len(),
            "header": {
                "rooms": header.rooms,
                "doors": header.doors,
                "groups": header.groups,
                "emb": header.emb,
                "guide": header.guide,
                "bodyhash": hex(&header.bodyhash),
            },
            "tokens": tokens,
            "max_door_tokens": max_door,
            "groups": groups,
            "check": check,
        });
        *lock(&self.file) = Some(Arc::new(OpenFile {
            path: path.to_owned(),
            bytes,
        }));
        Ok(answer)
    }

    fn room(&self, id: &str) -> ApiResult {
        let file = self.current()?;
        let bytes = &file.bytes;
        let opened = kapu_core::open_bytes(bytes, &[id])?;
        let door = opened
            .doors
            .into_iter()
            .next()
            .ok_or_else(|| ApiError::gui("nothing opened"))??;
        let described = kapu_core::describe_bytes(bytes, &[id])?
            .into_iter()
            .next()
            .transpose()?;
        let s = Sections::parse(bytes)?;
        let entries = s.doc_entries()?;
        let fingerprint = entries
            .iter()
            .find(|e| e.id == id)
            .map(|e| format!("b3:{}", hex(&e.hash)));
        let prefix = format!("{id}/");
        let subs: Vec<&kapu_core::DocEntry<'_>> = entries
            .iter()
            .filter(|e| e.id.starts_with(&prefix))
            .collect();
        let map = kapu_core::catalog::parse_map(s.map)?;
        let children: Vec<Value> = subs
            .iter()
            .map(|e| {
                let line = map.door(e.id);
                json!({
                    "id": e.id,
                    "title": line.map_or("", |l| l.title),
                    "tokens": line.map_or(0, |l| l.tokens),
                })
            })
            .collect();
        let sub_ids: Vec<String> = subs.iter().map(|e| e.id.to_owned()).collect();
        let is_room = !id.contains('/');
        let text = if is_room {
            skip_room_head(&door.text)
        } else {
            door.text.as_str()
        };
        let is_door = |target: &str| map.door(target).is_some();
        let html = markdown::render(text, &is_door, if is_room { &sub_ids } else { &[] });
        let neighbours: Vec<Value> = door
            .neighbours
            .iter()
            .map(|n| json!({ "id": n.id, "title": n.title, "tokens": n.tokens, "kind": n.kind }))
            .collect();
        let parent = id.split_once('/').map(|(room, _)| {
            let title = map.door(room).map_or("", |l| l.title);
            json!({ "id": room, "title": title })
        });
        Ok(json!({
            "id": door.id,
            "title": door.title,
            "tokens": door.tokens,
            "room": is_room,
            "parent": parent,
            "description": described.as_ref().map_or("", |d| d.description.as_str()),
            "tags": described.as_ref().map_or_else(Vec::new, |d| d.tags.clone()),
            "fingerprint": fingerprint,
            "html": html,
            "children": children,
            "neighbours": neighbours,
            "index_stale": opened.index_stale,
        }))
    }

    /// The model named in the file header, loaded once and kept (FR-10/4).
    fn model_for(&self, bytes: &[u8]) -> Result<Option<Arc<Model>>, ApiError> {
        let header = Header::parse(bytes)?;
        if header.emb == NO_MODEL {
            return Ok(None);
        }
        let mut cached = lock(&self.model);
        if let Some((name, model)) = cached.as_ref()
            && *name == header.emb
        {
            return Ok(Some(Arc::clone(model)));
        }
        let Some(dir) = find_model_dir(&header.emb) else {
            return Ok(None);
        };
        let model = Arc::new(Model::load(&dir)?);
        *cached = Some((header.emb, Arc::clone(&model)));
        Ok(Some(model))
    }

    fn search(&self, args: &Value) -> ApiResult {
        let file = self.current()?;
        let query = arg_str(args, "query")?;
        let k = arg_u64(args, "k").map_or(10, |k| usize::try_from(k).unwrap_or(10));
        let max_price = arg_u64(args, "max_price").or_else(|| arg_u64(args, "max_tokens"));
        let start = Instant::now();
        let model = self.model_for(&file.bytes)?;
        let model_ms = elapsed_ms(start);
        let cache = model.as_deref().map(VectorCache::new);
        let start = Instant::now();
        let r = kapu_core::search::search(
            &file.bytes,
            query,
            &SearchOptions {
                k,
                max_price,
                kind: args.get("kind").and_then(Value::as_str).map(str::to_owned),
            },
            cache.as_ref(),
        )?;
        let vector = match &r.vector {
            VectorStatus::Available => json!({ "available": true }),
            VectorStatus::Unavailable(why) => json!({ "available": false, "reason": why }),
        };
        let hits: Vec<Value> = r
            .hits
            .iter()
            .map(|h| json!({ "id": h.id, "title": h.title, "tokens": h.tokens, "score": h.score, "source": h.source.as_str() }))
            .collect();
        Ok(json!({
            "query": query,
            "hits": hits,
            "vector": vector,
            "search_ms": elapsed_ms(start),
            "model_ms": model_ms,
        }))
    }

    fn graph(&self) -> ApiResult {
        let file = self.current()?;
        let s = Sections::parse(&file.bytes)?;
        let edges: Vec<Value> = parse_graph(s.graph)?
            .iter()
            .map(|e| json!({ "from": e.from, "to": e.to, "kind": e.kind, "weight": e.weight }))
            .collect();
        let mut nodes = Vec::new();
        for (head, doors) in grouped(&file.bytes)? {
            for d in &doors {
                nodes.push(json!({
                    "id": d.id,
                    "title": d.title,
                    "tokens": d.tokens,
                    "room": d.is_room,
                    "gid": head.get("gid").cloned().unwrap_or(Value::Null),
                }));
            }
        }
        Ok(json!({ "nodes": nodes, "edges": edges }))
    }

    fn validate(&self, full: bool) -> ApiResult {
        let file = self.current()?;
        let start = Instant::now();
        let level = if full { Level::Full } else { Level::Basic };
        let model = if full {
            self.model_for(&file.bytes)?
        } else {
            None
        };
        let answer = match kapu_core::validate_with(&file.bytes, level, model.as_deref()) {
            Ok(report) => {
                let mut warnings: Vec<Value> = report
                    .warnings
                    .iter()
                    .map(|w| json!({ "code": w.code, "detail": w.message }))
                    .collect();
                if full && !report.rebuild_checked {
                    warnings.push(
                        json!({ "code": "W001", "detail": "model not available, rebuild check skipped (D50)" }),
                    );
                }
                json!({
                    "valid": true,
                    "rooms": report.rooms,
                    "doors": report.doors,
                    "full": report.full,
                    "rebuild_checked": report.rebuild_checked,
                    "warnings": warnings,
                })
            }
            Err(e) => {
                json!({ "valid": false, "full": full, "code": e.code.as_str(), "detail": e.detail })
            }
        };
        let mut answer = answer;
        if let Some(obj) = answer.as_object_mut() {
            obj.insert("ms".to_owned(), json!(elapsed_ms(start)));
        }
        Ok(answer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/vectors/minta.kapu")
    }

    fn open_sample() -> Session {
        let s = Session::new(Some(sample()));
        let path = sample().display().to_string();
        s.dispatch("open", &json!({ "path": path })).unwrap();
        s
    }

    #[test]
    fn open_lists_groups_with_their_doors() {
        let s = Session::new(None);
        let v = s
            .dispatch("open", &json!({ "path": sample().display().to_string() }))
            .unwrap();
        assert_eq!(v["header"]["rooms"], 3);
        assert_eq!(v["header"]["doors"], 7);
        assert_eq!(v["groups"].as_array().unwrap().len(), 2);
        assert_eq!(v["groups"][0]["doors"].as_array().unwrap().len(), 6);
        assert_eq!(v["check"]["valid"], true);
        assert_eq!(v["tokens"], 486);
    }

    #[test]
    fn room_renders_without_its_head_lines_and_with_sub_door_anchors() {
        let s = open_sample();
        let v = s
            .dispatch("room", &json!({ "id": "frontend-design-alapok" }))
            .unwrap();
        let html = v["html"].as_str().unwrap();
        assert!(!html.contains("tags:"), "{html}");
        assert!(
            html.contains(r#"id="door-frontend-design-alapok/szinek""#),
            "{html}"
        );
        assert!(
            html.contains(r##"href="#room=electron-leckek/preload-bridge""##),
            "{html}"
        );
        assert_eq!(v["children"].as_array().unwrap().len(), 2);
        assert_eq!(v["tags"].as_array().unwrap().len(), 6);
        assert!(v["fingerprint"].as_str().unwrap().starts_with("b3:"));
        let sub = s
            .dispatch("room", &json!({ "id": "electron-leckek/preload-bridge" }))
            .unwrap();
        assert_eq!(sub["parent"]["id"], "electron-leckek");
    }

    #[test]
    fn search_graph_and_validate_answer() {
        let s = open_sample();
        let r = s
            .dispatch("search", &json!({ "query": "glass gomb", "k": 3 }))
            .unwrap();
        assert_eq!(r["hits"][0]["id"], "frontend-design-alapok/glass-gomb");
        assert_eq!(r["vector"]["available"], false);
        let g = s.dispatch("graph", &json!({})).unwrap();
        assert_eq!(g["nodes"].as_array().unwrap().len(), 7);
        assert_eq!(g["edges"].as_array().unwrap().len(), 3);
        let v = s.dispatch("validate", &json!({ "full": true })).unwrap();
        assert_eq!(v["valid"], true);
        assert_eq!(v["rebuild_checked"], true);
    }

    #[test]
    fn errors_carry_a_code() {
        let s = Session::new(None);
        assert_eq!(s.dispatch("graph", &json!({})).unwrap_err().code, "GUI");
        assert_eq!(s.dispatch("nope", &json!({})).unwrap_err().code, "GUI");
        let missing = s
            .dispatch("open", &json!({ "path": "no/such/file.kapu" }))
            .unwrap_err();
        assert_eq!(missing.code, "E012");
        let s = open_sample();
        assert_eq!(
            s.dispatch("room", &json!({ "id": "nincs" }))
                .unwrap_err()
                .code,
            "E009"
        );
    }
}
