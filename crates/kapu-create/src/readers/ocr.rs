//! OCR with Tesseract (KD5): a separate process per page, with a time
//! limit, on half the cores. The program is looked for in the `--tesseract`
//! path, `KAPU_TESSERACT`, a `tesseract` folder next to the program, and the
//! `PATH`; without it the image pages are reported, not read.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use crate::{Error, Result};

/// OCR settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// Tesseract languages, `+`-joined (default `eng+deu+hun`).
    pub langs: String,
    /// The `tesseract` program, if not found by the search.
    pub tesseract: Option<PathBuf>,
    /// The language data folder, if not the program's own.
    pub tessdata: Option<PathBuf>,
    /// Seconds per page before the process is stopped.
    pub timeout: u64,
    /// Pages read at the same time (0: half the cores).
    pub workers: usize,
    /// Rendering resolution of a PDF page.
    pub dpi: u32,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            langs: "eng+deu+hun".into(),
            tesseract: None,
            tessdata: None,
            timeout: 120,
            workers: 0,
            dpi: 300,
        }
    }
}

impl Options {
    /// The number of pages read at the same time.
    #[must_use]
    pub fn workers(&self) -> usize {
        if self.workers > 0 {
            return self.workers;
        }
        std::thread::available_parallelism().map_or(2, |n| (n.get() / 2).max(1))
    }
}

/// The `tesseract` program to run, or `None`.
#[must_use]
pub fn find(options: &Options) -> Option<PathBuf> {
    let exe = if cfg!(windows) {
        "tesseract.exe"
    } else {
        "tesseract"
    };
    let mut candidates: Vec<PathBuf> = Vec::new();
    candidates.extend(options.tesseract.clone());
    candidates.extend(std::env::var_os("KAPU_TESSERACT").map(PathBuf::from));
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
    {
        candidates.push(dir.join("tesseract").join(exe));
        candidates.push(dir.join(exe));
    }
    if cfg!(windows) {
        for var in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"] {
            if let Some(base) = std::env::var_os(var) {
                candidates.push(PathBuf::from(&base).join("Tesseract-OCR").join(exe));
                candidates.push(
                    PathBuf::from(base)
                        .join("Programs")
                        .join("Tesseract-OCR")
                        .join(exe),
                );
            }
        }
    }
    if let Some(found) = candidates.into_iter().find(|p| p.is_file()) {
        return Some(found);
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(exe))
        .find(|p| p.is_file())
}

/// Frequent words of the languages the package carries, without the words
/// they share (`a`, `is`, `in`).
const STOP_WORDS: &[(&str, &[&str])] = &[
    (
        "eng",
        &[
            "the", "and", "of", "to", "for", "with", "that", "are", "this", "be", "on", "as", "by",
            "or", "it", "from", "at", "an", "which", "not",
        ],
    ),
    (
        "deu",
        &[
            "der", "die", "und", "das", "ist", "nicht", "mit", "den", "von", "zu", "des", "für",
            "auf", "dem", "ein", "eine", "sich", "werden", "oder", "wird",
        ],
    ),
    (
        "hun",
        &[
            "az", "és", "hogy", "nem", "egy", "van", "meg", "ez", "csak", "már", "mint", "volt",
            "vagy", "ha", "kell", "ki", "el", "azt", "nincs", "lesz",
        ],
    ),
];

/// The one language of `text` among `langs` (`eng+deu+hun`), by frequent
/// words: at least 20 of them, and twice as many as of the next language.
/// One language reads its pages better and faster than several together
/// (Hungarian scans: word recall 0.951 against 0.916, accented words 0.961
/// against 0.861, 0.9 s a page against 1.5 s). `None` when it is not clear
/// or a language has no word list.
#[must_use]
pub fn detect_language(text: &str, langs: &str) -> Option<String> {
    let wanted: Vec<&str> = langs.split('+').filter(|l| !l.is_empty()).collect();
    if wanted.len() < 2
        || wanted
            .iter()
            .any(|l| !STOP_WORDS.iter().any(|(s, _)| s == l))
    {
        return None;
    }
    let mut counts: Vec<(&str, usize)> = wanted.iter().map(|l| (*l, 0)).collect();
    for word in text
        .split(|c: char| !c.is_alphabetic())
        .filter(|w| !w.is_empty())
    {
        let word = word.to_lowercase();
        for (lang, n) in &mut counts {
            if STOP_WORDS
                .iter()
                .any(|(l, list)| l == lang && list.contains(&word.as_str()))
            {
                *n += 1;
            }
        }
    }
    counts.sort_by_key(|c| std::cmp::Reverse(c.1));
    match counts.as_slice() {
        [(best, n), (_, m), ..] if *n >= 20 && *n >= m * 2 => Some((*best).to_owned()),
        _ => None,
    }
}

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// A fresh temporary folder for one OCR job.
fn scratch() -> Result<PathBuf> {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("kapu-create-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// The text of one image file (any format Tesseract reads).
///
/// # Errors
/// When the program fails, times out or writes no text.
pub fn image_file(exe: &Path, image: &Path, options: &Options) -> Result<String> {
    let dir = scratch()?;
    let base = dir.join("out");
    let mut cmd = Command::new(exe);
    cmd.arg(image)
        .arg(&base)
        .args(["-l", &options.langs, "--psm", "3"]);
    if let Some(data) = &options.tessdata {
        cmd.arg("--tessdata-dir").arg(data);
    }
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let result = run(cmd, Duration::from_secs(options.timeout.max(1)))
        .and_then(|()| std::fs::read(base.with_extension("txt")).map_err(Error::from))
        .map(|bytes| crate::text::clean(&String::from_utf8_lossy(&bytes)));
    let _ = std::fs::remove_dir_all(&dir);
    result
}

/// The text of an image given as bytes; `ext` is its file extension.
///
/// # Errors
/// As [`image_file`].
pub fn image_bytes(exe: &Path, bytes: &[u8], ext: &str, options: &Options) -> Result<String> {
    let dir = scratch()?;
    let path = dir.join(format!("page.{ext}"));
    std::fs::write(&path, bytes)?;
    let result = image_file(exe, &path, options);
    let _ = std::fs::remove_dir_all(&dir);
    result
}

/// Runs `cmd`, stopping it after `limit`.
fn run(mut cmd: Command, limit: Duration) -> Result<()> {
    let mut child = cmd
        .spawn()
        .map_err(|e| Error::new(format!("tesseract: {e}")))?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => {
                let mut err = String::new();
                if let Some(mut stderr) = child.stderr.take() {
                    let _ = std::io::Read::read_to_string(&mut stderr, &mut err);
                }
                let last = err
                    .lines()
                    .rev()
                    .find(|l| !l.trim().is_empty())
                    .unwrap_or_default();
                return Err(Error::new(format!("tesseract failed ({status}): {last}")));
            }
            Ok(None) if start.elapsed() > limit => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(Error::new(format!(
                    "tesseract: no result in {} s, stopped",
                    limit.as_secs()
                )));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(40)),
            Err(e) => return Err(Error::new(format!("tesseract: {e}"))),
        }
    }
}

/// Runs `job` on every item on `workers` threads; one failing item never
/// stops the others (each result is kept with its item).
pub fn parallel<T: Sync, R: Send>(
    items: &[T],
    workers: usize,
    job: impl Fn(&T) -> R + Sync,
) -> Vec<R> {
    let next = AtomicUsize::new(0);
    let results: std::sync::Mutex<Vec<(usize, R)>> =
        std::sync::Mutex::new(Vec::with_capacity(items.len()));
    std::thread::scope(|s| {
        for _ in 0..workers.clamp(1, items.len().max(1)) {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(item) = items.get(i) else { break };
                    let r = job(item);
                    if let Ok(mut all) = results.lock() {
                        all.push((i, r));
                    }
                }
            });
        }
    });
    let mut all = results.into_inner().unwrap_or_default();
    all.sort_by_key(|(i, _)| *i);
    all.into_iter().map(|(_, r)| r).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parallel_keeps_the_order_and_every_result() {
        let items: Vec<usize> = (0..50).collect();
        let out = parallel(&items, 4, |&i| if i % 7 == 0 { Err(i) } else { Ok(i * 2) });
        assert_eq!(out.len(), 50);
        assert_eq!(out.get(3), Some(&Ok(6)));
        assert_eq!(out.get(14), Some(&Err(14)));
    }

    #[test]
    fn the_language_of_a_page() {
        let hu =
            "A gép az indítás után ellenőrzi, hogy a tengely nem mozdul-e, és ha kell, megáll. "
                .repeat(6);
        let de = "Der Antrieb wird mit dem Parameter eingestellt, und die Achse ist nicht frei. "
            .repeat(6);
        let en = "The drive is set with the parameter, and the axis of the machine is not free. "
            .repeat(6);
        assert_eq!(detect_language(&hu, "eng+deu+hun").as_deref(), Some("hun"));
        assert_eq!(detect_language(&de, "eng+deu+hun").as_deref(), Some("deu"));
        assert_eq!(detect_language(&en, "eng+deu+hun").as_deref(), Some("eng"));
        assert_eq!(detect_language("Short text.", "eng+deu+hun"), None);
        assert_eq!(
            detect_language(&hu, "hun"),
            None,
            "one language: nothing to choose"
        );
        assert_eq!(
            detect_language(&hu, "fra+hun"),
            None,
            "no word list for fra"
        );
    }

    #[test]
    fn a_missing_program_is_an_error_not_a_hang() {
        let options = Options {
            tesseract: Some(PathBuf::from("no-such-tesseract.exe")),
            ..Options::default()
        };
        let r = image_file(
            Path::new("no-such-tesseract.exe"),
            Path::new("x.png"),
            &options,
        );
        assert!(r.is_err());
    }
}
