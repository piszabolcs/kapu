//! The window's KAPU maker (KD1): the `create` command runs `kapu-create`
//! (preview, make, refresh) and reports its progress as `create-progress`
//! events.

use std::path::Path;
use std::sync::Mutex;
use std::time::Instant;

use kapu_core::embed::{Model, find_default_model_dir};
use kapu_create::create::{self, Progress, Settings};
use kapu_create::rooms;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::api::ApiError;

/// The options the window sends.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Options {
    /// OCR languages, `+`-joined.
    pub ocr_lang: String,
    /// The most tokens of a room.
    pub room_max: usize,
    /// An overview room per document.
    pub overview: bool,
    /// Sections copied word for word kept once.
    pub dedupe: bool,
    /// The most web pages per address.
    pub max_pages: usize,
    /// The URL prefix web pages must start with (empty: the default).
    pub scope: String,
    /// Compute the vectors with the packaged model.
    pub vectors: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            ocr_lang: "eng+deu+hun".into(),
            room_max: 4000,
            overview: true,
            dedupe: true,
            max_pages: 5000,
            scope: String::new(),
            vectors: true,
        }
    }
}

impl Options {
    fn settings(&self) -> Settings {
        let mut s = Settings::default();
        if !self.ocr_lang.trim().is_empty() {
            self.ocr_lang.trim().clone_into(&mut s.read.ocr.langs);
        }
        s.rooms = rooms::Settings {
            room_max: self.room_max.max(300),
            overview: self.overview,
            dedupe: self.dedupe,
            ..rooms::Settings::default()
        };
        s.web.max_pages = self.max_pages.max(1);
        self.scope.trim().clone_into(&mut s.web.scope);
        s
    }

    fn model(&self) -> Option<Model> {
        if !self.vectors {
            return None;
        }
        find_default_model_dir().and_then(|d| Model::load(&d).ok())
    }
}

/// One progress step as the window shows it.
fn progress_json(p: &Progress<'_>) -> Value {
    match p {
        Progress::Fetched { done, total, url } => {
            json!({"step": "fetch", "done": done, "total": total, "what": url})
        }
        Progress::Read { done, total, name } => {
            json!({"step": "read", "done": done, "total": total, "what": name})
        }
        Progress::Planned { rooms } => {
            json!({"step": "plan", "done": rooms, "total": rooms, "what": ""})
        }
        Progress::Vectors { done, total } => {
            json!({"step": "vectors", "done": done, "total": total, "what": ""})
        }
        Progress::Written { path, pending } => {
            json!({"step": "written", "done": 1, "total": 1, "what": path.display().to_string(), "pending": pending})
        }
    }
}

/// Runs one maker command; `emit` gets the progress (at most about five
/// times a second, and always the last step of a kind).
///
/// # Errors
/// A missing file name, or the maker's error.
pub fn run(
    mode: &str,
    file: Option<&str>,
    sources: &[String],
    options: &Options,
    emit: &(dyn Fn(Value) + Sync),
) -> Result<Value, ApiError> {
    let settings = options.settings();
    let last = Mutex::new((Instant::now(), String::new()));
    let progress = |p: Progress<'_>| {
        let v = progress_json(&p);
        let step = v
            .get("step")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let finished = v.get("done") == v.get("total");
        let Ok(mut l) = last.lock() else { return };
        if finished || l.1 != step || l.0.elapsed().as_millis() >= 200 {
            *l = (Instant::now(), step);
            drop(l);
            emit(v);
        }
    };
    let need_file = || {
        file.filter(|f| !f.trim().is_empty())
            .ok_or_else(|| ApiError::gui("choose where to write the file"))
    };
    let report = match mode {
        "preview" => create::preview(sources, &settings, &progress).map(|(r, _)| r),
        "make" => {
            let file = need_file()?;
            let model = options.model();
            create::make(
                sources,
                Path::new(file),
                &settings,
                model.as_ref(),
                &progress,
            )
        }
        "refresh" => {
            let file = need_file()?;
            let model = options.model();
            create::refresh(Path::new(file), &settings, model.as_ref(), &progress)
        }
        other => return Err(ApiError::gui(format!("unknown maker command `{other}`"))),
    }
    .map_err(|e| ApiError::gui(e.0))?;
    serde_json::to_value(&report).map_err(|e| ApiError::gui(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_and_make_report_progress() {
        let dir = std::env::temp_dir().join(format!("kapu-gui-create-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.md"), "# Alpha\n\nThe alpha text.\n").unwrap();
        let sources = vec![dir.to_string_lossy().into_owned()];
        let options = Options {
            vectors: false,
            ..Options::default()
        };
        let steps = Mutex::new(Vec::new());
        let emit = |v: Value| {
            steps
                .lock()
                .unwrap()
                .push(v["step"].as_str().unwrap_or_default().to_owned())
        };
        let report = run("preview", None, &sources, &options, &emit).unwrap();
        assert_eq!(report["rooms"], 1);
        assert!(steps.lock().unwrap().contains(&"read".to_owned()));
        assert!(
            run("make", None, &sources, &options, &emit).is_err(),
            "make needs a file"
        );
        let out = dir.join("out.kapu");
        let report = run(
            "make",
            Some(&out.to_string_lossy()),
            &sources,
            &options,
            &emit,
        )
        .unwrap();
        assert_eq!(report["rooms"], 1);
        assert!(out.is_file() && dir.join("out.sources.json").is_file());
        let report = run(
            "refresh",
            Some(&out.to_string_lossy()),
            &[],
            &options,
            &emit,
        )
        .unwrap();
        assert_eq!(report["cached"], 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
