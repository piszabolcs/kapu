//! Mails (KD7): EML with `mail-parser`, Outlook MSG with `msg_parser`. The
//! subject is the title, sender, recipients and date are the first lines,
//! the body is the text; attachments are read as their own documents and
//! linked from the mail.

use crate::doc::{Block, Document, Kind};
use crate::readers::archive::failed;
use crate::readers::{Ctx, html, read, text};
use crate::{Error, Result};

/// The parts of a mail both readers give.
struct Mail {
    subject: String,
    from: String,
    to: String,
    date: String,
    text: String,
    html: String,
    attachments: Vec<(String, Vec<u8>)>,
}

/// An `.eml` file.
///
/// # Errors
/// When it is not a mail.
pub fn eml(bytes: &[u8], key: &str, name: &str, ctx: &Ctx<'_>) -> Result<Vec<Document>> {
    let message = mail_parser::MessageParser::default()
        .parse(bytes)
        .ok_or_else(|| Error::new("not a readable mail"))?;
    let addresses = |list: Option<mail_parser::AddressList<'_>>| {
        list.map(|l| {
            l.mailboxes()
                .map(|m| match (m.name(), m.address()) {
                    (Some(n), Some(a)) => format!("{n} <{a}>"),
                    (Some(n), None) => n.to_owned(),
                    (None, Some(a)) => a.to_owned(),
                    (None, None) => String::new(),
                })
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default()
    };
    let attachments = message
        .attachments()
        .enumerate()
        .map(|(i, part)| {
            let file = part
                .attachment_name()
                .map_or_else(|| format!("attachment-{}", i + 1), str::to_owned);
            (file, part.decoded().into_owned())
        })
        .collect();
    let mail = Mail {
        subject: message.subject().unwrap_or_default().to_owned(),
        from: addresses(message.from()),
        to: addresses(message.to()),
        date: message.date().map(|d| d.to_rfc3339()).unwrap_or_default(),
        text: message
            .body_text(0)
            .map(std::borrow::Cow::into_owned)
            .unwrap_or_default(),
        html: message
            .body_html(0)
            .map(std::borrow::Cow::into_owned)
            .unwrap_or_default(),
        attachments,
    };
    Ok(documents(&mail, key, name, ctx))
}

/// An Outlook `.msg` file.
///
/// # Errors
/// When it cannot be parsed.
pub fn msg(bytes: &[u8], key: &str, name: &str, ctx: &Ctx<'_>) -> Result<Vec<Document>> {
    let message =
        msg_parser::Outlook::from_slice(bytes).map_err(|e| Error::new(format!("MSG: {e}")))?;
    let person = |p: &msg_parser::Person| match (p.name.is_empty(), p.email.is_empty()) {
        (false, false) => format!("{} <{}>", p.name, p.email),
        (false, true) => p.name.clone(),
        _ => p.email.clone(),
    };
    let date = if message.client_submit_time.is_empty() {
        message.message_delivery_time.clone()
    } else {
        message.client_submit_time.clone()
    };
    let attachments = message
        .attachments
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let file = [&a.long_file_name, &a.display_name, &a.file_name]
                .into_iter()
                .find(|n| !n.trim().is_empty())
                .cloned()
                .unwrap_or_else(|| format!("attachment-{}{}", i + 1, a.extension));
            let file = if a.attach_method == 5 && !file.contains('.') {
                format!("{file}.msg")
            } else {
                file
            };
            (file, a.payload_bytes.clone())
        })
        .collect();
    let mail = Mail {
        subject: message.subject.clone(),
        from: person(&message.sender),
        to: message.to.iter().map(person).collect::<Vec<_>>().join(", "),
        date,
        text: message.body.clone(),
        html: message.html.clone(),
        attachments,
    };
    Ok(documents(&mail, key, name, ctx))
}

fn documents(mail: &Mail, key: &str, name: &str, ctx: &Ctx<'_>) -> Vec<Document> {
    let mut doc = if mail.text.trim().is_empty() && !mail.html.trim().is_empty() {
        let page = html::convert(&mail.html);
        text::from_markdown(&page.markdown, key, name, "mail")
    } else {
        let mut d = text::plain(mail.text.as_bytes(), key, name);
        d.reader = "mail".into();
        d
    };
    if !mail.subject.trim().is_empty() {
        mail.subject.trim().clone_into(&mut doc.title);
    }
    let mut head = Vec::new();
    for (label, value) in [("From", &mail.from), ("To", &mail.to), ("Date", &mail.date)] {
        if !value.trim().is_empty() {
            head.push(format!("{label}: {}", value.trim()));
            doc.meta.push((label.to_owned(), value.trim().to_owned()));
        }
    }
    if !head.is_empty() {
        doc.blocks
            .insert(0, Block::new(Kind::Paragraph, head.join("  \n")));
    }
    let mut out = Vec::new();
    let mut links = Vec::new();
    for (file, bytes) in &mail.attachments {
        let inner_key = format!("{key}!{file}");
        links.push(format!(
            "- [{}](<kapu-key:{inner_key}>)",
            file.replace(['[', ']'], "")
        ));
        match ctx
            .deeper()
            .and_then(|deeper| read(bytes, &inner_key, &format!("{name} > {file}"), &deeper))
        {
            Ok(docs) => out.extend(docs),
            Err(e) => out.push(failed(&inner_key, &format!("{name} > {file}"), &e.0)),
        }
    }
    if !links.is_empty() {
        doc.blocks.push(Block::new(
            Kind::List,
            format!("Attachments:\n\n{}", links.join("\n")),
        ));
    }
    out.insert(0, doc);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::readers::Options;

    #[test]
    fn an_eml_with_an_attachment() {
        let raw = "From: Ann <ann@example.com>\r\nTo: bob@example.com\r\nSubject: Motor settings\r\nDate: Mon, 2 Nov 2026 10:00:00 +0100\r\nMIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=\"b\"\r\n\r\n--b\r\nContent-Type: text/plain\r\n\r\nThe gain is 12.\r\n--b\r\nContent-Type: text/markdown\r\nContent-Disposition: attachment; filename=\"notes.md\"\r\n\r\n# Notes\r\n\r\nSee the gain.\r\n--b--\r\n";
        let options = Options::default();
        let docs = eml(
            raw.as_bytes(),
            "m.eml",
            "m.eml",
            &Ctx {
                options: &options,
                depth: 0,
            },
        )
        .unwrap_or_default();
        assert_eq!(docs.len(), 2);
        let mail = docs.first().cloned().unwrap_or_default();
        assert_eq!(mail.title, "Motor settings");
        let text: String = mail
            .blocks
            .iter()
            .map(|b| b.text.clone())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("From: Ann <ann@example.com>"), "{text}");
        assert!(text.contains("The gain is 12."));
        assert!(text.contains("[notes.md](<kapu-key:m.eml!notes.md>)"));
        assert_eq!(docs.get(1).map(|d| d.title.as_str()), Some("Notes"));
    }
}
