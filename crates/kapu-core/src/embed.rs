//! Vector models (FR-10, D28, D29, D38, D46): `Model2Vec` inference, the
//! contextual BERT encoder of [`crate::bert`] (plan v3 V2, D171), and the
//! reader's vector cache.
//!
//! A model directory holds `model.safetensors` (one `embeddings` tensor,
//! `vocab × dim`, f32 or i8 with a per-row `scales` tensor), `tokenizer.json`
//! and `config.json`. Encoding follows `model2vec` 0.10 (`StaticModel.encode`):
//! cut the text to `512 × median token length` characters, tokenize without
//! special tokens and at most 512 tokens, drop the unknown token, average the
//! rows, L2-normalise. The average is a plain sequential sum in f64 (D46): no
//! SIMD, no parallel reduction, the same bits on every platform.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tokenizers::{Tokenizer, TruncationParams};

use crate::error::{Error, ErrorCode, Result};
use crate::header::Hash;

/// Token limit of one encoded text (`model2vec` default).
pub const MAX_TOKENS: usize = 512;

fn model_err(detail: impl Into<String>) -> Error {
    Error::new(ErrorCode::E012, format!("vector model: {}", detail.into()))
}

/// A loaded vector model: static (`Model2Vec`) or contextual (BERT, D171).
pub struct Model {
    name: String,
    hash: Hash,
    dim: usize,
    tokenizer: Tokenizer,
    kind: Kind,
    /// `sim` edges: minimum cosine (`sim_threshold` in `config.json`, else
    /// [`crate::build::SIM_THRESHOLD`]).
    sim_threshold: f64,
    /// Bits per component in `#VEC`: 1 for a static model (D143), the
    /// `vec_bits` of `config.json` (4 or 8, else 4) for a contextual one (D171:
    /// 4 bits measured as good as 8 at half the size).
    vec_bits: u8,
    /// The cache root of the window log (D171), if not [`cache_root`].
    log_root: Option<PathBuf>,
}

/// What computes a vector.
enum Kind {
    Static {
        rows: usize,
        /// `rows × dim`, row-major.
        embeddings: Matrix,
        unk: Option<u32>,
        max_chars: usize,
        normalize: bool,
    },
    Bert(Box<crate::bert::Encoder>),
}

impl std::fmt::Debug for Model {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Model")
            .field("name", &self.name)
            .field("dim", &self.dim)
            .field("contextual", &self.is_contextual())
            .finish_non_exhaustive()
    }
}

/// D38: BLAKE3 over the bytes of `model.safetensors` followed by the bytes of
/// `tokenizer.json`, streamed.
///
/// # Errors
/// I/O errors (E012).
pub fn model_hash(dir: &Path) -> Result<Hash> {
    let mut hasher = blake3::Hasher::new();
    for name in ["model.safetensors", "tokenizer.json"] {
        let path = dir.join(name);
        let mut file = std::fs::File::open(&path)
            .map_err(|e| model_err(format!("{}: {e}", path.display())))?;
        let mut buf = vec![0u8; 1 << 20];
        loop {
            let n = file
                .read(&mut buf)
                .map_err(|e| model_err(format!("{}: {e}", path.display())))?;
            if n == 0 {
                break;
            }
            hasher.update(buf.get(..n).unwrap_or(&[]));
        }
    }
    Ok(Hash(*hasher.finalize().as_bytes()))
}

/// The embedding matrix as stored: f32, or i8 with one f32 scale per row
/// (kept as i8 in memory, so the i8 model needs a quarter of the RAM).
enum Matrix {
    F32(Vec<f32>),
    I8 { values: Vec<i8>, scales: Vec<f32> },
}

impl Matrix {
    /// Adds row `row` to `sum` in f64; false if the row does not exist.
    fn add_row(&self, row: usize, dim: usize, sum: &mut [f64]) -> bool {
        let start = row * dim;
        match self {
            Self::F32(values) => values.get(start..start + dim).is_some_and(|r| {
                for (s, v) in sum.iter_mut().zip(r) {
                    *s += f64::from(*v);
                }
                true
            }),
            Self::I8 { values, scales } => {
                match (values.get(start..start + dim), scales.get(row)) {
                    (Some(r), Some(&scale)) => {
                        // the product is formed in f32 first, like dequantizing the row
                        for (s, v) in sum.iter_mut().zip(r) {
                            *s += f64::from(f32::from(*v) * scale);
                        }
                        true
                    }
                    _ => false,
                }
            }
        }
    }
}

/// Little-endian f32 values; trailing bytes that do not make a value are ignored.
fn f32_le(bytes: &[u8]) -> Vec<f32> {
    let (chunks, _) = bytes.as_chunks::<4>();
    chunks.iter().map(|c| f32::from_le_bytes(*c)).collect()
}

/// Reads the `embeddings` tensor of a safetensors file as f32, row-major.
/// F32 is taken as is; I8 is multiplied by the per-row `scales` (f32).
fn read_embeddings(path: &Path) -> Result<(Matrix, usize, usize)> {
    let bytes = std::fs::read(path).map_err(|e| model_err(format!("{}: {e}", path.display())))?;
    let index = crate::bert::safetensors_index(&bytes)?;
    let tensor = |name: &str| -> Result<(&str, &[usize], &[u8])> {
        let (dtype, shape, range) = index
            .get(name)
            .ok_or_else(|| model_err(format!("no `{name}` tensor")))?;
        Ok((dtype, shape, bytes.get(range.clone()).unwrap_or(&[])))
    };
    let (dtype, shape, data) = tensor("embeddings")?;
    let [rows, dim] = shape else {
        return Err(model_err("`embeddings` is not two-dimensional"));
    };
    let (rows, dim) = (*rows, *dim);
    let count = rows
        .checked_mul(dim)
        .ok_or_else(|| model_err("tensor too large"))?;
    let values = match dtype {
        "F32" if data.len() == count * 4 => Matrix::F32(f32_le(data)),
        "I8" if data.len() == count => {
            let (_, scale_shape, scale_data) = tensor("scales")?;
            if scale_shape.first() != Some(&rows) || scale_data.len() != rows * 4 {
                return Err(model_err("`scales` does not match `embeddings`"));
            }
            Matrix::I8 {
                values: data.iter().map(|&b| i8::from_le_bytes([b])).collect(),
                scales: f32_le(scale_data),
            }
        }
        other => {
            return Err(model_err(format!(
                "unsupported embeddings dtype `{other}` or size"
            )));
        }
    };
    Ok((values, rows, dim))
}

impl Model {
    /// Loads a model directory. The model name is the directory name (D39:
    /// at most 64 characters).
    ///
    /// # Errors
    /// Missing or malformed files (E012).
    pub fn load(dir: &Path) -> Result<Self> {
        let name = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .filter(|n| !n.is_empty() && n.chars().count() <= 64 && n != crate::build::NO_MODEL)
            .ok_or_else(|| model_err(format!("bad model directory name: {}", dir.display())))?;
        let mut tokenizer = Tokenizer::from_file(dir.join("tokenizer.json"))
            .map_err(|e| model_err(format!("tokenizer.json: {e}")))?;
        let _ = tokenizer.with_padding(None);
        let config: serde_json::Value = std::fs::read(dir.join("config.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or(serde_json::Value::Null);
        let sim_threshold = config
            .get("sim_threshold")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(crate::build::SIM_THRESHOLD);
        if config.get("kind").and_then(serde_json::Value::as_str) == Some("bert") {
            // D171: the encoder cuts long texts into windows itself
            tokenizer
                .with_truncation(None)
                .map_err(|e| model_err(format!("truncation: {e}")))?;
            let encoder = crate::bert::Encoder::load(dir, &config)?;
            let vec_bits = match config.get("vec_bits").and_then(serde_json::Value::as_u64) {
                None | Some(4) => 4,
                Some(8) => 8,
                Some(other) => {
                    return Err(model_err(format!(
                        "config.json: `vec_bits` is {other}, a contextual model stores 4 or 8"
                    )));
                }
            };
            return Ok(Self {
                name,
                hash: model_hash(dir)?,
                dim: encoder.dim(),
                tokenizer,
                kind: Kind::Bert(Box::new(encoder)),
                sim_threshold,
                vec_bits,
                log_root: None,
            });
        }
        let (embeddings, rows, dim) = read_embeddings(&dir.join("model.safetensors"))?;
        tokenizer
            .with_truncation(Some(TruncationParams {
                max_length: MAX_TOKENS,
                ..TruncationParams::default()
            }))
            .map_err(|e| model_err(format!("truncation: {e}")))?;
        let normalize = config
            .get("normalize")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true);
        let unk = tokenizer
            .token_to_id("[UNK]")
            .or_else(|| tokenizer.token_to_id("<unk>"));
        // model2vec: median length (in characters) of the vocabulary tokens
        let mut lengths: Vec<usize> = tokenizer
            .get_vocab(false)
            .keys()
            .map(|t| t.chars().count())
            .collect();
        lengths.sort_unstable();
        let median = match lengths.len() {
            0 => 1,
            n if n % 2 == 1 => lengths.get(n / 2).copied().unwrap_or(1),
            n => usize::midpoint(
                lengths.get(n / 2 - 1).copied().unwrap_or(1),
                lengths.get(n / 2).copied().unwrap_or(1),
            ),
        };
        Ok(Self {
            name,
            hash: model_hash(dir)?,
            dim,
            tokenizer,
            kind: Kind::Static {
                rows,
                embeddings,
                unk,
                max_chars: MAX_TOKENS.saturating_mul(median.max(1)),
                normalize,
            },
            sim_threshold,
            vec_bits: 1,
            log_root: None,
        })
    }

    /// Keeps the window log of a contextual model (D171) under `root`
    /// instead of the user cache folder (tests, portable setups).
    pub fn set_log_root(&mut self, root: &Path) {
        self.log_root = Some(root.to_path_buf());
    }

    /// Bits per component of this model's windows in `#VEC`: 1 (sign bits,
    /// D143) for a static model, 4 or 8 for a contextual one (D171).
    #[must_use]
    pub fn vec_bits(&self) -> u8 {
        self.vec_bits
    }

    /// D171: whether the model is contextual. Its window vectors are stored
    /// whole in `#VEC` (`bits=8`), since the reader cannot compute them on
    /// the fly, and a room with sub-doors has windows of its own text only.
    #[must_use]
    pub fn is_contextual(&self) -> bool {
        matches!(self.kind, Kind::Bert(_))
    }

    /// `sim` edges: the minimum cosine of two room vectors.
    #[must_use]
    pub fn sim_threshold(&self) -> f64 {
        self.sim_threshold
    }

    /// Model name written to `emb=`.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Fingerprint written to `embhash=` (D38).
    #[must_use]
    pub fn hash(&self) -> Hash {
        self.hash
    }

    /// Vector dimension.
    #[must_use]
    pub fn dim(&self) -> usize {
        self.dim
    }

    /// Token ids of a text, as `model2vec` would use them; for a contextual
    /// model with the special tokens, at most its token limit.
    ///
    /// # Errors
    /// A tokenizer failure (E012).
    pub fn token_ids(&self, text: &str) -> Result<Vec<u32>> {
        let (max_chars, unk) = match &self.kind {
            Kind::Static { max_chars, unk, .. } => (*max_chars, *unk),
            Kind::Bert(encoder) => return encoder.token_ids(&self.tokenizer, text),
        };
        let cut: String = text.chars().take(max_chars).collect();
        let encoding = self
            .tokenizer
            .encode_fast(cut, false)
            .map_err(|e| model_err(format!("tokenize: {e}")))?;
        Ok(encoding
            .get_ids()
            .iter()
            .copied()
            .filter(|id| Some(*id) != unk)
            .collect())
    }

    /// The vector of a text: for a static model the mean of the token rows,
    /// L2-normalised (zero vector for a text without known tokens); for a
    /// contextual one the text as one passage window.
    ///
    /// # Errors
    /// A tokenizer failure (E012).
    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let (rows, embeddings, normalize) = match &self.kind {
            Kind::Static {
                rows,
                embeddings,
                normalize,
                ..
            } => (*rows, embeddings, *normalize),
            Kind::Bert(encoder) => {
                return encoder.encode(
                    &self.tokenizer,
                    &format!("{}{text}", encoder.passage_prefix),
                );
            }
        };
        let ids = self.token_ids(text)?;
        let mut sum = vec![0f64; self.dim];
        let mut used = 0u32;
        for id in ids {
            let Ok(row) = usize::try_from(id) else {
                continue;
            };
            if row >= rows {
                continue;
            }
            if embeddings.add_row(row, self.dim, &mut sum) {
                used += 1;
            }
        }
        if used == 0 {
            return Ok(vec![0f32; self.dim]);
        }
        let n = f64::from(used);
        let mean: Vec<f64> = sum.iter().map(|s| s / n).collect();
        #[allow(clippy::cast_possible_truncation)]
        let out = if normalize {
            let norm = mean.iter().map(|x| x * x).sum::<f64>().sqrt() + 1e-32;
            mean.iter().map(|x| (x / norm) as f32).collect()
        } else {
            mean.iter().map(|x| *x as f32).collect()
        };
        Ok(out)
    }

    /// The vector of a search query: [`Model::embed`], for a contextual
    /// model with its query prefix (D171).
    ///
    /// # Errors
    /// A tokenizer failure (E012).
    pub fn embed_query(&self, query: &str) -> Result<Vec<f32>> {
        match &self.kind {
            Kind::Bert(encoder) => {
                encoder.encode(&self.tokenizer, &format!("{}{query}", encoder.query_prefix))
            }
            Kind::Static { .. } => self.embed(query),
        }
    }
}

impl Model {
    /// FR-10/5 (D141): one vector per window of `text` (see [`windows`]),
    /// each window at most the model's character limit. The first window is
    /// what [`Model::embed`] sees.
    ///
    /// # Errors
    /// A tokenizer failure (E012).
    pub fn embed_windows(&self, text: &str) -> Result<Vec<Vec<f32>>> {
        match &self.kind {
            Kind::Static { max_chars, .. } => windows(text, *max_chars)
                .into_iter()
                .map(|w| self.embed(w))
                .collect(),
            Kind::Bert(encoder) => encoder
                .passage_windows(&self.tokenizer, text)?
                .iter()
                .map(|w| encoder.encode(&self.tokenizer, w))
                .collect(),
        }
    }
}

/// Progress of a long vector computation (D171), called from any thread:
/// windows done (taken from the window log or computed) and all windows.
pub type Progress<'a> = &'a (dyn Fn(usize, usize) + Sync);

impl Model {
    /// D171: the window vectors of many texts, in order, and how many windows
    /// failed. A static model computes them one after the other. A contextual
    /// model cuts every text into windows on every core, takes the windows it
    /// computed before from the window log ([`WindowLog`]), computes the
    /// others on every core, the longest first, and appends each to the log as
    /// soon as it is done, so an interrupted build goes on where it stopped. A
    /// text the tokenizer refuses has no windows, and a window it refuses is
    /// all zeros (left out of `#VEC`); both count as failed, and the others
    /// go on.
    ///
    /// # Errors
    /// A tokenizer failure of a static model (E012).
    pub fn embed_windows_many(
        &self,
        texts: &[String],
        progress: Option<Progress<'_>>,
    ) -> Result<(Vec<Vec<Vec<f32>>>, usize)> {
        let Kind::Bert(encoder) = &self.kind else {
            let all = texts
                .iter()
                .map(|t| self.embed_windows(t))
                .collect::<Result<_>>()?;
            return Ok((all, 0));
        };
        let cut: Vec<Option<Vec<String>>> =
            parallel_map(texts, |t| encoder.passage_windows(&self.tokenizer, t).ok())
                .into_iter()
                .map(Option::flatten)
                .collect();
        let mut failed = cut.iter().filter(|c| c.is_none()).count();
        let flat: Vec<&str> = cut.iter().flatten().flatten().map(String::as_str).collect();
        let keys: Vec<Hash> = flat
            .iter()
            .map(|w| crate::docs::blake3_of(w.as_bytes()))
            .collect();
        let log = WindowLog::open(self);
        let mut vectors: Vec<Option<Vec<f32>>> = keys.iter().map(|k| log.get(k)).collect();
        let mut todo: Vec<usize> = (0..flat.len())
            .filter(|i| vectors.get(*i).is_some_and(Option::is_none))
            .collect();
        // the longest first, so the last windows to finish are short
        todo.sort_by_key(|i| std::cmp::Reverse(flat.get(*i).map_or(0, |w| w.len())));
        let total = flat.len();
        let done = std::sync::atomic::AtomicUsize::new(total - todo.len());
        if let Some(report) = progress {
            report(total - todo.len(), total);
        }
        let computed = parallel_map(&todo, |i| {
            let window = flat.get(*i).copied().unwrap_or("");
            let vector = encoder.encode(&self.tokenizer, window).ok();
            if let (Some(v), Some(key)) = (&vector, keys.get(*i)) {
                log.append(key, v);
            }
            let now = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            if let Some(report) = progress {
                report(now, total);
            }
            vector
        });
        for (i, vector) in todo.iter().zip(computed) {
            if let Some(slot) = vectors.get_mut(*i) {
                *slot = Some(vector.flatten().unwrap_or_else(|| {
                    failed += 1;
                    vec![0.0; self.dim]
                }));
            }
        }
        let mut vectors = vectors.into_iter().map(Option::unwrap_or_default);
        let all = cut
            .iter()
            .map(|c| {
                vectors
                    .by_ref()
                    .take(c.as_ref().map_or(0, Vec::len))
                    .collect()
            })
            .collect();
        Ok((all, failed))
    }
}

/// `f` of every item on every core (D171), the results in item order. The
/// items are handed out one at a time, so long and short ones mix. A result
/// is `None` only if its thread stopped (it cannot without a panic).
fn parallel_map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<Option<R>> {
    let threads = std::thread::available_parallelism()
        .map_or(1, std::num::NonZero::get)
        .min(items.len());
    if threads <= 1 {
        return items.iter().map(|x| Some(f(x))).collect();
    }
    let next = std::sync::atomic::AtomicUsize::new(0);
    let parts: Vec<Vec<(usize, R)>> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut out = Vec::new();
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(item) = items.get(i) else {
                            return out;
                        };
                        out.push((i, f(item)));
                    }
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|w| w.join().unwrap_or_default())
            .collect()
    });
    let mut out: Vec<Option<R>> = std::iter::repeat_with(|| None).take(items.len()).collect();
    for (i, r) in parts.into_iter().flatten() {
        if let Some(slot) = out.get_mut(i) {
            *slot = Some(r);
        }
    }
    out
}

/// D171: the windows a contextual model has computed, in
/// `<cache>/<embhash>/windows.bin`. A record is the BLAKE3 of the window text
/// (32 bytes), the `dim` f32 components (little-endian), and the first 8
/// bytes of the BLAKE3 of the two, so a record torn by a crash or mixed by
/// two writers ends the log there (the rest is computed again). The log only
/// grows; it can be deleted any time, and its failures are silent.
struct WindowLog {
    known: BTreeMap<[u8; 32], Vec<f32>>,
    file: Option<Mutex<std::fs::File>>,
}

impl WindowLog {
    fn open(model: &Model) -> Self {
        let dim = model.dim();
        let mut known = BTreeMap::new();
        let file = (|| {
            use std::io::{Seek, Write as _};
            let root = model.log_root.clone().or_else(cache_root)?;
            let dir = root.join(hex(&model.hash()));
            std::fs::create_dir_all(&dir).ok()?;
            let path = dir.join("windows.bin");
            let bytes = std::fs::read(&path).unwrap_or_default();
            let record = 32 + 4 * dim + 8;
            let mut good = 0usize;
            for r in bytes.chunks_exact(record) {
                let (key, rest) = r.split_at(32);
                let (values, check) = rest.split_at(4 * dim);
                if crate::docs::blake3_of(r.get(..32 + 4 * dim)?).0.get(..8) != Some(check) {
                    break;
                }
                known.insert(<[u8; 32]>::try_from(key).ok()?, f32_le(values));
                good += record;
            }
            let mut f = std::fs::OpenOptions::new()
                .create(true)
                .truncate(false)
                .write(true)
                .open(&path)
                .ok()?;
            f.set_len(u64::try_from(good).ok()?).ok()?;
            f.seek(std::io::SeekFrom::End(0)).ok()?;
            f.flush().ok()?;
            Some(Mutex::new(f))
        })();
        Self { known, file }
    }

    fn get(&self, key: &Hash) -> Option<Vec<f32>> {
        self.known.get(&key.0).cloned()
    }

    fn append(&self, key: &Hash, vector: &[f32]) {
        use std::io::Write as _;
        let Some(file) = &self.file else { return };
        let mut record = Vec::with_capacity(32 + 4 * vector.len() + 8);
        record.extend_from_slice(&key.0);
        for x in vector {
            record.extend_from_slice(&x.to_le_bytes());
        }
        let check = crate::docs::blake3_of(&record);
        record.extend_from_slice(check.0.get(..8).unwrap_or(&[]));
        if let Ok(mut f) = file.lock() {
            let _ = f.write_all(&record);
        }
    }
}

/// D141: windows of `text` of at most `max` characters, each starting 7/8 of
/// `max` characters after the previous one, until one reaches the end (one
/// window for a short text).
pub(crate) fn windows(text: &str, max: usize) -> Vec<&str> {
    let starts: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
    let chars = starts.len();
    let max = max.max(1);
    let step = (max - max / 8).max(1);
    let at = |c: usize| starts.get(c).copied().unwrap_or(text.len());
    let mut out = Vec::new();
    let mut first: usize = 0;
    loop {
        let end = first.saturating_add(max).min(chars);
        out.push(text.get(at(first)..at(end)).unwrap_or(""));
        if end >= chars {
            return out;
        }
        first += step;
    }
}

/// FR-10/2: the text a door's vector is computed from: title, description
/// and served content (comments already removed), one per line (D91).
#[must_use]
pub fn door_text(title: &str, description: &str, served: &str) -> String {
    format!("{title}\n{description}\n{served}")
}

/// Cosine similarity of two vectors, summed sequentially in f64 (D46).
#[must_use]
pub fn cosine(a: &[f32], b: &[f32]) -> f64 {
    let (mut dot, mut na, mut nb) = (0f64, 0f64, 0f64);
    for (x, y) in a.iter().zip(b) {
        let (x, y) = (f64::from(*x), f64::from(*y));
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        dot / (na.sqrt() * nb.sqrt())
    }
}

/// Where a model directory is looked for: `KAPU_MODEL_DIR` if set, otherwise
/// `models/<name>` next to the running executable and in its parent
/// directories (portable layout, NFR-06), otherwise `models/<name>` in the
/// current directory.
#[must_use]
pub fn find_model_dir(name: &str) -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("KAPU_MODEL_DIR").map(PathBuf::from) {
        return dir.join("model.safetensors").is_file().then_some(dir);
    }
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        candidates.extend(
            exe.ancestors()
                .skip(1)
                .take(4)
                .map(|p| p.join("models").join(name)),
        );
    }
    candidates.push(PathBuf::from("models").join(name));
    candidates
        .into_iter()
        .find(|d| d.join("model.safetensors").is_file())
}

/// The packaged model's name (D172): `multilingual-e5-small` in its int8
/// form, converted by `tools/onnx_to_kapu.py` (until 0.5.0 the i8 form of
/// `potion-multilingual-128M`, D102).
pub const DEFAULT_MODEL: &str = "multilingual-e5-small-q8";
/// Used when the packaged model is not there: the static model of the
/// releases before 0.5.0, in its i8 and its f32 form (D102).
pub const FALLBACK_MODELS: [&str; 2] = ["potion-multilingual-128M-i8", "potion-multilingual-128M"];

/// The default model directory: [`DEFAULT_MODEL`], else the first of
/// [`FALLBACK_MODELS`] that is there.
#[must_use]
pub fn find_default_model_dir() -> Option<PathBuf> {
    find_model_dir(DEFAULT_MODEL)
        .or_else(|| FALLBACK_MODELS.iter().find_map(|name| find_model_dir(name)))
}

// ---------------------------------------------------------------------------
// Vectors in the file (FR-10/6, D143, D171)
// ---------------------------------------------------------------------------

/// The component widths `#VEC` knows: one sign bit (D143), or a signed
/// integer of 4 or 8 bits (D171).
pub const VEC_BITS: [u8; 3] = [1, 4, 8];

/// The first line of `#VEC`: `dim=<n> bits=<b>` (D143, D171).
#[must_use]
pub fn vec_params(dim: usize, bits: u8) -> String {
    format!("dim={dim} bits={bits}")
}

/// Bytes of one window: `dim` components of `bits` bits, the last byte filled
/// up with zero bits.
const fn window_bytes(dim: usize, bits: u8) -> usize {
    (dim * bits as usize).div_ceil(8)
}

/// Base64 characters of one window without padding (43 for 256 sign bits,
/// 512 for 384 components of 8 bits).
#[must_use]
pub const fn window_chars(dim: usize, bits: u8) -> usize {
    (window_bytes(dim, bits) * 4).div_ceil(3)
}

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Appends `bytes` as unpadded base64.
fn push_base64(bytes: &[u8], line: &mut String) {
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .zip([16u32, 8, 0])
            .fold(0u32, |n, (b, shift)| n | (u32::from(*b) << shift));
        for k in 0..=chunk.len() {
            let six = (n >> (18 - 6 * k)) & 63;
            if let Some(c) = usize::try_from(six).ok().and_then(|i| BASE64.get(i)) {
                line.push(char::from(*c));
            }
        }
    }
}

/// D143: one `#VEC` line, the windows of a door in order. A window is the
/// sign bits of its vector (1 for a component above 0, the first component
/// in the highest bit of the first byte) as unpadded base64. A window whose
/// vector is all zeros (a text without known tokens) is left out, so such a
/// door scores 0 as with the float vectors.
#[must_use]
pub fn sign_line(windows: &[Vec<f32>]) -> String {
    let mut line = String::new();
    for v in windows.iter().filter(|v| v.iter().any(|x| *x != 0.0)) {
        let mut bytes = vec![0u8; v.len().div_ceil(8)];
        for (i, x) in v.iter().enumerate() {
            if let Some(b) = bytes.get_mut(i / 8).filter(|_| *x > 0.0) {
                *b |= 0x80 >> (i % 8);
            }
        }
        push_base64(&bytes, &mut line);
    }
    line
}

/// The largest value of a `bits`-bit component (D171): 7 or 127; -8 and
/// -128 are not used, so a window and its negation have the same scale.
const fn int_levels(bits: u8) -> i16 {
    if bits == 4 { 7 } else { 127 }
}

/// D171: one `#VEC` line of 4- or 8-bit integers, the windows of a door in
/// order. Every component is divided by the largest absolute component of
/// its window, times 7 or 127, rounded half to even (in f64), and written in
/// two's complement; with 4 bits two components share a byte, the first in
/// the high half (the low half of a last odd one is 0). A window whose vector
/// is all zeros is left out, as in [`sign_line`].
#[must_use]
pub fn int_line(windows: &[Vec<f32>], bits: u8) -> String {
    let levels = f64::from(int_levels(bits));
    let mut line = String::new();
    for v in windows.iter().filter(|v| v.iter().any(|x| *x != 0.0)) {
        let top = v.iter().map(|x| f64::from(x.abs())).fold(0.0, f64::max);
        // |x| / top <= 1, so the value fits in -127..=127
        #[allow(clippy::cast_possible_truncation)]
        let q: Vec<i8> = v
            .iter()
            .map(|x| (f64::from(*x) / top * levels).round_ties_even() as i8)
            .collect();
        let bytes: Vec<u8> = if bits == 4 {
            q.chunks(2)
                .map(|pair| {
                    let nibble = |x: Option<&i8>| x.map_or(0, |x| x.to_le_bytes()[0] & 0x0F);
                    (nibble(pair.first()) << 4) | nibble(pair.get(1))
                })
                .collect()
        } else {
            q.iter().map(|x| x.to_le_bytes()[0]).collect()
        };
        push_base64(&bytes, &mut line);
    }
    line
}

/// One `#VEC` line of the given width: [`sign_line`] or [`int_line`].
#[must_use]
pub fn vec_line(windows: &[Vec<f32>], bits: u8) -> String {
    if bits == 1 {
        sign_line(windows)
    } else {
        int_line(windows, bits)
    }
}

/// Unpadded base64 to bytes; `None` for a character outside the alphabet, a
/// dangling single character, or bits set below the last byte (only one
/// spelling of each byte string is accepted).
fn unbase64(text: &[u8], out: &mut Vec<u8>) -> Option<()> {
    let value = |c: u8| -> Option<u32> {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        };
        Some(u32::from(v))
    };
    for chunk in text.chunks(4) {
        let bytes = chunk.len().checked_sub(1).filter(|n| *n > 0)?;
        let mut n = 0u32;
        for (i, c) in chunk.iter().enumerate() {
            n |= value(*c)? << (18 - 6 * i);
        }
        if n & ((1u32 << (24 - 8 * bytes)) - 1) != 0 {
            return None;
        }
        out.extend(n.to_be_bytes().iter().skip(1).take(bytes));
    }
    Some(())
}

/// The largest `dim` a `#VEC` section may give (D151): the scoring table is
/// `dim / 8 * 256` sums, so a hostile file must not choose its size.
pub const MAX_VEC_DIM: usize = 4096;

/// The window vectors of a file's `#VEC` section (D143, D171): `dim`
/// components of `bits` bits per window, door `i` owning windows
/// `starts[i]..starts[i + 1]`. Sign bits stay packed; integer components are
/// kept as i16 with the length of each window.
#[derive(Debug, Clone, PartialEq)]
pub struct FileVectors {
    dim: usize,
    bits: u8,
    bytes: usize,
    data: Vec<u8>,
    values: Vec<i16>,
    norms: Vec<f64>,
    starts: Vec<usize>,
}

impl FileVectors {
    /// Reads the inner text of `#VEC` for a file of `doors` doors: the line
    /// `dim=<n> bits=<b>` (`b` one of [`VEC_BITS`]), then one line per door in
    /// `#DOCS` order, each a run of [`window_chars`] base64 characters per
    /// window.
    ///
    /// # Errors
    /// E005 for another first line, another number of door lines, a line that
    /// is not whole windows of canonical base64, or a set unused half byte.
    pub fn parse(text: &str, doors: usize) -> Result<Self> {
        let bad = |what: String| Error::new(ErrorCode::E005, format!("#VEC {what}"));
        let mut lines = text.lines();
        let params = lines.next().unwrap_or("");
        let parsed = params
            .strip_prefix("dim=")
            .and_then(|r| r.split_once(" bits="));
        let dim = parsed
            .and_then(|(d, _)| crate::header::parse_decimal(d))
            .and_then(|d| usize::try_from(d).ok())
            .filter(|d| (1..=MAX_VEC_DIM).contains(d));
        let bits = parsed
            .and_then(|(_, b)| b.parse::<u8>().ok())
            .filter(|b| VEC_BITS.contains(b) && parsed.is_some_and(|(_, t)| t == b.to_string()));
        let (Some(dim), Some(bits)) = (dim, bits) else {
            return Err(bad(format!(
                "line 1: `{params}`, expected `dim=<n> bits=<b>` with n from 1 to {MAX_VEC_DIM} and b 1, 4 or 8"
            )));
        };
        let (bytes, chars) = (window_bytes(dim, bits), window_chars(dim, bits));
        let mut data = Vec::new();
        let mut starts = vec![0usize];
        for (n, line) in lines.enumerate() {
            if line.len() % chars != 0 {
                return Err(bad(format!(
                    "line {}: {} characters, not whole windows of {chars}",
                    n + 2,
                    line.len()
                )));
            }
            for window in line.as_bytes().chunks(chars) {
                unbase64(window, &mut data)
                    .ok_or_else(|| bad(format!("line {}: not canonical base64", n + 2)))?;
                if bits == 4 && dim % 2 == 1 && data.last().is_some_and(|b| b & 0x0F != 0) {
                    return Err(bad(format!(
                        "line {}: the unused half byte is not 0",
                        n + 2
                    )));
                }
            }
            starts.push(data.len() / bytes);
        }
        if starts.len() != doors + 1 {
            return Err(bad(format!(
                "has {} door lines, #DOCS has {doors} doors",
                starts.len() - 1
            )));
        }
        let mut out = Self {
            dim,
            bits,
            bytes,
            data,
            values: Vec::new(),
            norms: Vec::new(),
            starts,
        };
        if bits != 1 {
            out.decode_integers();
        }
        Ok(out)
    }

    /// D171: the integer components of every window and their lengths; the
    /// packed bytes are not needed after that.
    fn decode_integers(&mut self) {
        let dim = self.dim;
        let windows = self.data.len() / self.bytes.max(1);
        let mut values = Vec::with_capacity(windows * dim);
        for w in self.data.chunks(self.bytes.max(1)) {
            if self.bits == 4 {
                let nibbles = w.iter().flat_map(|b| {
                    let b = i8::from_le_bytes([*b]);
                    // the high half by an arithmetic shift, the low half moved up first
                    [b >> 4, (b << 4) >> 4]
                });
                values.extend(nibbles.take(dim).map(i16::from));
            } else {
                values.extend(w.iter().map(|b| i16::from(i8::from_le_bytes([*b]))));
            }
        }
        self.norms = values
            .chunks(dim)
            .map(|w| {
                w.iter()
                    .map(|x| f64::from(*x) * f64::from(*x))
                    .sum::<f64>()
                    .sqrt()
            })
            .collect();
        self.values = values;
        self.data = Vec::new();
    }

    /// Number of components per window.
    #[must_use]
    pub fn dim(&self) -> usize {
        self.dim
    }

    /// Bits per component: 1 (sign bits, D143), 4 or 8 (D171).
    #[must_use]
    pub fn bits(&self) -> u8 {
        self.bits
    }

    /// Number of doors.
    #[must_use]
    pub fn doors(&self) -> usize {
        self.starts.len().saturating_sub(1)
    }

    /// Per door, the best cosine of `query` with one of its windows (0 for a
    /// door without windows). Sign bits are read as ±1 per component: a table
    /// of 256 partial sums per byte position makes a window `dim / 8`
    /// lookups, every sum in f64 in component order (D46). Integer windows
    /// (D171) are scored against the query rounded to 16 bits (`32767 /` its
    /// largest component): the dot product is exact in integers, so its order
    /// does not matter, and the cosine divides it by both lengths in f64.
    #[must_use]
    pub fn best_scores(&self, query: &[f32]) -> Vec<f64> {
        if self.bits == 1 {
            return self.sign_scores(query);
        }
        let top = query.iter().map(|x| f64::from(x.abs())).fold(0.0, f64::max);
        if top == 0.0 {
            return vec![0.0; self.doors()];
        }
        // |x| / top <= 1, so the value fits in i16
        #[allow(clippy::cast_possible_truncation)]
        let q: Vec<i16> = query
            .iter()
            .map(|x| (f64::from(*x) / top * 32767.0).round_ties_even() as i16)
            .collect();
        let q_norm = q
            .iter()
            .map(|x| f64::from(*x) * f64::from(*x))
            .sum::<f64>()
            .sqrt();
        let dim = self.dim.max(1);
        self.starts
            .windows(2)
            .map(|pair| {
                let (Some(&first), Some(&end)) = (pair.first(), pair.get(1)) else {
                    return 0.0;
                };
                let windows = self.values.get(first * dim..end * dim).unwrap_or(&[]);
                let norms = self.norms.get(first..end).unwrap_or(&[]);
                windows
                    .chunks(dim)
                    .zip(norms)
                    .map(|(w, norm)| {
                        // 128 components of at most 127 * 32767 stay below 2^31
                        let dot: i64 = w
                            .chunks(128)
                            .zip(q.chunks(128))
                            .map(|(a, b)| {
                                i64::from(
                                    a.iter()
                                        .zip(b)
                                        .map(|(x, y)| i32::from(*x) * i32::from(*y))
                                        .sum::<i32>(),
                                )
                            })
                            .sum();
                        let denom = norm * q_norm;
                        // exact below 2^53
                        #[allow(clippy::cast_precision_loss)]
                        let dot = dot as f64;
                        if denom == 0.0 { 0.0 } else { dot / denom }
                    })
                    .fold(None, |best: Option<f64>, c| {
                        Some(best.map_or(c, |b| b.max(c)))
                    })
                    .unwrap_or(0.0)
            })
            .collect()
    }

    /// [`FileVectors::best_scores`] of sign bits.
    fn sign_scores(&self, query: &[f32]) -> Vec<f64> {
        let norm = query
            .iter()
            .map(|x| f64::from(*x) * f64::from(*x))
            .sum::<f64>()
            .sqrt();
        #[allow(clippy::cast_precision_loss)]
        let scale = norm * (self.dim as f64).sqrt();
        let mut table = vec![0f64; self.bytes * 256];
        for (j, row) in table.chunks_mut(256).enumerate() {
            for (value, slot) in row.iter_mut().enumerate() {
                let mut sum = 0f64;
                for (k, x) in query.iter().skip(j * 8).take(8).enumerate() {
                    if value & (0x80 >> k) == 0 {
                        sum -= f64::from(*x);
                    } else {
                        sum += f64::from(*x);
                    }
                }
                *slot = sum;
            }
        }
        self.starts
            .windows(2)
            .map(|pair| {
                let (first, end) = (pair.first().copied(), pair.get(1).copied());
                let (Some(first), Some(end)) = (first, end) else {
                    return 0.0;
                };
                let windows = self
                    .data
                    .get(first * self.bytes..end * self.bytes)
                    .unwrap_or(&[]);
                windows
                    .chunks(self.bytes.max(1))
                    .map(|w| {
                        let dot: f64 = w
                            .iter()
                            .zip(table.chunks(256))
                            .map(|(b, row)| row.get(usize::from(*b)).copied().unwrap_or(0.0))
                            .sum();
                        if scale == 0.0 { 0.0 } else { dot / scale }
                    })
                    .fold(None, |best: Option<f64>, c| {
                        Some(best.map_or(c, |b| b.max(c)))
                    })
                    .unwrap_or(0.0)
            })
            .collect()
    }
}

/// How many files' `#VEC` sections stay decoded in memory (as [`MEMO_BODIES`]).
const MEMO_FILES: usize = 2;

/// Decoded `#VEC` sections by the BLAKE3 of their text, the most recent last.
static SIGN_MEMO: Mutex<Vec<(Hash, Arc<FileVectors>)>> = Mutex::new(Vec::new());

/// [`FileVectors::parse`], decoded once per section text for the process (as
/// the cache's memo, D137).
///
/// # Errors
/// As [`FileVectors::parse`].
pub fn file_vectors(text: &str, doors: usize) -> Result<Arc<FileVectors>> {
    let key = crate::docs::blake3_of(text.as_bytes());
    if let Ok(memo) = SIGN_MEMO.lock()
        && let Some((_, hit)) = memo.iter().find(|(k, v)| *k == key && v.doors() == doors)
    {
        return Ok(Arc::clone(hit));
    }
    let vectors = Arc::new(FileVectors::parse(text, doors)?);
    if let Ok(mut memo) = SIGN_MEMO.lock() {
        memo.retain(|(k, _)| *k != key);
        memo.push((key, Arc::clone(&vectors)));
        if memo.len() > MEMO_FILES {
            memo.remove(0);
        }
    }
    Ok(vectors)
}

/// How many doors' rescoring windows stay in memory for the process (D143).
const RESCORE_MEMO_DOORS: usize = 20_000;
/// D157 (plan v2): and at most this many windows in all, about 260 MB of
/// 256-component vectors. The sign bits favour long doors, whose windows
/// are many: bounded by doors alone, the memo of an MDN-sized file grew past
/// 20 GB.
const RESCORE_MEMO_WINDOWS: usize = 250_000;

/// What a door's rescoring windows depend on: the model, the door's stored
/// bytes (its text follows from them) and whether it is a sub-door.
type RescoreKey = ([u8; 32], [u8; 32], bool);

/// Rescoring windows by key, the keys in the order they came, and the
/// number of windows held.
type RescoreMemo = (
    BTreeMap<RescoreKey, Arc<Vec<Vec<f32>>>>,
    std::collections::VecDeque<RescoreKey>,
    usize,
);

static RESCORE_MEMO: Mutex<RescoreMemo> =
    Mutex::new((BTreeMap::new(), std::collections::VecDeque::new(), 0));

/// D143: the float window vectors of `entries` for rescoring the candidates
/// of the sign bits: from the process memo, or computed from `texts` (the
/// vector texts of `entries`, in order), which runs only if a door is missing.
///
/// # Errors
/// A tokenizer failure (E012), or an error of `texts`.
pub fn rescore_windows(
    model: &Model,
    entries: &[crate::DocEntry<'_>],
    texts: impl FnOnce() -> Result<Vec<String>>,
) -> Result<Vec<Arc<Vec<Vec<f32>>>>> {
    let keys: Vec<RescoreKey> = entries
        .iter()
        .map(|e| (model.hash().0, e.hash.0, e.id.contains('/')))
        .collect();
    let mut found: Vec<Option<Arc<Vec<Vec<f32>>>>> = match RESCORE_MEMO.lock() {
        Ok(memo) => keys.iter().map(|k| memo.0.get(k).cloned()).collect(),
        Err(_) => vec![None; keys.len()],
    };
    if found.iter().any(Option::is_none) {
        let texts = texts()?;
        let mut fresh = Vec::new();
        for ((slot, key), text) in found.iter_mut().zip(&keys).zip(&texts) {
            if slot.is_none() {
                let windows = Arc::new(model.embed_windows(text)?);
                fresh.push((*key, Arc::clone(&windows)));
                *slot = Some(windows);
            }
        }
        if let Ok(mut memo) = RESCORE_MEMO.lock() {
            for (key, windows) in fresh {
                let count = windows.len();
                if memo.0.insert(key, windows).is_none() {
                    memo.1.push_back(key);
                    memo.2 += count;
                }
            }
            while memo.1.len() > RESCORE_MEMO_DOORS || memo.2 > RESCORE_MEMO_WINDOWS {
                let Some(old) = memo.1.pop_front() else {
                    break;
                };
                if let Some(gone) = memo.0.remove(&old) {
                    memo.2 = memo.2.saturating_sub(gone.len());
                }
            }
        }
    }
    Ok(found.into_iter().map(Option::unwrap_or_default).collect())
}

// ---------------------------------------------------------------------------
// Vector cache (FR-10/3, D47)
// ---------------------------------------------------------------------------

/// The user cache directory of KAPU: `KAPU_CACHE_DIR` if set, otherwise
/// `%LOCALAPPDATA%\kapu` (Windows), `~/Library/Caches/kapu` (macOS),
/// `$XDG_CACHE_HOME/kapu` or `~/.cache/kapu` (others).
#[must_use]
pub fn cache_root() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("KAPU_CACHE_DIR") {
        return Some(PathBuf::from(dir));
    }
    let base = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library").join("Caches"))
    } else {
        std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
    };
    base.map(|b| b.join("kapu"))
}

/// Vectors of door texts, cached on disk under `<cache>/<embhash>/<b3 of
/// the text>.vec` (little-endian f32). The key is the text's BLAKE3, which
/// changes whenever the door's bytes or captions change. Cache failures are
/// silent: the vector is computed again (the cache can be deleted any time).
pub struct VectorCache<'m> {
    model: &'m Model,
    dir: Option<PathBuf>,
}

impl<'m> VectorCache<'m> {
    /// A cache for `model` in the default cache root (or none if unknown).
    #[must_use]
    pub fn new(model: &'m Model) -> Self {
        let dir = cache_root().map(|r| r.join(hex(&model.hash())));
        Self { model, dir }
    }

    /// The model behind the cache.
    #[must_use]
    pub fn model(&self) -> &'m Model {
        self.model
    }

    /// A cache in a given directory (tests, portable setups).
    #[must_use]
    pub fn in_dir(model: &'m Model, root: &Path) -> Self {
        Self {
            model,
            dir: Some(root.join(hex(&model.hash()))),
        }
    }

    /// A cache that never touches the disk.
    #[must_use]
    pub fn in_memory(model: &'m Model) -> Self {
        Self { model, dir: None }
    }

    /// The vector of one text (not cached on disk).
    ///
    /// # Errors
    /// A tokenizer failure (E012).
    pub fn vector(&self, text: &str) -> Result<Vec<f32>> {
        self.model.embed(text)
    }

    /// The window vectors of every door of one file body, in `#DOCS` order
    /// (D97, D141). The cache file is `<cache>/<embhash>/<bodyhash>.vecs2`:
    /// `count` and `dim` as little-endian u32, the `count` window counts as
    /// u32, then every window's `dim` f32. A missing or malformed file is
    /// recomputed with `texts` and written atomically; write failures are
    /// silent (the cache can be deleted any time). The vectors of the last
    /// [`MEMO_BODIES`] bodies stay in memory for the process (A44, D137).
    ///
    /// # Errors
    /// A tokenizer failure (E012), or an error of `texts`.
    pub fn door_vectors(
        &self,
        bodyhash: &Hash,
        count: usize,
        texts: impl FnOnce() -> Result<Vec<String>>,
    ) -> Result<Vectors> {
        let key = MemoKey {
            model: self.model.hash(),
            body: *bodyhash,
            dir: self.dir.clone(),
        };
        if let Some(hit) = memo_get(&key, count) {
            return Ok(hit);
        }
        let vectors = Arc::new(self.load_or_compute(bodyhash, count, texts)?);
        memo_put(key, Arc::clone(&vectors));
        Ok(vectors)
    }

    fn load_or_compute(
        &self,
        bodyhash: &Hash,
        count: usize,
        texts: impl FnOnce() -> Result<Vec<String>>,
    ) -> Result<Vec<Vec<Vec<f32>>>> {
        let dim = self.model.dim();
        let path = self
            .dir
            .as_ref()
            .map(|d| d.join(format!("{}.vecs2", hex(bodyhash))));
        if let Some(doors) = path
            .as_ref()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| parse_bundle(&b, dim))
            .filter(|d| d.len() == count)
        {
            return Ok(doors.into_iter().map(|(_, w)| w).collect());
        }
        // D141: a door whose text did not change keeps its windows from the
        // newest other bundle, so a write recomputes only the changed doors
        let mut known: BTreeMap<[u8; 32], Vec<Vec<f32>>> = self
            .newest_bundle(path.as_deref())
            .unwrap_or_default()
            .into_iter()
            .map(|(h, w)| (h.0, w))
            .collect();
        let doors: Vec<Door> = texts()?
            .iter()
            .map(|t| {
                let hash = crate::docs::blake3_of(t.as_bytes());
                match known.remove(&hash.0) {
                    Some(windows) => Ok((hash, windows)),
                    None => Ok((hash, self.model.embed_windows(t)?)),
                }
            })
            .collect::<Result<_>>()?;
        if let (Some(dir), Some(path)) = (&self.dir, &path)
            && std::fs::create_dir_all(dir).is_ok()
        {
            let _ = crate::build::write_atomic(path, &bundle_bytes(&doors, dim));
        }
        Ok(doors.into_iter().map(|(_, w)| w).collect())
    }

    /// The most recently written bundle of this model other than `except`.
    fn newest_bundle(&self, except: Option<&Path>) -> Option<Vec<Door>> {
        let newest = std::fs::read_dir(self.dir.as_ref()?)
            .ok()?
            .filter_map(std::result::Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "vecs2") && Some(p.as_path()) != except)
            .max_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok())?;
        parse_bundle(&std::fs::read(newest).ok()?, self.model.dim())
    }
}

/// One door's window vectors and the BLAKE3 of the text they came from.
type Door = (Hash, Vec<Vec<f32>>);

/// A `.vecs2` bundle: `count` and `dim` as little-endian u32; per door the
/// 32-byte BLAKE3 of its text and its window count as u32; then every
/// window's `dim` f32.
fn bundle_bytes(doors: &[Door], dim: usize) -> Vec<u8> {
    let le = |n: usize| u32::try_from(n).unwrap_or(u32::MAX).to_le_bytes();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&le(doors.len()));
    bytes.extend_from_slice(&le(dim));
    for (hash, windows) in doors {
        bytes.extend_from_slice(&hash.0);
        bytes.extend_from_slice(&le(windows.len()));
    }
    for x in doors.iter().flat_map(|(_, w)| w).flatten() {
        bytes.extend_from_slice(&x.to_le_bytes());
    }
    bytes
}

/// How many bodies' door vectors stay in memory (A44, D137).
pub const MEMO_BODIES: usize = 2;

/// What a memo entry belongs to: model, file body and cache directory (a
/// cache directory still gets its bundle written once).
#[derive(PartialEq, Eq)]
struct MemoKey {
    model: Hash,
    body: Hash,
    dir: Option<PathBuf>,
}

/// Window vectors per door, shared between the memo and its callers.
pub type Vectors = Arc<Vec<Vec<Vec<f32>>>>;

/// Memo entries, the most recent last.
static MEMO: Mutex<Vec<(MemoKey, Vectors)>> = Mutex::new(Vec::new());

fn memo_get(key: &MemoKey, count: usize) -> Option<Vectors> {
    let memo = MEMO.lock().ok()?;
    memo.iter()
        .find(|(k, v)| k == key && v.len() == count)
        .map(|(_, v)| Arc::clone(v))
}

fn memo_put(key: MemoKey, vectors: Vectors) {
    if let Ok(mut memo) = MEMO.lock() {
        memo.retain(|(k, _)| k != &key);
        memo.push((key, vectors));
        if memo.len() > MEMO_BODIES {
            memo.remove(0);
        }
    }
}

/// Reads a `.vecs2` bundle (see [`bundle_bytes`]) if its header and size match.
fn parse_bundle(bytes: &[u8], dim: usize) -> Option<Vec<Door>> {
    let u32_at = |b: &[u8]| -> Option<usize> {
        usize::try_from(u32::from_le_bytes(b.try_into().ok()?)).ok()
    };
    let (head, rest) = bytes.split_at_checked(8)?;
    let (n, d) = head.split_at_checked(4)?;
    let (n, d) = (u32_at(n)?, u32_at(d)?);
    if d != dim || dim == 0 {
        return None;
    }
    let (table, data) = rest.split_at_checked(n.checked_mul(36)?)?;
    let table: Vec<(Hash, usize)> = table
        .chunks(36)
        .map(|entry| {
            let (hash, windows) = entry.split_at_checked(32)?;
            Some((Hash(hash.try_into().ok()?), u32_at(windows)?))
        })
        .collect::<Option<_>>()?;
    let windows = table
        .iter()
        .try_fold(0usize, |a, (_, c)| a.checked_add(*c))?;
    if data.len() != windows.checked_mul(dim)?.checked_mul(4)? {
        return None;
    }
    let values = f32_le(data);
    let mut rows = values.chunks(dim).map(<[f32]>::to_vec);
    Some(
        table
            .into_iter()
            .map(|(hash, c)| (hash, rows.by_ref().take(c).collect()))
            .collect(),
    )
}

fn hex(h: &Hash) -> String {
    use std::fmt::Write as _;
    h.0.iter().fold(String::with_capacity(64), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn door_vectors_stay_in_memory_for_the_process() {
        // A44 (D137): the second call of the same body needs no texts and no disk
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/model/kapu-test-tiny");
        let model = Model::load(&dir).unwrap();
        let cache = VectorCache::in_memory(&model);
        let body = Hash([7u8; 32]);
        let first = cache
            .door_vectors(&body, 2, || Ok(vec!["alpha".to_owned(), "beta".to_owned()]))
            .unwrap();
        let second = cache
            .door_vectors(&body, 2, || {
                Err(Error::new(
                    ErrorCode::E012,
                    "texts are not needed".to_owned(),
                ))
            })
            .unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn unchanged_texts_keep_their_windows_from_the_newest_bundle() {
        // D141: only a door whose text changed is embedded again
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/model/kapu-test-tiny");
        let model = Model::load(&dir).unwrap();
        let root = std::env::temp_dir().join(format!("kapu-reuse-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let cache = VectorCache::in_dir(&model, &root);
        let bundles = cache.dir.clone().unwrap();
        std::fs::create_dir_all(&bundles).unwrap();
        let marker = vec![vec![0.5f32; model.dim()]];
        let old = [(crate::docs::blake3_of(b"alpha"), marker.clone())];
        std::fs::write(
            bundles.join(format!("{}.vecs2", hex(&Hash([1u8; 32])))),
            bundle_bytes(&old, model.dim()),
        )
        .unwrap();
        let got = cache
            .door_vectors(&Hash([2u8; 32]), 2, || {
                Ok(vec!["alpha".to_owned(), "beta".to_owned()])
            })
            .unwrap();
        assert_eq!(got[0], marker);
        assert_eq!(got[1], model.embed_windows("beta").unwrap());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn windows_cover_a_long_text_with_an_eighth_of_overlap() {
        // D141: windows of at most `max` characters, each starting 7/8 of `max` later
        assert_eq!(windows("short", 16), vec!["short"]);
        assert_eq!(windows("", 16), vec![""]);
        let text: String = ('a'..='z').cycle().take(40).collect();
        let w = windows(&text, 16);
        assert_eq!(w.len(), 3);
        assert_eq!(w[0], &text[0..16]);
        assert_eq!(w[1], &text[14..30]);
        assert_eq!(w[2], &text[28..40]);
        let accented = "é".repeat(20);
        assert!(windows(&accented, 8).iter().all(|w| w.chars().count() <= 8));
    }

    #[test]
    fn sign_lines_round_trip_and_score_like_plus_minus_one_vectors() {
        // D143: 1 for a component above 0, the first component in the highest bit
        let a: Vec<f32> = (0u16..256)
            .map(|i| if i % 3 == 0 { 0.5 } else { -0.25 })
            .collect();
        let b: Vec<f32> = (0u16..256)
            .map(|i| if i < 128 { 1.0 } else { -1.0 })
            .collect();
        let zero = vec![0f32; 256];
        assert_eq!(window_chars(256, 1), 43);
        let line = sign_line(&[a.clone(), zero.clone(), b.clone()]);
        assert_eq!(line.len(), 2 * 43, "the zero window is left out");
        let text = format!("{}\n{line}\n{}\n", vec_params(256, 1), sign_line(&[zero]));
        let signs = FileVectors::parse(&text, 2).unwrap();
        assert_eq!((signs.doors(), signs.dim()), (2, 256));
        let q: Vec<f32> = (0u16..256).map(|i| f32::from((i * 7) % 11) - 5.0).collect();
        let sign = |v: &[f32]| -> Vec<f32> {
            v.iter()
                .map(|x| if *x > 0.0 { 1.0 } else { -1.0 })
                .collect()
        };
        let want = cosine(&q, &sign(&a)).max(cosine(&q, &sign(&b)));
        let got = signs.best_scores(&q);
        assert!((got[0] - want).abs() < 1e-12, "{got:?} {want}");
        assert!(
            got[1].abs() < f64::EPSILON,
            "a door without windows scores 0"
        );
    }

    #[test]
    fn a_bad_vec_section_is_e005() {
        let ones = sign_line(&[vec![1.0f32; 256]]);
        assert_eq!(ones, format!("{}//8", "/".repeat(40)));
        let ok = format!("dim=256 bits=1\n{ones}\n");
        assert!(FileVectors::parse(&ok, 1).is_ok());
        assert!(FileVectors::parse(&format!("dim={MAX_VEC_DIM} bits=1\n"), 0).is_ok());
        let not_canonical = format!("{}//9", "/".repeat(40));
        let bad_char = format!("*{}", &ones[1..]);
        for (text, doors) in [
            (format!("dim=256 bits=2\n{ones}\n"), 1),
            (format!("dim=256 bits=8\n{ones}\n"), 1),
            (format!("dim=256 bits=08\n{ones}\n"), 1),
            (format!("dim=256 bits=1\n{ones}x\n"), 1),
            (format!("dim=256 bits=1\n{not_canonical}\n"), 1),
            (format!("dim=256 bits=1\n{bad_char}\n"), 1),
            (ok.clone(), 2),
            // D151: found by the `vec` fuzz target, a 236 GB scoring table
            ("dim=922222223 bits=1".to_owned(), 0),
            (format!("dim={} bits=1\n", MAX_VEC_DIM + 1), 0),
        ] {
            let e = FileVectors::parse(&text, doors).unwrap_err();
            assert_eq!(e.code, ErrorCode::E005, "{text:?}");
        }
    }

    #[test]
    #[allow(clippy::cast_possible_truncation)]
    fn integer_lines_round_trip_and_score_like_their_rounded_vectors() {
        // D171: rounded to 127 or 7 steps of the largest component
        let a: Vec<f32> = (0u16..384).map(|i| f32::from(i % 17) - 8.0).collect();
        let b: Vec<f32> = (0u16..384)
            .map(|i| if i < 100 { 0.5 } else { -0.125 })
            .collect();
        let zero = vec![0f32; 384];
        let q: Vec<f32> = (0u16..384).map(|i| f32::from((i * 7) % 11) - 5.0).collect();
        for bits in [8u8, 4] {
            let levels = f64::from(int_levels(bits));
            let rounded = |v: &[f32]| -> Vec<f32> {
                let top = v.iter().map(|x| f64::from(x.abs())).fold(0.0, f64::max);
                v.iter()
                    .map(|x| (f64::from(*x) / top * levels).round_ties_even() as f32)
                    .collect()
            };
            let line = int_line(&[a.clone(), zero.clone(), b.clone()], bits);
            assert_eq!(
                line.len(),
                2 * window_chars(384, bits),
                "the zero window is left out"
            );
            let text = format!("{}\n{line}\n\n", vec_params(384, bits));
            let v = FileVectors::parse(&text, 2).unwrap();
            assert_eq!((v.doors(), v.dim(), v.bits()), (2, 384, bits));
            let want = cosine(&q, &rounded(&a)).max(cosine(&q, &rounded(&b)));
            let got = v.best_scores(&q);
            // the query is rounded to 16 bits
            assert!((got[0] - want).abs() < 1e-4, "{bits}: {got:?} {want}");
            assert!(got[1].abs() < f64::EPSILON);
        }
        assert_eq!(window_chars(384, 8), 512);
        assert_eq!(window_chars(384, 4), 256);
        // an odd count of 4-bit components: the last low half must be 0
        let odd = int_line(&[vec![1.0f32; 3]], 4);
        assert!(FileVectors::parse(&format!("dim=3 bits=4\n{odd}\n"), 1).is_ok());
        let mut bad = String::new();
        push_base64(&[0x77, 0x71], &mut bad);
        let e = FileVectors::parse(&format!("dim=3 bits=4\n{bad}\n"), 1).unwrap_err();
        assert_eq!(e.code, ErrorCode::E005);
    }

    #[test]
    fn cosine_basics() {
        assert!((cosine(&[1.0, 0.0], &[1.0, 0.0]) - 1.0).abs() < 1e-12);
        assert!(cosine(&[1.0, 0.0], &[0.0, 1.0]).abs() < 1e-12);
        assert!(cosine(&[0.0, 0.0], &[1.0, 0.0]).abs() < 1e-12);
    }
}
