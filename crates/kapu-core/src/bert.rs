//! Contextual vector model (plan v3 V2, D171): a BERT sentence encoder in
//! the int8 form of ONNX Runtime's dynamic quantization, in plain Rust.
//!
//! The folder comes from `tools/onnx_to_kapu.py`: `model.safetensors`,
//! `tokenizer.json` and a `config.json` with `"kind": "bert"`. A forward pass
//! follows the ONNX graph it was converted from. Every matrix product is
//! `DynamicQuantizeLinear` of its input (u8, one scale per tensor, round half
//! to even) times i8 weights, or times another quantized tensor, in exact
//! integers. Layer norm, softmax and the erf GELU run in f32, `exp` and `erf`
//! by fixed formulas of `+`, `*` and `/` (correctly rounded on every
//! platform); the mean of the token vectors and its L2 norm run in f64.
//! Integer sums are exact in any order and every float sum runs in index
//! order, so a vector has the same bits on every platform (D46) although the
//! integer products are vectorised. The text of a query starts with the
//! model's query prefix, a passage with its passage prefix (`query: `,
//! `passage: ` for e5).

use std::collections::BTreeMap;
use std::ops::Range;
use std::path::Path;

use tokenizers::Tokenizer;

use crate::error::{Error, ErrorCode, Result};

fn model_err(detail: impl Into<String>) -> Error {
    Error::new(ErrorCode::E012, format!("vector model: {}", detail.into()))
}

/// The tensors of a safetensors file by name: dtype, shape, and the byte
/// range of the data in the file.
pub(crate) type TensorIndex = BTreeMap<String, (String, Vec<usize>, Range<usize>)>;

/// Reads the header of a safetensors file.
///
/// # Errors
/// A missing or malformed header, or data outside the file (E012).
pub(crate) fn safetensors_index(bytes: &[u8]) -> Result<TensorIndex> {
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
    let usizes = |v: Option<&serde_json::Value>| -> Vec<usize> {
        v.and_then(|s| s.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_u64().and_then(|v| usize::try_from(v).ok()))
                    .collect()
            })
            .unwrap_or_default()
    };
    let mut out = TensorIndex::new();
    for (name, t) in header.as_object().into_iter().flatten() {
        if name == "__metadata__" {
            continue;
        }
        let dtype = t
            .get("dtype")
            .and_then(|d| d.as_str())
            .unwrap_or("")
            .to_owned();
        let offsets = usizes(t.get("data_offsets"));
        let (Some(&start), Some(&end)) = (offsets.first(), offsets.get(1)) else {
            return Err(model_err(format!("`{name}` has no data offsets")));
        };
        let range = header_end.saturating_add(start)..header_end.saturating_add(end);
        if range.start > range.end || range.end > bytes.len() {
            return Err(model_err(format!("`{name}` data out of range")));
        }
        out.insert(name.clone(), (dtype, usizes(t.get("shape")), range));
    }
    Ok(out)
}

/// Little-endian f32 values.
fn f32_le(bytes: &[u8]) -> Vec<f32> {
    let (chunks, _) = bytes.as_chunks::<4>();
    chunks.iter().map(|c| f32::from_le_bytes(*c)).collect()
}

/// An embedding table: u8 rows in the model bytes, value `(q - zero) * scale`.
struct Table {
    data: Range<usize>,
    scale: f32,
    zero: i16,
}

/// Layer norm weights.
struct Norm {
    weight: Vec<f32>,
    bias: Vec<f32>,
}

/// A linear map: one row of weights per output, the weight zero point
/// already subtracted (i16), a scale and a bias per output.
struct Linear {
    weights: Vec<i16>,
    inputs: usize,
    scale: Vec<f32>,
    bias: Vec<f32>,
}

struct Layer {
    query: Linear,
    key: Linear,
    value: Linear,
    output: Linear,
    up: Linear,
    down: Linear,
    norm1: Norm,
    norm2: Norm,
}

/// A loaded BERT encoder.
pub(crate) struct Encoder {
    bytes: Vec<u8>,
    hidden: usize,
    heads: usize,
    max_tokens: usize,
    eps: f32,
    word: Table,
    position: Table,
    segment: Table,
    norm: Norm,
    layers: Vec<Layer>,
    normalize: bool,
    pub(crate) query_prefix: String,
    pub(crate) passage_prefix: String,
}

impl Encoder {
    /// Loads `model.safetensors` of `dir` with the sizes of `config`.
    ///
    /// # Errors
    /// A missing size, a missing tensor or one of the wrong type or shape (E012).
    #[allow(clippy::too_many_lines)]
    pub(crate) fn load(dir: &Path, config: &serde_json::Value) -> Result<Self> {
        let num = |key: &str| {
            config
                .get(key)
                .and_then(serde_json::Value::as_u64)
                .and_then(|v| usize::try_from(v).ok())
                .filter(|v| *v > 0)
                .ok_or_else(|| model_err(format!("config.json: no `{key}`")))
        };
        let text = |key: &str| {
            config
                .get(key)
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .to_owned()
        };
        let (hidden, layers, heads) = (num("hidden")?, num("layers")?, num("heads")?);
        let (intermediate, max_tokens) = (num("intermediate")?, num("max_tokens")?);
        if hidden % heads != 0 || max_tokens < 8 {
            return Err(model_err(
                "config.json: `hidden` is not a multiple of `heads`",
            ));
        }
        if text("pooling") != "mean" {
            return Err(model_err(
                "config.json: only `\"pooling\": \"mean\"` is supported",
            ));
        }
        #[allow(clippy::cast_possible_truncation)]
        let eps = config
            .get("layer_norm_eps")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(1e-12) as f32;
        let path = dir.join("model.safetensors");
        let bytes =
            std::fs::read(&path).map_err(|e| model_err(format!("{}: {e}", path.display())))?;
        let index = safetensors_index(&bytes)?;
        let tensor = |name: &str, dtype: &str, shape: &[usize]| -> Result<Range<usize>> {
            let (d, s, range) = index
                .get(name)
                .ok_or_else(|| model_err(format!("no `{name}` tensor")))?;
            let size = match dtype {
                "F32" => 4,
                _ => 1,
            };
            let count = s.iter().try_fold(1usize, |a, n| a.checked_mul(*n));
            if d != dtype
                || s != shape
                || count.and_then(|c| c.checked_mul(size)) != Some(range.len())
            {
                return Err(model_err(format!(
                    "`{name}` is {d} {s:?}, expected {dtype} {shape:?}"
                )));
            }
            Ok(range.clone())
        };
        let floats = |name: &str, n: usize| -> Result<Vec<f32>> {
            let range = tensor(name, "F32", &[n])?;
            Ok(f32_le(bytes.get(range).unwrap_or(&[])))
        };
        let table = |name: &str, rows: Option<usize>| -> Result<Table> {
            let rows = match rows {
                Some(r) => r,
                None => index
                    .get(&format!("{name}.q"))
                    .and_then(|(_, s, _)| s.first().copied())
                    .ok_or_else(|| model_err(format!("no `{name}.q` tensor")))?,
            };
            let data = tensor(&format!("{name}.q"), "U8", &[rows, hidden])?;
            let scale = floats(&format!("{name}.scale"), 1)?;
            let zero = tensor(&format!("{name}.zero"), "U8", &[1])?;
            Ok(Table {
                data,
                scale: scale.first().copied().unwrap_or(0.0),
                zero: bytes.get(zero.start).map_or(0, |z| i16::from(*z)),
            })
        };
        let norm = |name: &str| -> Result<Norm> {
            Ok(Norm {
                weight: floats(&format!("{name}.weight"), hidden)?,
                bias: floats(&format!("{name}.bias"), hidden)?,
            })
        };
        let linear = |name: &str, inputs: usize, outputs: usize| -> Result<Linear> {
            let w = tensor(&format!("{name}.w"), "I8", &[outputs, inputs])?;
            let zero = tensor(&format!("{name}.zero"), "I8", &[outputs])?;
            let zeros = bytes.get(zero).unwrap_or(&[]);
            let rows = bytes.get(w).unwrap_or(&[]);
            let weights = rows
                .chunks(inputs)
                .zip(zeros)
                .flat_map(|(row, z)| {
                    let z = i16::from(i8::from_le_bytes([*z]));
                    row.iter()
                        .map(move |b| i16::from(i8::from_le_bytes([*b])) - z)
                })
                .collect();
            Ok(Linear {
                weights,
                inputs,
                scale: floats(&format!("{name}.scale"), outputs)?,
                bias: floats(&format!("{name}.bias"), outputs)?,
            })
        };
        let mut blocks = Vec::with_capacity(layers);
        for l in 0..layers {
            let p = format!("layer.{l}");
            blocks.push(Layer {
                query: linear(&format!("{p}.q"), hidden, hidden)?,
                key: linear(&format!("{p}.k"), hidden, hidden)?,
                value: linear(&format!("{p}.v"), hidden, hidden)?,
                output: linear(&format!("{p}.o"), hidden, hidden)?,
                up: linear(&format!("{p}.up"), hidden, intermediate)?,
                down: linear(&format!("{p}.down"), intermediate, hidden)?,
                norm1: norm(&format!("{p}.ln1"))?,
                norm2: norm(&format!("{p}.ln2"))?,
            });
        }
        let (word, position, segment) = (
            table("word", None)?,
            table("position", Some(max_tokens))?,
            table("type", None)?,
        );
        let norm = norm("ln")?;
        Ok(Self {
            bytes,
            hidden,
            heads,
            max_tokens,
            eps,
            word,
            position,
            segment,
            norm,
            layers: blocks,
            normalize: config
                .get("normalize")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(true),
            query_prefix: text("query_prefix"),
            passage_prefix: text("passage_prefix"),
        })
    }

    /// Vector dimension.
    pub(crate) fn dim(&self) -> usize {
        self.hidden
    }

    /// The token ids of `text` with the special tokens, at most `max_tokens`:
    /// a longer text keeps its first ids and its last (closing) id, as the
    /// tokenizer's own truncation does.
    ///
    /// # Errors
    /// A tokenizer failure (E012).
    pub(crate) fn token_ids(&self, tokenizer: &Tokenizer, text: &str) -> Result<Vec<u32>> {
        let encoding = tokenizer
            .encode_fast(text, true)
            .map_err(|e| model_err(format!("tokenize: {e}")))?;
        let mut ids = encoding.get_ids().to_vec();
        if ids.len() > self.max_tokens {
            let last = ids.last().copied();
            ids.truncate(self.max_tokens - 1);
            ids.extend(last);
        }
        Ok(ids)
    }

    /// The vector of `text` (prefix included).
    ///
    /// # Errors
    /// A tokenizer failure (E012).
    pub(crate) fn encode(&self, tokenizer: &Tokenizer, text: &str) -> Result<Vec<f32>> {
        Ok(self.forward(&self.token_ids(tokenizer, text)?))
    }

    /// D171: the window texts of a passage, each with the passage prefix and
    /// short enough for one forward pass. The text is tokenized once; a window
    /// is at most `max_tokens` less the special tokens, the prefix and a margin
    /// of 4 of its tokens, and starts 7/8 of that after the previous one (as
    /// D141), until one reaches the end. A window is the text from its first
    /// token's start to its last token's end.
    ///
    /// # Errors
    /// A tokenizer failure (E012).
    pub(crate) fn passage_windows(&self, tokenizer: &Tokenizer, text: &str) -> Result<Vec<String>> {
        let prefix = &self.passage_prefix;
        // `encode`, not `encode_fast`: only it gives the byte offsets
        let tokenize = |t: &str| {
            tokenizer
                .encode(t, false)
                .map_err(|e| model_err(format!("tokenize: {e}")))
        };
        let reserved = 2 + tokenize(prefix)?.get_ids().len() + 4;
        let size = self.max_tokens.saturating_sub(reserved).max(8);
        let encoding = tokenize(text)?;
        let offsets = encoding.get_offsets();
        if offsets.len() <= size {
            return Ok(vec![format!("{prefix}{text}")]);
        }
        let step = size - size / 8;
        let mut out = Vec::new();
        let mut first = 0usize;
        loop {
            let end = (first + size).min(offsets.len());
            let span = offsets.get(first..end).unwrap_or(&[]);
            let start = span.iter().map(|o| o.0).min().unwrap_or(0);
            let last = span.iter().map(|o| o.1).max().unwrap_or(text.len());
            out.push(format!("{prefix}{}", char_span(text, start, last)));
            if end >= offsets.len() {
                return Ok(out);
            }
            first += step;
        }
    }

    /// The row of an embedding table as values.
    fn row<'a>(&'a self, table: &'a Table, row: usize) -> impl Iterator<Item = f32> + 'a {
        let start = table
            .data
            .start
            .saturating_add(row.saturating_mul(self.hidden));
        let end = start.saturating_add(self.hidden).min(table.data.end);
        self.bytes
            .get(start..end)
            .unwrap_or(&[])
            .iter()
            .map(move |q| f32::from(i16::from(*q) - table.zero) * table.scale)
            .chain(std::iter::repeat(0.0))
            .take(self.hidden)
    }

    /// The mean-pooled vector of token ids.
    #[allow(clippy::many_single_char_names)] // q, k, v, h: the names of the formula
    fn forward(&self, ids: &[u32]) -> Vec<f32> {
        let (hidden, n) = (self.hidden, ids.len().max(1));
        let mut h = vec![0f32; n * hidden];
        for (pos, (row, id)) in h.chunks_mut(hidden).zip(ids).enumerate() {
            let id = usize::try_from(*id).unwrap_or(usize::MAX);
            let parts = self
                .row(&self.word, id)
                .zip(self.row(&self.segment, 0))
                .zip(self.row(&self.position, pos));
            for (x, ((w, s), p)) in row.iter_mut().zip(parts) {
                *x = (w + s) + p;
            }
        }
        self.norm.apply(&mut h, hidden, self.eps);
        for layer in &self.layers {
            let q = layer.query.forward(&h);
            let k = layer.key.forward(&h);
            let v = layer.value.forward(&h);
            let context = self.attention(&q, &k, &v, n);
            let mut a = layer.output.forward(&context);
            for (x, r) in a.iter_mut().zip(&h) {
                *x += r;
            }
            layer.norm1.apply(&mut a, hidden, self.eps);
            let mut up = layer.up.forward(&a);
            for x in &mut up {
                *x = *x * (erf(*x / std::f32::consts::SQRT_2) + 1.0) * 0.5;
            }
            let mut d = layer.down.forward(&up);
            for (x, r) in d.iter_mut().zip(&a) {
                *x += r;
            }
            layer.norm2.apply(&mut d, hidden, self.eps);
            h = d;
        }
        let mut mean = vec![0f64; hidden];
        for row in h.chunks(hidden) {
            for (m, x) in mean.iter_mut().zip(row) {
                *m += f64::from(*x);
            }
        }
        #[allow(clippy::cast_precision_loss)]
        let count = n as f64;
        for m in &mut mean {
            *m /= count;
        }
        let norm = if self.normalize {
            mean.iter().map(|x| x * x).sum::<f64>().sqrt()
        } else {
            1.0
        };
        #[allow(clippy::cast_possible_truncation)]
        let out = mean
            .iter()
            .map(|x| if norm > 0.0 { (x / norm) as f32 } else { 0.0 })
            .collect();
        out
    }

    /// Self-attention of `n` tokens: per head the scaled dot products of the
    /// quantized queries and keys, softmax, then the quantized probabilities
    /// times the quantized values; the heads side by side.
    #[allow(clippy::many_single_char_names)] // q, k, v, m, n: the names of the formula
    fn attention(&self, q: &[f32], k: &[f32], v: &[f32], n: usize) -> Vec<f32> {
        let (hidden, heads) = (self.hidden, self.heads);
        let dh = hidden / heads;
        let (qq, qs) = quantize(q);
        let (kq, ks) = quantize(k);
        let (vq, vs) = quantize(v);
        // [head][token][dh] for the queries and keys, [head][dh][token] for the values
        let by_head = |x: &[i16]| -> Vec<i16> {
            let mut out = Vec::with_capacity(x.len());
            for head in 0..heads {
                for row in x.chunks(hidden) {
                    out.extend_from_slice(row.get(head * dh..(head + 1) * dh).unwrap_or(&[]));
                }
            }
            out
        };
        let (qh, kh) = (by_head(&qq), by_head(&kq));
        let mut vt = vec![0i16; hidden * n];
        for (t, row) in vq.chunks(hidden).enumerate() {
            for (c, x) in row.iter().enumerate() {
                if let Some(slot) = vt.get_mut(c * n + t) {
                    *slot = *x;
                }
            }
        }
        #[allow(clippy::cast_precision_loss)]
        let root = (dh as f32).sqrt();
        let m = qs * ks;
        let mut probs = vec![0f32; heads * n * n];
        for ((block, qb), kb) in probs
            .chunks_mut(n * n)
            .zip(qh.chunks(n * dh))
            .zip(kh.chunks(n * dh))
        {
            for (row, qi) in block.chunks_mut(n).zip(qb.chunks(dh)) {
                for (p, kj) in row.iter_mut().zip(kb.chunks(dh)) {
                    #[allow(clippy::cast_precision_loss)]
                    let score = dot(qi, kj) as f32 * m / root;
                    *p = score;
                }
                softmax(row);
            }
        }
        let (pq, ps) = quantize(&probs);
        let m = ps * vs;
        let mut context = vec![0f32; n * hidden];
        for (t, out) in context.chunks_mut(hidden).enumerate() {
            for (c, y) in out.iter_mut().enumerate() {
                let head = c / dh;
                let row = pq
                    .get((head * n + t) * n..(head * n + t + 1) * n)
                    .unwrap_or(&[]);
                let column = vt.get(c * n..(c + 1) * n).unwrap_or(&[]);
                #[allow(clippy::cast_precision_loss)]
                let value = dot(row, column) as f32 * m;
                *y = value;
            }
        }
        context
    }
}

/// `text[start..stop]`, the ends moved out to character boundaries.
fn char_span(text: &str, start: usize, stop: usize) -> &str {
    let mut a = start.min(text.len());
    while !text.is_char_boundary(a) {
        a -= 1;
    }
    let mut b = stop.clamp(a, text.len());
    while !text.is_char_boundary(b) {
        b += 1;
    }
    text.get(a..b).unwrap_or("")
}

impl Norm {
    /// Layer norm of every row of `x`, in f32, sums in index order.
    fn apply(&self, x: &mut [f32], width: usize, eps: f32) {
        #[allow(clippy::cast_precision_loss)]
        let count = width as f32;
        for row in x.chunks_mut(width) {
            let mean = row.iter().sum::<f32>() / count;
            let var = row.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>() / count;
            let denom = (var + eps).sqrt();
            for ((v, w), b) in row.iter_mut().zip(&self.weight).zip(&self.bias) {
                *v = (*v - mean) / denom * w + b;
            }
        }
    }
}

impl Linear {
    /// `x` (rows of `inputs` values) through the map: the quantized input
    /// times the weights in integers, then `acc * (scale_x * scale) + bias`.
    fn forward(&self, x: &[f32]) -> Vec<f32> {
        let (xq, scale_x) = quantize(x);
        let k = self.inputs.max(1);
        let outputs = self.scale.len();
        let m: Vec<f32> = self.scale.iter().map(|w| scale_x * w).collect();
        let mut y = vec![0f32; x.len() / k * outputs];
        for (xr, yr) in xq.chunks(k).zip(y.chunks_mut(outputs)) {
            for (((out, w), mj), bj) in yr
                .iter_mut()
                .zip(self.weights.chunks(k))
                .zip(&m)
                .zip(&self.bias)
            {
                #[allow(clippy::cast_precision_loss)]
                let value = dot(xr, w) as f32 * mj + bj;
                *out = value;
            }
        }
        y
    }
}

/// ONNX `DynamicQuantizeLinear` of a whole tensor, returned as `q - zero`
/// (from -255 to 255), and the scale: the range always holds 0, the scale is
/// the range over 255, zero point and values round half to even.
fn quantize(x: &[f32]) -> (Vec<i16>, f32) {
    let lo = x.iter().copied().fold(0f32, f32::min);
    let hi = x.iter().copied().fold(0f32, f32::max);
    let scale = (hi - lo) / 255.0;
    if scale.is_nan() || scale <= 0.0 {
        return (vec![0; x.len()], 0.0);
    }
    let zero = (0.0 - lo / scale).clamp(0.0, 255.0).round_ties_even();
    #[allow(clippy::cast_possible_truncation)]
    let q = x
        .iter()
        .map(|v| (((v / scale).round_ties_even() + zero).clamp(0.0, 255.0) - zero) as i16)
        .collect();
    (q, scale)
}

/// `e^x` for `x <= 0` (Cephes `expf`): `x = n ln 2 + r` with `n` rounded half
/// to even by adding `1.5 * 2^23`, `r` in two steps (Cody-Waite), `e^r` by a
/// degree-5 polynomial, `2^n` from the bits of that sum (its last bits are
/// `n`). Below -87 it gives `e^-87`. Only `+`, `*` and integer bit work, so
/// the bits are the same on every platform and the loops around it vectorise.
#[allow(clippy::many_single_char_names)] // x, t, n, r, p: the names of Cephes
fn exp_neg(x: f32) -> f32 {
    const MAGIC: f32 = 12_582_912.0;
    let x = if x < -87.0 { -87.0 } else { x };
    let t = x * std::f32::consts::LOG2_E + MAGIC;
    let n = t - MAGIC;
    let r = (x - n * 0.693_359_4) - n * -2.121_944_4e-4;
    let p = ((((1.987_569_1e-4 * r + 1.398_199_9e-3) * r + 8.333_452e-3) * r + 4.166_579_6e-2) * r
        + 1.666_666_5e-1)
        * r
        + 0.5;
    // n from -126 to 0: the exponent field of 2^n is n + 127
    let two_n = f32::from_bits(t.to_bits().wrapping_add(127).wrapping_sub(0x4B40_0000) << 23);
    (p * r * r + r + 1.0) * two_n
}

/// `erf(x)` by Abramowitz-Stegun 7.1.26 (error below 1.5e-7) with
/// [`exp_neg`]: `1 - t (a1 + t (a2 + ...)) e^(-x^2)`, `t = 1 / (1 + p |x|)`.
fn erf(x: f32) -> f32 {
    let a = x.abs();
    let t = 1.0 / (1.0 + 0.327_591_1 * a);
    let poly = t
        * (0.254_829_6
            + t * (-0.284_496_74 + t * (1.421_413_8 + t * (-1.453_152 + t * 1.061_405_4))));
    let y = 1.0 - poly * exp_neg(-(a * a));
    if x < 0.0 { -y } else { y }
}

/// Softmax of one row in f32: `exp(x - max)` over their sum. The sum runs
/// in 8 lanes (value `i` into lane `i mod 8`, each lane in index order), then
/// the lanes in order: a fixed order that vectorises.
fn softmax(row: &mut [f32]) {
    let max = row.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    for x in row.iter_mut() {
        *x = exp_neg(*x - max);
    }
    let mut lanes = [0f32; 8];
    for chunk in row.chunks(8) {
        for (lane, x) in lanes.iter_mut().zip(chunk) {
            *lane += x;
        }
    }
    let sum: f32 = lanes.iter().sum();
    for x in row.iter_mut() {
        *x /= sum;
    }
}

/// Exact integer dot product.
fn dot(a: &[i16], b: &[i16]) -> i32 {
    a.iter()
        .zip(b)
        .map(|(x, y)| i32::from(*x) * i32::from(*y))
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantize_follows_dynamic_quantize_linear() {
        // range [-1, 3]: scale 4/255, zero point round(255/4) = 64
        let (q, s) = quantize(&[-1.0, 0.0, 3.0, 1.0]);
        assert!((s - 4.0 / 255.0).abs() < 1e-9);
        assert_eq!(q, vec![-64, 0, 191, 64]);
        // all zeros: scale 0, everything 0
        assert_eq!(quantize(&[0.0, 0.0]), (vec![0, 0], 0.0));
        // positive only: the range still starts at 0
        let (q, _) = quantize(&[0.25, 1.0]);
        assert_eq!(q, vec![64, 255]);
    }

    #[test]
    fn exp_and_erf_are_close_to_libm() {
        for i in 0..=2000u16 {
            let x = -f32::from(i) * 0.05;
            let (got, want) = (exp_neg(x), libm::expf(x));
            assert!(
                (got - want).abs() <= 2e-7 * want.max(1e-30) + 1e-37,
                "exp({x}): {got} {want}"
            );
        }
        for i in 0..=1200u16 {
            let x = f32::from(i) * 0.01 - 6.0;
            let (got, want) = (erf(x), libm::erff(x));
            assert!((got - want).abs() < 6e-7, "erf({x}): {got} {want}");
        }
    }

    #[test]
    fn char_span_moves_to_boundaries() {
        assert_eq!(char_span("aé b", 2, 3), "é");
        assert_eq!(char_span("abc", 1, 9), "bc");
    }
}
