//! The web reader against a local test server: `llms.txt`, sitemap and
//! crawl, `robots.txt`, the scope, and a 429 answer. No network.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use kapu_create::web::{Options, crawl};

/// Serves `pages` (path to content type and body) on a free port; a path
/// listed in `busy_once` answers 429 the first time.
fn serve(pages: HashMap<&'static str, (&'static str, String)>, busy_once: &'static str) -> String {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let port = server.server_addr().to_ip().unwrap().port();
    let hits = Arc::new(AtomicUsize::new(0));
    std::thread::spawn(move || {
        for request in server.incoming_requests() {
            let path = request.url().split('?').next().unwrap_or("/").to_owned();
            if path == busy_once && hits.fetch_add(1, Ordering::Relaxed) == 0 {
                let r = tiny_http::Response::from_string("slow down")
                    .with_status_code(429)
                    .with_header(tiny_http::Header::from_bytes("Retry-After", "1").unwrap());
                let _ = request.respond(r);
                continue;
            }
            match pages.get(path.as_str()) {
                Some((kind, body)) => {
                    let r = tiny_http::Response::from_string(body.clone())
                        .with_header(tiny_http::Header::from_bytes("Content-Type", *kind).unwrap());
                    let _ = request.respond(r);
                }
                None => {
                    let _ = request
                        .respond(tiny_http::Response::from_string("missing").with_status_code(404));
                }
            }
        }
    });
    format!("http://127.0.0.1:{port}")
}

const HTML: &str = "text/html; charset=utf-8";
const MD: &str = "text/markdown";

fn page(title: &str, links: &[&str]) -> String {
    let links = links.iter().fold(String::new(), |mut out, l| {
        let _ = write!(out, "<a href=\"{l}\">{l}</a> ");
        out
    });
    format!(
        "<html><head><title>{title}</title></head><body><nav>menu</nav><main><h1>{title}</h1><p>Text of {title}, long enough to be the main part of the page.</p><p>{links}</p></main></body></html>"
    )
}

fn quiet(_: usize, _: usize, _: &str) {}

#[test]
fn llms_txt_pages_come_first() {
    let mut pages = HashMap::new();
    pages.insert(
        "/docs/llms.txt",
        ("text/plain", "# Docs\n\n- [Intro](/docs/intro.md): Start\n- [Voices](/docs/voices.md)\n- [Elsewhere](/blog/x.md)\n".to_owned()),
    );
    pages.insert("/docs/intro.md", (MD, "# Intro\n\nHello.".to_owned()));
    pages.insert("/docs/voices.md", (MD, "# Voices\n\nVoices.".to_owned()));
    let base = serve(pages, "/docs/voices.md");
    let c = crawl(&format!("{base}/docs/intro"), &Options::default(), &quiet).unwrap();
    assert_eq!(c.method, "llms.txt");
    let urls: Vec<&str> = c.pages.iter().map(|p| p.url.as_str()).collect();
    assert_eq!(
        urls,
        [
            format!("{base}/docs/intro.md"),
            format!("{base}/docs/voices.md")
        ],
        "429 retried, /blog out of scope"
    );
    assert_eq!(c.pages[0].description, "Start");
}

#[test]
fn a_crawl_stays_in_scope_and_obeys_robots() {
    let mut pages = HashMap::new();
    pages.insert(
        "/robots.txt",
        (
            "text/plain",
            "User-agent: *\nDisallow: /guide/private\n".to_owned(),
        ),
    );
    pages.insert(
        "/guide/",
        (
            HTML,
            page(
                "Start",
                &[
                    "a.html",
                    "/guide/b.html",
                    "/guide/private.html",
                    "/shop/",
                    "https://other.example/x",
                ],
            ),
        ),
    );
    pages.insert(
        "/guide/a.html",
        (HTML, page("A", &["b.html#part", "a.html?utm_source=x"])),
    );
    pages.insert("/guide/b.html", (HTML, page("B", &["/guide/"])));
    pages.insert("/guide/private.html", (HTML, page("Private", &[])));
    pages.insert("/shop/", (HTML, page("Shop", &[])));
    let base = serve(pages, "");
    let c = crawl(&format!("{base}/guide/"), &Options::default(), &quiet).unwrap();
    assert_eq!(c.method, "crawl");
    let mut urls: Vec<String> = c
        .pages
        .iter()
        .map(|p| p.url.trim_start_matches(&base).to_owned())
        .collect();
    urls.sort();
    assert_eq!(urls, ["/guide/", "/guide/a.html", "/guide/b.html"]);
    assert!(
        c.warnings.iter().any(|w| w.contains("robots.txt")),
        "{:?}",
        c.warnings
    );
}

#[test]
fn a_sitemap_lists_the_pages() {
    let mut pages = HashMap::new();
    let base_placeholder = "BASE";
    pages.insert("/sitemap.xml", ("application/xml", format!("<urlset><url><loc>{base_placeholder}/kb/one</loc></url><url><loc>{base_placeholder}/kb/two</loc></url><url><loc>{base_placeholder}/news/x</loc></url></urlset>")));
    pages.insert("/kb/one", (HTML, page("One", &[])));
    pages.insert("/kb/two", (HTML, page("Two", &[])));
    // The sitemap needs the server's own address: serve once to learn it.
    let probe = serve(HashMap::new(), "");
    drop(probe);
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let port = server.server_addr().to_ip().unwrap().port();
    let base = format!("http://127.0.0.1:{port}");
    let pages: HashMap<&'static str, (&'static str, String)> = pages
        .into_iter()
        .map(|(k, (t, b))| (k, (t, b.replace(base_placeholder, &base))))
        .collect();
    std::thread::spawn(move || {
        for request in server.incoming_requests() {
            let path = request.url().to_owned();
            let r = match pages.get(path.as_str()) {
                Some((kind, body)) => tiny_http::Response::from_string(body.clone())
                    .with_header(tiny_http::Header::from_bytes("Content-Type", *kind).unwrap()),
                None => tiny_http::Response::from_string("missing").with_status_code(404),
            };
            let _ = request.respond(r);
        }
    });
    let c = crawl(&format!("{base}/kb/one"), &Options::default(), &quiet).unwrap();
    assert_eq!(c.method, "sitemap");
    assert_eq!(c.pages.len(), 2, "/news is out of scope");
}
