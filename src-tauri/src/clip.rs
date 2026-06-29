//! clip.rs — clip a web page into a note: its title, the URL as a `ref`, and the
//! extracted readable text.
//!
//! Same thesis as every capture feature: it creates a note, never an edge. Connecting
//! a clip stays in the no-autocomplete, justified `linker` flow. It reuses the #18d
//! paper-note helper ([`bibtex::note_body`] over a [`bibtex::Source`]) so a clipped page
//! and an imported paper produce the same structured note — the URL becomes a ref, the
//! extracted text becomes the marked-as-imported reference block.
//!
//! Pure where it counts: title extraction and HTML→text are string-in/string-out and
//! tested. [`fetch`] is the lone network seam, called by the (untested) command layer.

use crate::bibtex::{Captured, Source};
use anyhow::{Context, Result};
use std::time::Duration;

/// Cap on the extracted body so clipping a huge page yields a note, not a wall of text.
const MAX_TEXT: usize = 8000;

const USER_AGENT: &str = "local-roam/0.1 (research notebook; web clip)";

/// Fetch a URL's HTML. Network seam — kept thin and untested.
pub fn fetch(url: &str) -> Result<String> {
    ureq::get(url)
        .set("User-Agent", USER_AGENT)
        .timeout(Duration::from_secs(20))
        .call()
        .with_context(|| format!("failed to fetch {url}"))?
        .into_string()
        .with_context(|| format!("failed to read {url}"))
}

/// Turn fetched HTML and its source URL into an importable [`Source`], reusing the
/// paper-note shape: title (extracted or derived from the URL), the URL as the only ref,
/// and the readable text as the imported "Clipped text" block.
pub fn to_source(html: &str, url: &str) -> Source {
    let title = extract_title(html).unwrap_or_else(|| title_from_url(url));
    let text = readable_text(html);
    Source {
        title,
        authors: vec![],
        year: None,
        refs: vec![url.to_string()],
        tags: vec!["clip".into(), "web".into()],
        captured: if text.is_empty() {
            None
        } else {
            Some(Captured { heading: "Clipped text".into(), text })
        },
    }
}

/// The page's `<title>`, or its first `<h1>` as a fallback. `None` if neither is present.
fn extract_title(html: &str) -> Option<String> {
    tag_text(html, "title")
        .or_else(|| tag_text(html, "h1"))
        .map(|t| collapse_ws(&unescape(&t)))
        .filter(|t| !t.is_empty())
}

/// A readable title derived from the URL when the page carries none: the last non-empty
/// path segment (de-slugged), else the host.
fn title_from_url(url: &str) -> String {
    let without_scheme = url.splitn(2, "://").last().unwrap_or(url);
    let (host, path) = match without_scheme.split_once('/') {
        Some((h, p)) => (h, p),
        None => (without_scheme, ""),
    };
    let segment = path
        .split(['?', '#'])
        .next()
        .unwrap_or("")
        .trim_end_matches('/')
        .rsplit('/')
        .find(|s| !s.is_empty());
    match segment {
        Some(s) => {
            let cleaned = s.trim_end_matches(".html").trim_end_matches(".htm");
            collapse_ws(&cleaned.replace(['-', '_'], " "))
        }
        None => host.to_string(),
    }
}

/// Extract readable text from HTML: drop `<script>`/`<style>`/`<head>` content, strip
/// every remaining tag, decode entities, collapse blank runs, and cap the length.
fn readable_text(html: &str) -> String {
    let stripped = strip_elements(html, &["script", "style", "head", "nav", "footer"]);
    let mut text = String::with_capacity(stripped.len());
    let mut in_tag = false;
    for c in stripped.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => text.push(c),
            _ => {}
        }
    }
    let text = unescape(&text);
    // Collapse runs of blank lines and trim each line; keep paragraph breaks.
    let mut lines: Vec<&str> = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() {
            if matches!(lines.last(), Some(&"")) {
                continue;
            }
            lines.push("");
        } else {
            lines.push(t);
        }
    }
    let mut out = lines.join("\n").trim().to_string();
    if out.chars().count() > MAX_TEXT {
        let cut: String = out.chars().take(MAX_TEXT).collect();
        out = format!("{cut}…");
    }
    out
}

/// Remove `<el>…</el>` blocks (and self-contained `<el …/>`), case-insensitively, for
/// each named element.
fn strip_elements(html: &str, elements: &[&str]) -> String {
    let mut out = html.to_string();
    for el in elements {
        out = strip_one(&out, el);
    }
    out
}

fn strip_one(html: &str, el: &str) -> String {
    let lower = html.to_lowercase();
    let open = format!("<{el}");
    let close = format!("</{el}>");
    let mut out = String::with_capacity(html.len());
    let mut i = 0;
    while i < html.len() {
        if lower[i..].starts_with(&open) {
            // Find the end of this element (or the rest of the string if unterminated).
            if let Some(rel) = lower[i..].find(&close) {
                i += rel + close.len();
            } else {
                break;
            }
        } else {
            let ch = html[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

/// Inner text of the first `<tag …>…</tag>`, case-insensitive on the tag name.
fn tag_text(html: &str, tag: &str) -> Option<String> {
    let lower = html.to_lowercase();
    let open = format!("<{tag}");
    let start = lower.find(&open)?;
    let gt = html[start..].find('>')? + start + 1;
    let close = format!("</{tag}>");
    let end = lower[gt..].find(&close)? + gt;
    Some(html[gt..end].to_string())
}

fn collapse_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn unescape(s: &str) -> String {
    s.replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = r#"<html>
      <head><title>The Bitter Lesson &amp; Scaling</title><style>.x{color:red}</style></head>
      <body>
        <nav>home about</nav>
        <h1>The Bitter Lesson</h1>
        <p>The biggest lesson is that general methods are most effective.</p>
        <script>track();</script>
        <p>Compute beats cleverness over time.</p>
        <footer>copyright</footer>
      </body>
    </html>"#;

    #[test]
    fn extracts_the_title() {
        assert_eq!(extract_title(PAGE).as_deref(), Some("The Bitter Lesson & Scaling"));
    }

    #[test]
    fn falls_back_to_h1_then_url() {
        let no_title = "<html><body><h1>Just an H1</h1></body></html>";
        assert_eq!(extract_title(no_title).as_deref(), Some("Just an H1"));
        assert_eq!(
            title_from_url("https://example.com/blog/the-bitter-lesson.html"),
            "the bitter lesson"
        );
        assert_eq!(title_from_url("https://example.com/"), "example.com");
    }

    #[test]
    fn readable_text_drops_scripts_styles_and_chrome() {
        let text = readable_text(PAGE);
        assert!(text.contains("general methods are most effective"));
        assert!(text.contains("Compute beats cleverness"));
        assert!(!text.contains("track()"), "script content must be dropped");
        assert!(!text.contains("color:red"), "style content must be dropped");
        assert!(!text.contains("home about"), "nav chrome must be dropped");
        assert!(!text.contains('<'), "no tags should survive");
    }

    #[test]
    fn long_text_is_capped() {
        let big = format!("<body>{}</body>", "word ".repeat(5000));
        let text = readable_text(&big);
        assert!(text.chars().count() <= MAX_TEXT + 1, "should be truncated near the cap");
        assert!(text.ends_with('…'));
    }

    #[test]
    fn to_source_reuses_the_paper_note_shape_without_edges() {
        let src = to_source(PAGE, "https://example.com/bitter-lesson");
        assert_eq!(src.title, "The Bitter Lesson & Scaling");
        assert_eq!(src.refs, vec!["https://example.com/bitter-lesson".to_string()]);
        assert!(src.tags.contains(&"clip".to_string()));
        let cap = src.captured.as_ref().unwrap();
        assert_eq!(cap.heading, "Clipped text");

        let body = crate::bibtex::note_body(&src);
        assert!(body.contains("# The Bitter Lesson & Scaling"));
        assert!(body.contains("## Clipped text"));
        assert!(body.contains("Refs: https://example.com/bitter-lesson"));
        assert!(
            body.contains("Link from memory"),
            "a clip must not become an auto-link shortcut"
        );
    }
}
