//! Automatic tags with YAKE (Campos et al., 2020), FR-06/3 and D84.
//!
//! Used when a room has no `tags:` line or an empty one. Deterministic: no
//! hashing order, ties broken by byte order. Simplifications against the
//! paper (D84): candidates are 1-3 consecutive non-stop words inside one
//! sentence (no stop words inside a phrase), the relatedness window is one
//! word, deduplication uses a normalised Levenshtein similarity of 0.9.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

/// How many tags YAKE returns (ASSUMPTION-03).
pub const TAG_COUNT: usize = 8;
const MAX_NGRAM: usize = 3;
const DEDUP: f64 = 0.9;

fn stopwords() -> &'static BTreeSet<&'static str> {
    static SET: OnceLock<BTreeSet<&'static str>> = OnceLock::new();
    SET.get_or_init(|| {
        include_str!("../data/stopwords.txt")
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .collect()
    })
}

/// Text YAKE looks at: the served text without fenced code, without the
/// `tags:` line and without Markdown heading and quote markers.
fn prose(served: &str) -> String {
    let lines: Vec<&str> = served.split('\n').collect();
    let mask = crate::text::code_fence_mask(&lines);
    lines
        .iter()
        .zip(mask)
        .filter(|(l, in_code)| !in_code && !l.starts_with("tags:"))
        .map(|(l, _)| l.trim_start_matches(['#', '>', ' ']))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Sentences of words. A sentence ends at `.`, `!`, `?`, `;`, `:` or a line
/// break; a word is a run of letters, digits, `-` and `_` (trimmed of `-`/`_`).
fn sentences(text: &str) -> Vec<Vec<String>> {
    let mut out: Vec<Vec<String>> = Vec::new();
    let mut cur: Vec<String> = Vec::new();
    let mut word = String::new();
    let flush_word = |word: &mut String, cur: &mut Vec<String>| {
        let w = word.trim_matches(|c| c == '-' || c == '_');
        if !w.is_empty() {
            cur.push(w.to_owned());
        }
        word.clear();
    };
    for c in text.chars() {
        if c.is_alphanumeric() || c == '-' || c == '_' {
            word.push(c);
        } else {
            flush_word(&mut word, &mut cur);
            if matches!(c, '.' | '!' | '?' | ';' | ':' | '\n') && !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
        }
    }
    flush_word(&mut word, &mut cur);
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

#[derive(Default)]
struct Stats {
    tf: f64,
    upper: f64,
    acronym: f64,
    sentences: BTreeSet<usize>,
    left: BTreeMap<String, u32>,
    right: BTreeMap<String, u32>,
}

/// A score rounded to 1e-9 for comparisons (D105).
#[must_use]
pub fn tie_key(score: f64) -> f64 {
    (score * 1e9).round()
}

fn is_stop(word: &str) -> bool {
    stopwords().contains(word) || word.chars().all(char::is_numeric) || word.chars().count() < 2
}

/// Per lower-cased word: frequency, casing, sentences and neighbours.
fn collect_stats(sents: &[Vec<String>]) -> BTreeMap<String, Stats> {
    let mut stats: BTreeMap<String, Stats> = BTreeMap::new();
    for (si, sent) in sents.iter().enumerate() {
        for (wi, word) in sent.iter().enumerate() {
            let st = stats.entry(word.to_lowercase()).or_default();
            st.tf += 1.0;
            let first_upper = word.chars().next().is_some_and(char::is_uppercase);
            let acronym = word.chars().count() > 1
                && word.chars().all(|c| !c.is_lowercase())
                && word.chars().any(char::is_uppercase);
            if acronym {
                st.acronym += 1.0;
            } else if first_upper && wi > 0 {
                st.upper += 1.0;
            }
            st.sentences.insert(si);
            if let Some(left) = wi.checked_sub(1).and_then(|i| sent.get(i)) {
                *st.left.entry(left.to_lowercase()).or_insert(0) += 1;
            }
            if let Some(right) = sent.get(wi + 1) {
                *st.right.entry(right.to_lowercase()).or_insert(0) += 1;
            }
        }
    }
    stats
}

/// YAKE word score `S(t) = (TRel * TPos) / (TCase + TFNorm/TRel + TSent/TRel)`;
/// lower is more important.
fn word_scores(stats: &BTreeMap<String, Stats>, sentence_count: usize) -> BTreeMap<&str, f64> {
    // TFNorm uses the mean and standard deviation of non-stop-word frequencies
    let content: Vec<f64> = stats
        .iter()
        .filter(|(w, _)| !is_stop(w))
        .map(|(_, s)| s.tf)
        .collect();
    #[allow(clippy::cast_precision_loss)]
    let n = content.len().max(1) as f64;
    let mean = content.iter().sum::<f64>() / n;
    let std = (content.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n).sqrt();
    let max_tf = stats.values().map(|s| s.tf).fold(0.0, f64::max);
    #[allow(clippy::cast_precision_loss)]
    let sentence_count = sentence_count.max(1) as f64;

    let mut score = BTreeMap::new();
    for (word, s) in stats {
        let t_case = s.upper.max(s.acronym) / (1.0 + libm::log(s.tf));
        // BTreeSet iterates in order, so the indices are already sorted
        let idx: Vec<usize> = s.sentences.iter().copied().collect();
        #[allow(clippy::cast_precision_loss)]
        let median = match idx.len() {
            0 => 0.0,
            l if l % 2 == 1 => idx.get(l / 2).copied().unwrap_or(0) as f64,
            l => {
                (idx.get(l / 2 - 1).copied().unwrap_or(0) + idx.get(l / 2).copied().unwrap_or(0))
                    as f64
                    / 2.0
            }
        };
        let t_pos = libm::log(libm::log(3.0 + median));
        let t_norm = s.tf / (mean + std).max(f64::MIN_POSITIVE);
        let t_rel = 1.0 + (side(&s.left) + side(&s.right)) * s.tf / max_tf.max(1.0);
        #[allow(clippy::cast_precision_loss)]
        let t_sent = s.sentences.len() as f64 / sentence_count;
        score.insert(
            word.as_str(),
            (t_rel * t_pos) / (t_case + t_norm / t_rel + t_sent / t_rel),
        );
    }
    score
}

/// Candidates of 1..3 consecutive non-stop words within a sentence, scored
/// `prod(S) / (TF * (1 + sum(S)))`, best (lowest) first, ties in byte order.
fn ranked_candidates(sents: &[Vec<String>], score: &BTreeMap<&str, f64>) -> Vec<String> {
    let mut cand_tf: BTreeMap<String, f64> = BTreeMap::new();
    for sent in sents {
        let lower: Vec<String> = sent.iter().map(|w| w.to_lowercase()).collect();
        for start in 0..lower.len() {
            for len in 1..=MAX_NGRAM {
                let Some(words) = lower.get(start..start + len) else {
                    break;
                };
                if words.iter().any(|w| is_stop(w)) {
                    break;
                }
                *cand_tf.entry(words.join(" ")).or_insert(0.0) += 1.0;
            }
        }
    }
    let mut ranked: Vec<(String, f64)> = cand_tf
        .into_iter()
        .map(|(kw, tf)| {
            let parts: Vec<f64> = kw
                .split(' ')
                .map(|w| score.get(w).copied().unwrap_or(1.0))
                .collect();
            let prod: f64 = parts.iter().product();
            let sum: f64 = parts.iter().sum();
            let s = prod / (tf * (1.0 + sum));
            (kw, s)
        })
        .collect();
    // D105: scores equal to 1e-9 are ties, broken by byte order, so the last
    // bits of the floating-point arithmetic never decide the order
    ranked.sort_by(|a, b| {
        tie_key(a.1)
            .total_cmp(&tie_key(b.1))
            .then_with(|| a.0.as_bytes().cmp(b.0.as_bytes()))
    });
    ranked.into_iter().map(|(kw, _)| kw).collect()
}

/// Up to [`TAG_COUNT`] lower-case keywords, best first. Empty text gives none.
#[must_use]
pub fn keywords(served: &str) -> Vec<String> {
    let sents = sentences(&prose(served));
    if sents.is_empty() {
        return Vec::new();
    }
    let stats = collect_stats(&sents);
    let score = word_scores(&stats, sents.len());
    let mut out: Vec<String> = Vec::new();
    for kw in ranked_candidates(&sents, &score) {
        if out.len() == TAG_COUNT {
            break;
        }
        if kw.chars().count() > crate::text::TAG_MAX
            || out.iter().any(|k| similarity(k, &kw) >= DEDUP)
        {
            continue;
        }
        out.push(kw);
    }
    out
}

// Distinct neighbours over neighbour occurrences (YAKE `WL`/`WR`).
fn side(m: &BTreeMap<String, u32>) -> f64 {
    let total: u32 = m.values().sum();
    if total == 0 {
        0.0
    } else {
        #[allow(clippy::cast_precision_loss)]
        let r = m.len() as f64 / f64::from(total);
        r
    }
}

/// 1 - Levenshtein distance / longer length, on characters.
fn similarity(a: &str, b: &str) -> f64 {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let longest = a.len().max(b.len());
    if longest == 0 {
        return 1.0;
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let sub = prev.get(j).copied().unwrap_or(0) + usize::from(ca != cb);
            let del = prev.get(j + 1).copied().unwrap_or(0) + 1;
            let ins = cur.get(j).copied().unwrap_or(0) + 1;
            cur.push(sub.min(del).min(ins));
        }
        prev = cur;
    }
    #[allow(clippy::cast_precision_loss)]
    let d = prev.last().copied().unwrap_or(0) as f64 / longest as f64;
    1.0 - d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_and_template_give_nothing() {
        assert_eq!(keywords(""), Vec::<String>::new());
        assert_eq!(keywords("# \n\n> \n\ntags: \n\n"), Vec::<String>::new());
    }

    #[test]
    fn finds_the_topic_and_is_deterministic() {
        let text = "# Glass gomb\nAz üvegszerű gomb elmosott hátteret kap. A gomb színe türkiz. \
                    Az üvegszerű gomb a sötét témában jól mutat. A backdrop-filter adja az elmosást.\n\
                    ```css\n.x { color: red; }\n```\n";
        let a = keywords(text);
        assert_eq!(a, keywords(text));
        assert!(!a.is_empty() && a.len() <= TAG_COUNT);
        assert!(a.iter().any(|k| k.contains("gomb")), "{a:?}");
        assert!(
            !a.iter().any(|k| k.contains("color")),
            "code is not prose: {a:?}"
        );
    }

    #[test]
    fn similar_phrases_are_deduplicated() {
        assert!(similarity("gombok", "gombok") >= DEDUP);
        assert!(similarity("gomb", "türkiz") < DEDUP);
    }
}
