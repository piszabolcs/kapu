//! Deterministic synthetic corpus generator (TERV.md M1, used from M3 on).
//!
//! `kapu-bench gen --rooms N --seed S --out DIR` writes `N` Markdown rooms as
//! `DIR/<room-id>.md`. The same `N` and `S` give the same bytes on every
//! platform: the only randomness is a self-contained xoshiro256** generator
//! (D43 forbids relying on `std` hashing or platform PRNGs for anything that
//! must be reproducible). Rooms mix Hungarian and English prose, a `tags:` line,
//! `##` sections, a fenced code block and a link to another room, so the
//! builder's caption, sub-door, index and graph paths all get exercised.

use std::fmt::Write as _;
use std::path::Path;

/// xoshiro256** (Blackman & Vigna), seeded through `splitmix64`.
pub struct Xoshiro256 {
    s: [u64; 4],
}

impl Xoshiro256 {
    #[must_use]
    pub fn from_seed(seed: u64) -> Self {
        let mut x = seed;
        let mut next = move || {
            x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = x;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        };
        Self {
            s: [next(), next(), next(), next()],
        }
    }

    pub fn next_u64(&mut self) -> u64 {
        let result = self.s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = self.s[1] << 17;
        self.s[2] ^= self.s[0];
        self.s[3] ^= self.s[1];
        self.s[1] ^= self.s[2];
        self.s[0] ^= self.s[3];
        self.s[2] ^= t;
        self.s[3] = self.s[3].rotate_left(45);
        result
    }

    /// Uniform-ish value in `0..n` (`n == 0` gives 0). The modulo bias is
    /// irrelevant for corpus generation; determinism is what matters.
    pub fn below(&mut self, n: usize) -> usize {
        let n64 = u64::try_from(n).unwrap_or(u64::MAX).max(1);
        usize::try_from(self.next_u64() % n64).unwrap_or(0)
    }

    pub fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items.get(self.below(items.len())).copied().unwrap_or("")
    }
}

const TITLE_WORDS: &[&str] = &[
    "Glass", "Button", "Preload", "Bridge", "Palette", "Layout", "Grid", "Token", "Theme", "Cache",
    "Router", "Schema", "Worker", "Queue", "Stream", "Parser", "Header", "Footer", "Modal",
    "Toast",
];
const TOPICS: &[&str] = &[
    "frontend", "design", "electron", "rust", "python", "sql", "css", "testing", "mcp", "cli",
];
const HU_WORDS: &[&str] = &[
    "a",
    "az",
    "és",
    "gomb",
    "szín",
    "sötét",
    "téma",
    "üvegszerű",
    "elmosott",
    "háttér",
    "hiba",
    "megoldás",
    "függvény",
    "szoba",
    "ajtó",
    "térkép",
    "keresés",
    "tartalom",
    "minta",
    "szabály",
    "lásd",
    "mindig",
    "soha",
    "csak",
    "például",
    "fontos",
    "gyors",
    "lassú",
    "kicsi",
    "nagy",
];
const EN_WORDS: &[&str] = &[
    "the",
    "a",
    "button",
    "color",
    "dark",
    "theme",
    "glass",
    "blurred",
    "background",
    "error",
    "fix",
    "function",
    "room",
    "door",
    "map",
    "search",
    "content",
    "pattern",
    "rule",
    "see",
    "always",
    "never",
    "only",
    "example",
    "important",
    "fast",
    "slow",
    "small",
    "large",
    "and",
];
const CODE_LANGS: &[&str] = &["css", "ts", "rust", "python", "sql"];
const CODE_IDENTS: &[&str] = &[
    "btn-glass",
    "backdropFilter",
    "contextIsolation",
    "ipcRenderer",
    "--turkiz",
    "--bronz",
    "render_map",
    "openDoor",
    "search_index",
    "SELECT",
    "fn build",
    "def distill",
];

fn slug_ascii(title: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for ch in title.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    out.trim_end_matches('-').to_owned()
}

fn sentence(rng: &mut Xoshiro256, hungarian: bool) -> String {
    let words = if hungarian { HU_WORDS } else { EN_WORDS };
    let n = 6 + rng.below(9);
    let mut s = String::new();
    for i in 0..n {
        let w = rng.pick(words);
        if i == 0 {
            let mut chars = w.chars();
            if let Some(first) = chars.next() {
                s.extend(first.to_uppercase());
                s.push_str(chars.as_str());
            }
        } else {
            s.push(' ');
            s.push_str(w);
        }
    }
    s.push('.');
    s
}

fn paragraph(rng: &mut Xoshiro256) -> String {
    let hungarian = rng.below(2) == 0;
    let n = 2 + rng.below(3);
    (0..n)
        .map(|_| sentence(rng, hungarian))
        .collect::<Vec<_>>()
        .join(" ")
}

fn code_block(rng: &mut Xoshiro256) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "```{}", rng.pick(CODE_LANGS));
    let lines = 2 + rng.below(4);
    for _ in 0..lines {
        let _ = writeln!(
            s,
            "{} {}: {};",
            rng.pick(CODE_IDENTS),
            rng.pick(EN_WORDS),
            rng.below(1000)
        );
    }
    s.push_str("```\n");
    s
}

/// Title and id of room `i` (1-based number keeps ids unique).
fn room_title(rng: &mut Xoshiro256, i: usize) -> (String, String) {
    let title = format!(
        "{} {} {}",
        rng.pick(TITLE_WORDS),
        rng.pick(TITLE_WORDS),
        i + 1
    );
    let id = slug_ascii(&title);
    (title, id)
}

/// Generates `rooms` Markdown files into `out`. Returns the number written.
///
/// # Errors
/// Any I/O error while creating the directory or writing a file.
pub fn generate(rooms: usize, seed: u64, out: &Path) -> std::io::Result<usize> {
    std::fs::create_dir_all(out)?;
    let mut rng = Xoshiro256::from_seed(seed);
    let titles: Vec<(String, String)> = (0..rooms).map(|i| room_title(&mut rng, i)).collect();

    for (i, (title, id)) in titles.iter().enumerate() {
        let mut md = String::new();
        let _ = writeln!(md, "# {title}");
        let hungarian = rng_coin(&mut rng);
        let _ = writeln!(md, "> {}", sentence(&mut rng, hungarian));
        let tag_count = 3 + rng.below(3);
        let tags: Vec<&str> = (0..tag_count).map(|_| rng.pick(TOPICS)).collect();
        let _ = writeln!(md, "tags: {}", tags.join(", "));
        md.push('\n');
        md.push_str(&paragraph(&mut rng));
        md.push('\n');

        let sections = 2 + rng.below(3);
        for s in 0..sections {
            let _ = write!(md, "\n## {} {}\n", rng.pick(TITLE_WORDS), s + 1);
            md.push_str(&paragraph(&mut rng));
            md.push('\n');
            if s == 0 {
                md.push_str(&code_block(&mut rng));
            }
        }

        // One link to another room; the target exists, so it becomes a `link` edge.
        if rooms > 1 {
            let mut j = rng.below(rooms);
            if j == i {
                j = (j + 1) % rooms;
            }
            if let Some((other_title, other_id)) = titles.get(j) {
                let _ = writeln!(md, "\nLásd még: [{other_title}](#{other_id}).");
            }
        }

        std::fs::write(out.join(format!("{id}.md")), md)?;
    }
    Ok(titles.len())
}

fn rng_coin(rng: &mut Xoshiro256) -> bool {
    rng.below(2) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xoshiro_is_deterministic_and_not_constant() {
        let mut a = Xoshiro256::from_seed(42);
        let mut b = Xoshiro256::from_seed(42);
        let xs: Vec<u64> = (0..8).map(|_| a.next_u64()).collect();
        let ys: Vec<u64> = (0..8).map(|_| b.next_u64()).collect();
        assert_eq!(xs, ys);
        assert!(xs.windows(2).any(|w| w.first() != w.get(1)));
    }

    #[test]
    fn slug_follows_the_ascii_rules() {
        assert_eq!(slug_ascii("Glass Button 17"), "glass-button-17");
        assert_eq!(slug_ascii("  Modal--Toast  "), "modal-toast");
    }

    #[test]
    fn generate_is_reproducible() {
        let dir = std::env::temp_dir().join(format!("kapu-bench-gen-{}", std::process::id()));
        let a = dir.join("a");
        let b = dir.join("b");
        assert_eq!(generate(5, 42, &a).unwrap_or(0), 5);
        assert_eq!(generate(5, 42, &b).unwrap_or(0), 5);
        let read = |d: &Path| -> Vec<(String, Vec<u8>)> {
            let mut v: Vec<_> = std::fs::read_dir(d)
                .map(|it| {
                    it.filter_map(Result::ok)
                        .map(|e| {
                            let name = e.file_name().to_string_lossy().into_owned();
                            (name, std::fs::read(e.path()).unwrap_or_default())
                        })
                        .collect()
                })
                .unwrap_or_default();
            v.sort();
            v
        };
        let (fa, fb) = (read(&a), read(&b));
        assert_eq!(fa, fb);
        assert!(fa.iter().all(|(_, bytes)| !bytes.contains(&b'\r')));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
