//! `kapu bench` (spec chapter 8, M8/2, D49): retrieval quality of a file on a
//! question set.
//!
//! Questions are JSON lines `{"kerdes", "valasz_fajl", "valasz_szakasz",
//! "nyelv"}`. The expected door is
//! `slug(file name without extension)/slug(section)`, or the room when the
//! section is empty. A strict hit is the expected door in the top k; a
//! lenient hit is the expected door or its room (D49); a room-level hit is any
//! door of the expected room (D130).

use crate::embed::VectorCache;
use crate::error::{Error, ErrorCode, Result};
use crate::search::rankings;
use crate::text::slug;

/// One question of the benchmark.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    /// The query text.
    pub kerdes: String,
    /// Source file of the answer (`gombok.md`).
    pub valasz_fajl: String,
    /// Section heading of the answer (empty: the whole room).
    pub valasz_szakasz: String,
    /// `hu` or `en` (informative).
    pub nyelv: String,
}

impl Question {
    /// The expected door id (D49).
    #[must_use]
    pub fn expected(&self) -> String {
        let stem = std::path::Path::new(&self.valasz_fajl)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let room = slug(&stem);
        if self.valasz_szakasz.trim().is_empty() {
            room
        } else {
            format!("{room}/{}", slug(&self.valasz_szakasz))
        }
    }
}

/// Parses the JSON-lines question file; empty lines are skipped.
///
/// # Errors
/// E012 with the line number for a line that is not a JSON object with a
/// string `kerdes` and `valasz_fajl`.
pub fn parse_questions(text: &str) -> Result<Vec<Question>> {
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let bad = || {
            Error::new(
                ErrorCode::E012,
                format!("question line {}: not a valid question object", n + 1),
            )
        };
        let v: serde_json::Value = serde_json::from_str(line).map_err(|_| bad())?;
        let field = |k: &str| {
            v.get(k)
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .to_owned()
        };
        let q = Question {
            kerdes: field("kerdes"),
            valasz_fajl: field("valasz_fajl"),
            valasz_szakasz: field("valasz_szakasz"),
            nyelv: field("nyelv"),
        };
        if q.kerdes.is_empty() || q.valasz_fajl.is_empty() {
            return Err(bad());
        }
        out.push(q);
    }
    Ok(out)
}

/// recall@5, recall@10 and MRR@10 of one ranking mode, strict and lenient.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Scores {
    /// Share of questions with a strict hit in the top 5.
    pub recall5: f64,
    /// ... in the top 10.
    pub recall10: f64,
    /// Mean reciprocal rank of the first strict hit within 10.
    pub mrr10: f64,
    /// Lenient recall@5.
    pub lenient_recall5: f64,
    /// Lenient recall@10.
    pub lenient_recall10: f64,
    /// Lenient MRR@10.
    pub lenient_mrr10: f64,
    /// Room-level recall@5: a hit is any door of the expected door's room
    /// (D130), the unit a chunk baseline is scored with.
    pub room_recall5: f64,
    /// Room-level recall@10.
    pub room_recall10: f64,
    /// Room-level MRR@10.
    pub room_mrr10: f64,
}

/// The benchmark result.
#[derive(Debug, Clone, PartialEq)]
pub struct Evaluation {
    /// Number of questions.
    pub questions: usize,
    /// BM25 only.
    pub bm25: Scores,
    /// Vectors only (`None` without a matching model).
    pub vector: Option<Scores>,
    /// Hybrid (RRF).
    pub hybrid: Scores,
}

fn first_rank(list: &[String], hit: &dyn Fn(&str) -> bool) -> Option<usize> {
    list.iter().take(10).position(|id| hit(id)).map(|p| p + 1)
}

fn add_rank(rank: Option<usize>, r5: &mut f64, r10: &mut f64, mrr: &mut f64) {
    if let Some(r) = rank {
        if r <= 5 {
            *r5 += 1.0;
        }
        *r10 += 1.0;
        #[allow(clippy::cast_precision_loss)]
        let reciprocal = 1.0 / r as f64;
        *mrr += reciprocal;
    }
}

fn score(lists: &[(Vec<String>, String)]) -> Scores {
    let mut s = Scores::default();
    if lists.is_empty() {
        return s;
    }
    for (list, expected) in lists {
        let room = expected.split('/').next().unwrap_or(expected).to_owned();
        let strict = first_rank(list, &|id| id == expected);
        let lenient = first_rank(list, &|id| id == expected || id == room);
        let room_level = first_rank(list, &|id| id.split('/').next() == Some(room.as_str()));
        add_rank(strict, &mut s.recall5, &mut s.recall10, &mut s.mrr10);
        add_rank(
            lenient,
            &mut s.lenient_recall5,
            &mut s.lenient_recall10,
            &mut s.lenient_mrr10,
        );
        add_rank(
            room_level,
            &mut s.room_recall5,
            &mut s.room_recall10,
            &mut s.room_mrr10,
        );
    }
    #[allow(clippy::cast_precision_loss)]
    let n = lists.len() as f64;
    for v in [
        &mut s.recall5,
        &mut s.recall10,
        &mut s.mrr10,
        &mut s.lenient_recall5,
        &mut s.lenient_recall10,
        &mut s.lenient_mrr10,
        &mut s.room_recall5,
        &mut s.room_recall10,
        &mut s.room_mrr10,
    ] {
        *v /= n;
    }
    s
}

/// Runs every question against the file.
///
/// # Errors
/// A broken file or a tokenizer failure.
pub fn evaluate(
    file: &[u8],
    questions: &[Question],
    cache: Option<&VectorCache<'_>>,
) -> Result<Evaluation> {
    let mut bm25 = Vec::with_capacity(questions.len());
    let mut vector = Vec::with_capacity(questions.len());
    let mut hybrid = Vec::with_capacity(questions.len());
    let mut vector_available = true;
    for q in questions {
        let r = rankings(file, &q.kerdes, cache)?;
        let expected = q.expected();
        bm25.push((r.bm25, expected.clone()));
        match r.vector {
            Some(v) => vector.push((v, expected.clone())),
            None => vector_available = false,
        }
        hybrid.push((r.hybrid, expected));
    }
    Ok(Evaluation {
        questions: questions.len(),
        bm25: score(&bm25),
        vector: vector_available.then(|| score(&vector)),
        hybrid: score(&hybrid),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expected_door_follows_d49() {
        let q = Question {
            kerdes: "x".into(),
            valasz_fajl: "dir/Gombok.md".into(),
            valasz_szakasz: "Glass gomb".into(),
            nyelv: "hu".into(),
        };
        assert_eq!(q.expected(), "gombok/glass-gomb");
        let room = Question {
            valasz_szakasz: String::new(),
            ..q
        };
        assert_eq!(room.expected(), "gombok");
    }

    #[test]
    fn question_lines() {
        let text = "{\"kerdes\":\"a\",\"valasz_fajl\":\"b.md\"}\n\n{\"kerdes\":\"c\",\"valasz_fajl\":\"d.md\",\"valasz_szakasz\":\"E\",\"nyelv\":\"en\"}\n";
        assert_eq!(parse_questions(text).unwrap().len(), 2);
        assert_eq!(
            parse_questions("{\"kerdes\":\"a\"}").unwrap_err().code,
            ErrorCode::E012
        );
    }

    #[test]
    fn scoring() {
        let lists = vec![
            (vec!["x".to_owned(), "a/b".to_owned()], "a/b".to_owned()),
            (vec!["a".to_owned()], "a/c".to_owned()),
        ];
        let s = score(&lists);
        assert!((s.recall5 - 0.5).abs() < 1e-12);
        assert!((s.mrr10 - 0.25).abs() < 1e-12);
        assert!((s.lenient_recall10 - 1.0).abs() < 1e-12);
        assert!((s.lenient_mrr10 - 0.75).abs() < 1e-12);
    }

    #[test]
    fn room_level_counts_any_door_of_the_room() {
        // D130: a sub-door of the expected room is a room-level hit, not a lenient one
        let lists = vec![
            (vec!["x".to_owned(), "a/z".to_owned()], "a".to_owned()),
            (vec!["b/c".to_owned()], "a/c".to_owned()),
        ];
        let s = score(&lists);
        assert!(s.lenient_recall10.abs() < 1e-12);
        assert!((s.room_recall10 - 0.5).abs() < 1e-12);
        assert!((s.room_mrr10 - 0.25).abs() < 1e-12);
        assert!((s.room_recall5 - 0.5).abs() < 1e-12);
    }
}
