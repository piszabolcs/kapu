//! Room text to HTML for the reader pane.
//!
//! The text comes from a `.kapu` file, so it is untrusted: raw HTML is shown as
//! text, images become their alt text, and only `http`, `https` and `mailto` links
//! survive. A link to a door (`[x](#room/sub)`, FR-11/1) becomes an in-app link
//! (`#room=<id>`). Sub-door headings get the sub-door id as their anchor.

use pulldown_cmark::{CowStr, Event, HeadingLevel, Options, Parser, Tag, TagEnd, html};

/// Renders `text` as HTML. `is_door` tells whether an id names a door of the open
/// file; `subs` are the ids of the sub-doors, in body order, for the `##` headings.
#[must_use]
pub fn render(text: &str, is_door: &dyn Fn(&str) -> bool, subs: &[String]) -> String {
    let options =
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let mut dropped_link = false;
    let mut in_image = false;
    let mut next_sub = subs.iter();
    let events = Parser::new_ext(text, options).map(|event| match event {
        Event::Html(raw) | Event::InlineHtml(raw) => Event::Text(raw),
        Event::Start(Tag::Link {
            link_type,
            dest_url,
            title,
            id,
        }) => {
            if let Some(href) = safe_href(&dest_url, is_door) {
                Event::Start(Tag::Link {
                    link_type,
                    dest_url: href,
                    title,
                    id,
                })
            } else {
                dropped_link = true;
                Event::Start(Tag::Emphasis)
            }
        }
        Event::End(TagEnd::Link) if dropped_link => {
            dropped_link = false;
            Event::End(TagEnd::Emphasis)
        }
        // An image shows its alt text only; the reader loads nothing from outside.
        Event::Start(Tag::Image { .. }) => {
            in_image = true;
            Event::Start(Tag::Emphasis)
        }
        Event::End(TagEnd::Image) if in_image => {
            in_image = false;
            Event::End(TagEnd::Emphasis)
        }
        Event::Start(Tag::Heading {
            level: HeadingLevel::H2,
            classes,
            attrs,
            ..
        }) => Event::Start(Tag::Heading {
            level: HeadingLevel::H2,
            id: next_sub.next().map(|s| CowStr::from(format!("door-{s}"))),
            classes,
            attrs,
        }),
        other => other,
    });
    let mut out = String::with_capacity(text.len() * 3 / 2);
    html::push_html(&mut out, events);
    out
}

fn safe_href<'a>(url: &str, is_door: &dyn Fn(&str) -> bool) -> Option<CowStr<'a>> {
    if let Some(id) = url.strip_prefix('#') {
        return is_door(id).then(|| CowStr::from(format!("#room={id}")));
    }
    let lower = url.to_ascii_lowercase();
    ["https://", "http://", "mailto:"]
        .iter()
        .any(|scheme| lower.starts_with(scheme))
        .then(|| CowStr::from(url.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn door(id: &str) -> bool {
        id == "a/b"
    }

    #[test]
    fn untrusted_markup_is_shown_as_text() {
        let html = render(
            "<script>x()</script>\n\n![logo](https://x/y.png) [bad](javascript:alert(1))",
            &door,
            &[],
        );
        assert!(!html.contains("<script"), "{html}");
        assert!(html.contains("&lt;script&gt;"), "{html}");
        assert!(!html.contains("<img"), "{html}");
        assert!(!html.contains("javascript:"), "{html}");
        assert!(html.contains("<em>logo</em>"), "{html}");
        assert!(html.contains("<em>bad</em>"), "{html}");
    }

    #[test]
    fn door_links_stay_in_the_app_and_headings_get_sub_door_anchors() {
        let html = render(
            "[go](#a/b) [gone](#x) [web](https://example.com)\n\n## One\n\n## Two\n",
            &door,
            &["a/one".to_owned(), "a/two".to_owned()],
        );
        assert!(html.contains(r##"<a href="#room=a/b">go</a>"##), "{html}");
        assert!(html.contains("<em>gone</em>"), "{html}");
        assert!(
            html.contains(r#"<a href="https://example.com">web</a>"#),
            "{html}"
        );
        assert!(html.contains(r#"<h2 id="door-a/one">One</h2>"#), "{html}");
        assert!(html.contains(r#"<h2 id="door-a/two">Two</h2>"#), "{html}");
    }
}
