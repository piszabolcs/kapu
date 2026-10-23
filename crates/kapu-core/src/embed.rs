//! Static vector model (FR-10, D28, D29, D38, D46): `Model2Vec` inference and the
//! reader's vector cache.
//!
//! A model directory holds `model.safetensors` (one `embeddings` tensor,
//! `vocab × dim`, f32 or i8 with a per-row `scales` tensor), `tokenizer.json`
//! and `config.json`. Encoding follows `model2vec` 0.10 (`StaticModel.encode`):
//! cut the text to `512 × median token length` characters, tokenize without
//! special tokens and at most 512 tokens, drop the unknown token, average the
//! rows, L2-normalise. The average is a plain sequential sum in f64 (D46): no
//! SIMD, no parallel reduction, the same bits on every platform.

use std::io::Read;
use std::path::{Path, PathBuf};

use tokenizers::{Tokenizer, TruncationParams};

use crate::error::{Error, ErrorCode, Result};
use crate::header::Hash;

/// Token limit of one encoded text (`model2vec` default).
pub const MAX_TOKENS: usize = 512;

fn model_err(detail: impl Into<String>) -> Error {
    Error::new(ErrorCode::E012, format!("vector model: {}", detail.into()))
}

/// A loaded static embedding model.
pub struct Model {
    name: String,
    hash: Hash,
    dim: usize,
    rows: usize,
    /// `rows × dim`, row-major.
    embeddings: Matrix,
    tokenizer: Tokenizer,
    unk: Option<u32>,
    max_chars: usize,
    normalize: bool,
}

impl std::fmt::Debug for Model {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Model")
            .field("name", &self.name)
            .field("dim", &self.dim)
            .field("rows", &self.rows)
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
    let header_len = bytes
        .get(..8)
        .and_then(|b| <[u8; 8]>::try_from(b).ok())
        .map(u64::from_le_bytes)
        .and_then(|n| usize::try_from(n).ok())
        .ok_or_else(|| model_err("safetensors header length missing"))?;
    let header_end = header_len
        .checked_add(8)
        .ok_or_else(|| model_err("bad header length"))?;
    let header: serde_json::Value = bytes
        .get(8..header_end)
        .and_then(|h| serde_json::from_slice(h).ok())
        .ok_or_else(|| model_err("safetensors header is not JSON"))?;
    let tensor = |name: &str| -> Result<(String, Vec<usize>, &[u8])> {
        let t = header
            .get(name)
            .ok_or_else(|| model_err(format!("no `{name}` tensor")))?;
        let dtype = t
            .get("dtype")
            .and_then(|d| d.as_str())
            .unwrap_or("")
            .to_owned();
        let shape: Vec<usize> = t
            .get("shape")
            .and_then(|s| s.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_u64().and_then(|v| usize::try_from(v).ok()))
                    .collect()
            })
            .unwrap_or_default();
        let offsets: Vec<usize> = t
            .get("data_offsets")
            .and_then(|s| s.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_u64().and_then(|v| usize::try_from(v).ok()))
                    .collect()
            })
            .unwrap_or_default();
        let (Some(&start), Some(&end)) = (offsets.first(), offsets.get(1)) else {
            return Err(model_err(format!("`{name}` has no data offsets")));
        };
        let data = bytes
            .get(header_end.saturating_add(start)..header_end.saturating_add(end))
            .ok_or_else(|| model_err(format!("`{name}` data out of range")))?;
        Ok((dtype, shape, data))
    };
    let (dtype, shape, data) = tensor("embeddings")?;
    let [rows, dim] = shape.as_slice() else {
        return Err(model_err("`embeddings` is not two-dimensional"));
    };
    let (rows, dim) = (*rows, *dim);
    let count = rows
        .checked_mul(dim)
        .ok_or_else(|| model_err("tensor too large"))?;
    let values = match dtype.as_str() {
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
        let (embeddings, rows, dim) = read_embeddings(&dir.join("model.safetensors"))?;
        let mut tokenizer = Tokenizer::from_file(dir.join("tokenizer.json"))
            .map_err(|e| model_err(format!("tokenizer.json: {e}")))?;
        tokenizer
            .with_truncation(Some(TruncationParams {
                max_length: MAX_TOKENS,
                ..TruncationParams::default()
            }))
            .map_err(|e| model_err(format!("truncation: {e}")))?;
        let _ = tokenizer.with_padding(None);
        let config: serde_json::Value = std::fs::read(dir.join("config.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or(serde_json::Value::Null);
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
            rows,
            embeddings,
            tokenizer,
            unk,
            max_chars: MAX_TOKENS.saturating_mul(median.max(1)),
            normalize,
        })
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

    /// Token ids of a text, as `model2vec` would use them.
    ///
    /// # Errors
    /// A tokenizer failure (E012).
    pub fn token_ids(&self, text: &str) -> Result<Vec<u32>> {
        let cut: String = text.chars().take(self.max_chars).collect();
        let encoding = self
            .tokenizer
            .encode_fast(cut, false)
            .map_err(|e| model_err(format!("tokenize: {e}")))?;
        Ok(encoding
            .get_ids()
            .iter()
            .copied()
            .filter(|id| Some(*id) != self.unk)
            .collect())
    }

    /// The vector of a text: mean of the token rows, L2-normalised (zero
    /// vector for a text without known tokens).
    ///
    /// # Errors
    /// A tokenizer failure (E012).
    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let ids = self.token_ids(text)?;
        let mut sum = vec![0f64; self.dim];
        let mut used = 0u32;
        for id in ids {
            let Ok(row) = usize::try_from(id) else {
                continue;
            };
            if row >= self.rows {
                continue;
            }
            if self.embeddings.add_row(row, self.dim, &mut sum) {
                used += 1;
            }
        }
        if used == 0 {
            return Ok(vec![0f32; self.dim]);
        }
        let n = f64::from(used);
        let mean: Vec<f64> = sum.iter().map(|s| s / n).collect();
        #[allow(clippy::cast_possible_truncation)]
        let out = if self.normalize {
            let norm = mean.iter().map(|x| x * x).sum::<f64>().sqrt() + 1e-32;
            mean.iter().map(|x| (x / norm) as f32).collect()
        } else {
            mean.iter().map(|x| *x as f32).collect()
        };
        Ok(out)
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

/// The packaged model's name: the i8 form of `potion-multilingual-128M`
/// (D102; a distilled model may replace it after the M8 comparison, D28).
pub const DEFAULT_MODEL: &str = "potion-multilingual-128M-i8";
/// Used when the packaged i8 model is not there (D102).
pub const FALLBACK_MODEL: &str = "potion-multilingual-128M";

/// The default model directory: [`DEFAULT_MODEL`], else [`FALLBACK_MODEL`].
#[must_use]
pub fn find_default_model_dir() -> Option<PathBuf> {
    find_model_dir(DEFAULT_MODEL).or_else(|| find_model_dir(FALLBACK_MODEL))
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

    /// All door vectors of one file body, in `#DOCS` order (D97). The cache
    /// file is `<cache>/<embhash>/<bodyhash>.vecs`: `count` and `dim` as
    /// little-endian u32, then `count × dim` f32. A missing or malformed file
    /// is recomputed with `texts` and written atomically; write failures are
    /// silent (the cache can be deleted any time).
    ///
    /// # Errors
    /// A tokenizer failure (E012), or an error of `texts`.
    pub fn door_vectors(
        &self,
        bodyhash: &Hash,
        count: usize,
        texts: impl FnOnce() -> Result<Vec<String>>,
    ) -> Result<Vec<Vec<f32>>> {
        let dim = self.model.dim();
        let path = self
            .dir
            .as_ref()
            .map(|d| d.join(format!("{}.vecs", hex(bodyhash))));
        if let Some(bytes) = path.as_ref().and_then(|p| std::fs::read(p).ok())
            && let Some(vectors) = parse_bundle(&bytes, count, dim)
        {
            return Ok(vectors);
        }
        let vectors: Vec<Vec<f32>> = texts()?
            .iter()
            .map(|t| self.model.embed(t))
            .collect::<Result<_>>()?;
        if let (Some(dir), Some(path)) = (&self.dir, &path)
            && std::fs::create_dir_all(dir).is_ok()
        {
            let mut bytes = Vec::with_capacity(8 + vectors.len() * dim * 4);
            bytes.extend_from_slice(
                &u32::try_from(vectors.len())
                    .unwrap_or(u32::MAX)
                    .to_le_bytes(),
            );
            bytes.extend_from_slice(&u32::try_from(dim).unwrap_or(u32::MAX).to_le_bytes());
            for v in &vectors {
                for x in v {
                    bytes.extend_from_slice(&x.to_le_bytes());
                }
            }
            let _ = crate::build::write_atomic(path, &bytes);
        }
        Ok(vectors)
    }
}

/// Reads a `.vecs` bundle if its header and size match.
fn parse_bundle(bytes: &[u8], count: usize, dim: usize) -> Option<Vec<Vec<f32>>> {
    let (head, data) = bytes.split_at_checked(8)?;
    let (n, d) = head.split_at_checked(4)?;
    let n = usize::try_from(u32::from_le_bytes(n.try_into().ok()?)).ok()?;
    let d = usize::try_from(u32::from_le_bytes(d.try_into().ok()?)).ok()?;
    if n != count || d != dim || data.len() != n.checked_mul(d)?.checked_mul(4)? {
        return None;
    }
    let values = f32_le(data);
    Some(values.chunks(dim.max(1)).map(<[f32]>::to_vec).collect())
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
    fn cosine_basics() {
        assert!((cosine(&[1.0, 0.0], &[1.0, 0.0]) - 1.0).abs() < 1e-12);
        assert!(cosine(&[1.0, 0.0], &[0.0, 1.0]).abs() < 1e-12);
        assert!(cosine(&[0.0, 0.0], &[1.0, 0.0]).abs() < 1e-12);
    }
}
