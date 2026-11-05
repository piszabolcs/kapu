//! The web reader (KD9): `llms.txt` with a Markdown page per link, then
//! `llms-full.txt`, then the sitemap, then a crawl, always inside the scope
//! (the host and the first path segment of the start URL, or `--scope`),
//! with `robots.txt`, four requests at a time, waiting on 429 and 5xx, and
//! at most `max_pages` pages. Only this module and the window use the
//! network (KD1).

use std::collections::{BTreeSet, HashSet, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use ureq::ResponseExt;
use url::Url;

use crate::{Error, Result};

/// Crawl limits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// The most pages fetched.
    pub max_pages: usize,
    /// Requests at the same time to the host.
    pub parallel: usize,
    /// The URL prefix pages must start with; empty: the host and the first
    /// path segment of the start URL.
    pub scope: String,
    /// Seconds per request.
    pub timeout: u64,
    /// The largest page, bytes.
    pub max_bytes: u64,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            max_pages: 5000,
            parallel: 4,
            scope: String::new(),
            timeout: 30,
            max_bytes: 20 << 20,
        }
    }
}

/// A fetched page.
#[derive(Debug, Clone, Default)]
pub struct Page {
    /// Its URL, without fragment.
    pub url: String,
    /// The body.
    pub bytes: Vec<u8>,
    /// `markdown` or `html` (from the content type and the URL).
    pub kind: PageKind,
    /// Title and description from `llms.txt`, if it named the page.
    pub title: String,
    /// Description from `llms.txt`.
    pub description: String,
}

/// How a page is read.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PageKind {
    /// HTML.
    #[default]
    Html,
    /// Markdown or plain text.
    Markdown,
}

/// What a crawl gives: the pages, how they were found, and what went wrong.
#[derive(Debug, Default)]
pub struct Crawl {
    /// The pages in a stable order.
    pub pages: Vec<Page>,
    /// `llms.txt`, `llms-full.txt`, `sitemap` or `crawl`.
    pub method: String,
    /// Problems (blocked pages, 403, script-only pages are found later).
    pub warnings: Vec<String>,
}

/// The `User-Agent` of every request.
#[must_use]
pub fn user_agent() -> String {
    format!(
        "KAPU/{} (kapu-create; +https://github.com/piszabolcs/kapu)",
        env!("CARGO_PKG_VERSION")
    )
}

struct Client {
    agent: ureq::Agent,
    robots: Option<texting_robots::Robot>,
    max_bytes: u64,
    /// Earliest time of the next request (the host's crawl delay and 429).
    next: Mutex<Instant>,
    delay: Duration,
}

/// The default scope of `start`: its host and first path segment.
#[must_use]
pub fn default_scope(start: &Url) -> String {
    let mut segments: Vec<&str> = start.path().split('/').filter(|s| !s.is_empty()).collect();
    // A start URL that names a file (`intro.html`) keeps its folder.
    if segments.last().is_some_and(|s| s.contains('.')) {
        segments.pop();
    }
    let first = segments.first().map_or(String::new(), |s| format!("{s}/"));
    format!("{}/{first}", start.origin().ascii_serialization())
}

/// Fetches the pages of the site at `start`.
///
/// # Errors
/// When `start` is not an http(s) URL or nothing could be fetched.
// The four steps of the ladder read best in one place.
#[allow(clippy::too_many_lines)]
pub fn crawl(
    start: &str,
    options: &Options,
    progress: &(dyn Fn(usize, usize, &str) + Sync),
) -> Result<Crawl> {
    let start = Url::parse(start).map_err(|e| Error::new(format!("{start}: {e}")))?;
    if !matches!(start.scheme(), "http" | "https") {
        return Err(Error::new(format!(
            "{start}: only http and https addresses are fetched"
        )));
    }
    let scope = if options.scope.is_empty() {
        default_scope(&start)
    } else {
        options.scope.clone()
    };
    // Scheme, host and port.
    let origin = start.origin().ascii_serialization();
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(options.timeout.max(1))))
        .http_status_as_error(false)
        .user_agent(user_agent())
        .max_redirects(5)
        .build();
    let agent: ureq::Agent = config.into();
    let mut client = Client {
        agent,
        robots: None,
        max_bytes: options.max_bytes,
        next: Mutex::new(Instant::now()),
        delay: Duration::from_millis(100),
    };
    let mut crawl = Crawl::default();
    if let Ok((200, body, _)) = client.get(&format!("{origin}/robots.txt"))
        && let Ok(robot) = texting_robots::Robot::new("KAPU", &body)
    {
        if let Some(d) = robot.delay {
            client.delay = Duration::from_secs_f32(d.clamp(0.0, 30.0));
        }
        client.robots = Some(robot);
    }
    let in_scope = |u: &str| u.starts_with(&scope) || format!("{u}/") == scope;

    // 1. llms.txt
    let mut llms_candidates = vec![format!("{}llms.txt", scope), format!("{origin}/llms.txt")];
    llms_candidates.dedup();
    for candidate in &llms_candidates {
        let Ok((200, body, kind)) = client.get(candidate) else {
            continue;
        };
        if kind.contains("html") {
            continue;
        }
        let text = String::from_utf8_lossy(&body);
        let entries: Vec<(String, String, String)> = llms_links(&text, candidate)
            .into_iter()
            .filter(|(u, _, _)| in_scope(u))
            .take(options.max_pages)
            .collect();
        if entries.len() >= 2 {
            crawl.method = "llms.txt".into();
            let pages = fetch_all(
                &client,
                &entries,
                options.parallel,
                progress,
                &mut crawl.warnings,
            );
            crawl.pages = pages;
            return finish(crawl);
        }
    }
    // 2. llms-full.txt
    for candidate in [
        format!("{scope}llms-full.txt"),
        format!("{origin}/llms-full.txt"),
    ] {
        let Ok((200, body, kind)) = client.get(&candidate) else {
            continue;
        };
        if kind.contains("html") || body.len() < 200 {
            continue;
        }
        crawl.method = "llms-full.txt".into();
        crawl.pages.push(Page {
            url: candidate,
            bytes: body,
            kind: PageKind::Markdown,
            ..Page::default()
        });
        return finish(crawl);
    }
    // 3. sitemap
    let mut sitemaps: Vec<String> = client
        .robots
        .as_ref()
        .map(|r| r.sitemaps.clone())
        .unwrap_or_default();
    sitemaps.push(format!("{origin}/sitemap.xml"));
    sitemaps.dedup();
    let mut urls: BTreeSet<String> = BTreeSet::new();
    let mut queue: VecDeque<(String, usize)> = sitemaps.into_iter().map(|s| (s, 0)).collect();
    let mut seen_maps = BTreeSet::new();
    while let Some((map, depth)) = queue.pop_front() {
        if depth > 2 || !seen_maps.insert(map.clone()) || seen_maps.len() > 50 {
            continue;
        }
        let Ok((200, body, _)) = client.get(&map) else {
            continue;
        };
        let xml = String::from_utf8_lossy(&body);
        for loc in locs(&xml) {
            let lower = loc.to_lowercase();
            if std::path::Path::new(&lower)
                .extension()
                .is_some_and(|e| e == "xml")
                || lower.ends_with(".xml.gz")
            {
                queue.push_back((loc, depth + 1));
            } else if let Some(u) = normalise(&loc)
                && in_scope(&u)
            {
                urls.insert(u);
            }
        }
    }
    if urls.len() >= 2 {
        crawl.method = "sitemap".into();
        let entries: Vec<(String, String, String)> = urls
            .into_iter()
            .take(options.max_pages)
            .map(|u| (u, String::new(), String::new()))
            .collect();
        crawl.pages = fetch_all(
            &client,
            &entries,
            options.parallel,
            progress,
            &mut crawl.warnings,
        );
        return finish(crawl);
    }
    // 4. crawl
    crawl.method = "crawl".into();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut frontier: VecDeque<String> = VecDeque::new();
    if let Some(u) = normalise(start.as_str()) {
        seen.insert(u.clone());
        frontier.push_back(u);
    }
    while !frontier.is_empty() && crawl.pages.len() < options.max_pages {
        let batch: Vec<(String, String, String)> = std::iter::from_fn(|| frontier.pop_front())
            .take(options.parallel.max(1) * 4)
            .map(|u| (u, String::new(), String::new()))
            .collect();
        let pages = fetch_all(
            &client,
            &batch,
            options.parallel,
            &|_, _, u| progress(seen.len(), 0, u),
            &mut crawl.warnings,
        );
        for page in pages {
            if page.kind == PageKind::Html {
                for link in html_links(&page.bytes, &page.url) {
                    if in_scope(&link)
                        && seen.len() < options.max_pages * 2
                        && seen.insert(link.clone())
                    {
                        frontier.push_back(link);
                    }
                }
            }
            crawl.pages.push(page);
            if crawl.pages.len() >= options.max_pages {
                break;
            }
        }
    }
    finish(crawl)
}

fn finish(mut crawl: Crawl) -> Result<Crawl> {
    if crawl.pages.is_empty() {
        let why = crawl
            .warnings
            .first()
            .cloned()
            .unwrap_or_else(|| "no pages".into());
        return Err(Error::new(format!("nothing fetched: {why}")));
    }
    crawl.pages.sort_by(|a, b| a.url.cmp(&b.url));
    crawl.pages.dedup_by(|a, b| a.url == b.url);
    Ok(crawl)
}

impl Client {
    /// GET `url`: status, body, content type. Waits for the crawl delay,
    /// retries 429 and 5xx up to three times, refuses what `robots.txt`
    /// forbids.
    fn get(&self, url: &str) -> Result<(u16, Vec<u8>, String)> {
        if let Some(robot) = &self.robots
            && !robot.allowed(url)
        {
            return Err(Error::new(format!("{url}: forbidden by robots.txt")));
        }
        for attempt in 0..4u32 {
            self.wait();
            let mut response = match self.agent.get(url).call() {
                Ok(r) => r,
                Err(e) if attempt < 2 => {
                    self.pause(Duration::from_secs(2u64.pow(attempt + 1)));
                    let _ = e;
                    continue;
                }
                Err(e) => return Err(Error::new(format!("{url}: {e}"))),
            };
            let status = response.status().as_u16();
            if status == 429 || status >= 500 {
                let retry = response
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.trim().parse::<u64>().ok())
                    .unwrap_or(2u64.pow(attempt + 1))
                    .min(120);
                if attempt < 3 {
                    self.pause(Duration::from_secs(retry));
                    continue;
                }
            }
            let final_url = response.get_uri().to_string();
            if let (Ok(a), Ok(b)) = (Url::parse(url), Url::parse(&final_url))
                && a.host_str() != b.host_str()
            {
                return Err(Error::new(format!(
                    "{url}: redirected to another host ({final_url}), not followed"
                )));
            }
            let kind = response
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or_default()
                .to_lowercase();
            let body = response
                .body_mut()
                .with_config()
                .limit(self.max_bytes)
                .read_to_vec()
                .map_err(|e| Error::new(format!("{url}: {e}")))?;
            return Ok((status, body, kind));
        }
        Err(Error::new(format!(
            "{url}: the server kept refusing (429/5xx)"
        )))
    }

    fn wait(&self) {
        let until = {
            let Ok(mut next) = self.next.lock() else {
                return;
            };
            let now = Instant::now();
            let at = (*next).max(now);
            *next = at + self.delay;
            at
        };
        let now = Instant::now();
        if until > now {
            std::thread::sleep(until - now);
        }
    }

    fn pause(&self, d: Duration) {
        if let Ok(mut next) = self.next.lock() {
            *next = (*next).max(Instant::now() + d);
        }
    }
}

/// Fetches `entries` (URL, title, description) on `parallel` threads.
fn fetch_all(
    client: &Client,
    entries: &[(String, String, String)],
    parallel: usize,
    progress: &(dyn Fn(usize, usize, &str) + Sync),
    warnings: &mut Vec<String>,
) -> Vec<Page> {
    let done = std::sync::atomic::AtomicUsize::new(0);
    let total = entries.len();
    let results =
        crate::readers::ocr::parallel(entries, parallel.max(1), |(url, title, description)| {
            let r = client.get(url);
            let n = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            progress(n, total, url);
            match r {
                Ok((200..=299, bytes, kind)) => {
                    let markdown = kind.contains("markdown")
                        || kind.starts_with("text/plain")
                        || std::path::Path::new(url.split(['?', '#']).next().unwrap_or(url))
                            .extension()
                            .is_some_and(|e| {
                                e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("txt")
                            });
                    Ok(Page {
                        url: url.clone(),
                        bytes,
                        kind: if markdown {
                            PageKind::Markdown
                        } else {
                            PageKind::Html
                        },
                        title: title.clone(),
                        description: description.clone(),
                    })
                }
                Ok((status, _, _)) if status == 401 || status == 403 => Err(format!(
                    "{url}: {status}, the site refuses automated access or needs a login"
                )),
                Ok((status, _, _)) => Err(format!("{url}: HTTP {status}")),
                Err(e) => Err(e.0),
            }
        });
    let mut pages = Vec::new();
    for r in results {
        match r {
            Ok(p) => pages.push(p),
            Err(w) => warnings.push(w),
        }
    }
    pages
}

/// The `[title](url): description` links of an `llms.txt`, absolute.
#[must_use]
pub fn llms_links(text: &str, base: &str) -> Vec<(String, String, String)> {
    let base = Url::parse(base).ok();
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for line in text.lines() {
        let line = line.trim();
        let Some(open) = line.find('[') else { continue };
        let Some(mid) = line
            .get(open..)
            .and_then(|s| s.find("]("))
            .map(|m| m + open)
        else {
            continue;
        };
        let Some(close) = line
            .get(mid + 2..)
            .and_then(|s| s.find(')'))
            .map(|c| c + mid + 2)
        else {
            continue;
        };
        let title = line
            .get(open + 1..mid)
            .unwrap_or_default()
            .trim()
            .to_owned();
        let target = line.get(mid + 2..close).unwrap_or_default().trim();
        let description = line
            .get(close + 1..)
            .unwrap_or_default()
            .trim_start_matches([':', ' ', '-'])
            .trim()
            .to_owned();
        let url = match &base {
            Some(b) => b.join(target).ok().map(|u| u.to_string()),
            None => Url::parse(target).ok().map(|u| u.to_string()),
        };
        let Some(url) = url.and_then(|u| normalise(&u)) else {
            continue;
        };
        if seen.insert(url.clone()) {
            out.push((url, title, description));
        }
    }
    out
}

/// The `<loc>` entries of a sitemap.
fn locs(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(i) = rest.find("<loc>") {
        let after = rest.get(i + 5..).unwrap_or_default();
        let Some(j) = after.find("</loc>") else { break };
        let loc = after.get(..j).unwrap_or_default().trim();
        let loc = loc.trim_start_matches("<![CDATA[").trim_end_matches("]]>");
        out.push(loc.replace("&amp;", "&"));
        rest = after.get(j..).unwrap_or_default();
    }
    out
}

/// An http(s) URL without fragment and tracking parameters.
#[must_use]
pub fn normalise(url: &str) -> Option<String> {
    let mut u = Url::parse(url).ok()?;
    if !matches!(u.scheme(), "http" | "https") {
        return None;
    }
    u.set_fragment(None);
    let kept: Vec<(String, String)> = u
        .query_pairs()
        .filter(|(k, _)| {
            !k.starts_with("utm_") && !matches!(k.as_ref(), "ref" | "fbclid" | "gclid")
        })
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    if kept.is_empty() {
        u.set_query(None);
    } else {
        u.query_pairs_mut().clear().extend_pairs(kept);
    }
    Some(u.to_string())
}

/// The links of an HTML page, absolute and normalised; assets are left out.
fn html_links(bytes: &[u8], base: &str) -> Vec<String> {
    let Ok(base) = Url::parse(base) else {
        return Vec::new();
    };
    let html = scraper::Html::parse_document(&String::from_utf8_lossy(bytes));
    let Ok(sel) = scraper::Selector::parse("a[href]") else {
        return Vec::new();
    };
    let skip = [
        "png", "jpg", "jpeg", "gif", "svg", "webp", "ico", "css", "js", "zip", "gz", "mp4", "mp3",
        "woff", "woff2", "ttf", "exe", "msi", "dmg", "pdf",
    ];
    let mut out = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for a in html.select(&sel) {
        let Some(href) = a.value().attr("href") else {
            continue;
        };
        let Some(u) = base.join(href).ok().and_then(|u| normalise(u.as_str())) else {
            continue;
        };
        let path = u.split(['?', '#']).next().unwrap_or(&u).to_lowercase();
        if skip.iter().any(|e| path.ends_with(&format!(".{e}"))) {
            continue;
        }
        if seen.insert(u.clone()) {
            out.push(u);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_is_the_first_segment() {
        let u = Url::parse("https://elevenlabs.io/docs/overview/intro").ok();
        assert_eq!(
            u.as_ref().map(default_scope).as_deref(),
            Some("https://elevenlabs.io/docs/")
        );
        let u = Url::parse("https://docs.python.org/3/library/asyncio.html").ok();
        assert_eq!(
            u.as_ref().map(default_scope).as_deref(),
            Some("https://docs.python.org/3/")
        );
        let u = Url::parse("https://example.org/").ok();
        assert_eq!(
            u.as_ref().map(default_scope).as_deref(),
            Some("https://example.org/")
        );
        let u = Url::parse("https://example.org/guide.html").ok();
        assert_eq!(
            u.as_ref().map(default_scope).as_deref(),
            Some("https://example.org/")
        );
    }

    #[test]
    fn llms_lines_with_breadcrumbs() {
        let text = "# Docs\n\n- [Intro](https://x.org/docs/intro.md): Start here\n- API > Voices [List voices](/docs/api/voices.md)\n- [Intro](https://x.org/docs/intro.md)\n";
        let links = llms_links(text, "https://x.org/docs/llms.txt");
        assert_eq!(links.len(), 2);
        assert_eq!(links.first().map(|l| l.2.as_str()), Some("Start here"));
        assert_eq!(
            links.get(1).map(|l| l.0.as_str()),
            Some("https://x.org/docs/api/voices.md")
        );
    }

    #[test]
    fn tracking_parameters_go() {
        assert_eq!(
            normalise("https://x.org/a?utm_source=b&page=2#top").as_deref(),
            Some("https://x.org/a?page=2")
        );
        assert_eq!(normalise("mailto:a@b"), None);
    }

    #[test]
    fn sitemap_locations() {
        let xml = "<urlset><url><loc>https://x.org/a</loc></url><url><loc> <![CDATA[https://x.org/b?x=1&amp;y=2]]> </loc></url></urlset>";
        assert_eq!(locs(xml), ["https://x.org/a", "https://x.org/b?x=1&y=2"]);
    }
}
