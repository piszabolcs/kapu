//! HTML: the main content (`main`, `article`), without menus, headers and
//! footers, to Markdown with `htmd`, then the Markdown reader (plan L1/3).

use scraper::{ElementRef, Html, Selector};

use crate::doc::Document;
use crate::readers::text::from_markdown;
use crate::text::{decode, squash};

/// Tags whose content never reaches a room.
const SKIP: &[&str] = &[
    "script", "style", "noscript", "svg", "template", "iframe", "nav", "aside", "footer",
    "form", "button", "select", "input", "canvas", "dialog",
];

/// What an HTML page gives.
#[derive(Debug, Default)]
pub struct Page {
    /// The `h1` of the main content, else `<title>`.
    pub title: String,
    /// The meta description.
    pub description: String,
    /// The main content as Markdown.
    pub markdown: String,
    /// Set when the page looks like a JavaScript application with no text
    /// in its HTML (plan L1/4).
    pub script_only: bool,
}

/// An HTML file or a fetched page.
#[must_use]
pub fn read(bytes: &[u8], key: &str, name: &str) -> Document {
    let page = convert(&decode(bytes));
    let mut doc = from_markdown(&page.markdown, key, name, "html");
    if !page.title.is_empty() {
        doc.title = page.title;
    }
    if doc.description.is_empty() {
        doc.description = page.description;
    }
    if page.script_only {
        doc.warnings.push(
            "the page has almost no text in its HTML: it is probably built by JavaScript, which is not run".into(),
        );
    }
    doc
}

fn selector(css: &str) -> Option<Selector> {
    Selector::parse(css).ok()
}

fn first<'a>(html: &'a Html, css: &str) -> Option<ElementRef<'a>> {
    html.select(&selector(css)?).next()
}

fn text_of(e: ElementRef<'_>) -> String {
    squash(&e.text().collect::<String>())
}

/// Visible text length of an element, without scripts and styles.
fn visible_len(e: ElementRef<'_>) -> usize {
    let mut n = 0;
    for node in e.descendants() {
        if let Some(t) = node.value().as_text() {
            let hidden = node.ancestors().any(|a| {
                a.value()
                    .as_element()
                    .is_some_and(|el| matches!(el.name(), "script" | "style" | "noscript" | "template"))
            });
            if !hidden {
                n += t.trim().len();
            }
        }
    }
    n
}

/// The parts of a page the maker needs.
#[must_use]
pub fn convert(source: &str) -> Page {
    let html = Html::parse_document(source);
    let mut page = Page::default();
    let meta = |css: &str| {
        first(&html, css)
            .and_then(|e| e.value().attr("content"))
            .map(squash)
            .filter(|s| !s.is_empty())
    };
    page.description = meta("meta[name=description]")
        .or_else(|| meta("meta[property='og:description']"))
        .unwrap_or_default();
    let body = first(&html, "body");
    let body_len = body.map_or(0, visible_len);
    let mut main = None;
    for css in ["main", "[role=main]", "article", "#content", ".content", ".markdown-body"] {
        if let Some(e) = first(&html, css) {
            if visible_len(e) * 10 >= body_len * 3 && visible_len(e) > 0 {
                main = Some(e);
                break;
            }
        }
    }
    let (root, skip_header) = match (main, body) {
        (Some(m), _) => (Some(m), false),
        (None, Some(b)) => (Some(b), true),
        (None, None) => (None, false),
    };
    page.title = root
        .and_then(|r| r.select(&selector("h1")?).next())
        .map(text_of)
        .filter(|t| !t.is_empty())
        .or_else(|| first(&html, "title").map(text_of).filter(|t| !t.is_empty()))
        .unwrap_or_default();
    let inner = root.map_or_else(|| source.to_owned(), |r| r.inner_html());
    let mut skip: Vec<&str> = SKIP.to_vec();
    if skip_header {
        skip.push("header");
    }
    page.markdown = htmd::HtmlToMarkdown::builder()
        .skip_tags(skip)
        .build()
        .convert(&inner)
        .unwrap_or_default();
    let scripts = selector("script").map_or(0, |s| html.select(&s).count());
    page.script_only = body_len < 200 && scripts > 0;
    page
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn main_content_without_menus() {
        let html = r#"<!doctype html><html><head><title>Site | Page</title>
            <meta name="description" content="About  things"></head><body>
            <nav><a href="/">Home</a><a href="/x">Menu entry</a></nav>
            <main><h1>Real title</h1><p>The real <b>text</b> of the page, long enough to be the main part.</p>
            <h2>Part</h2><pre><code class="language-rust">fn main() {}</code></pre></main>
            <footer>Copyright</footer><script>var x = 1;</script></body></html>"#;
        let p = convert(html);
        assert_eq!(p.title, "Real title");
        assert_eq!(p.description, "About things");
        assert!(p.markdown.contains("The real **text**"), "{}", p.markdown);
        assert!(p.markdown.contains("## Part"));
        assert!(!p.markdown.contains("Menu entry"));
        assert!(!p.markdown.contains("Copyright"));
        assert!(!p.script_only);
    }

    #[test]
    fn a_script_shell_is_flagged() {
        let p = convert(r#"<html><body><div id="root"></div><script src="app.js"></script></body></html>"#);
        assert!(p.script_only);
    }
}
