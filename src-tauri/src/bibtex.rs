//! bibtex.rs — turn a pasted BibTeX entry or an arXiv id/URL into a *paper note*.
//!
//! Importing a citation is capture, and capture is allowed — but it stops at the note.
//! An imported paper carries its `refs` and a body skeleton; it never gets edges. The
//! body always ends with a "Connections" prompt that sends the user back to the
//! no-autocomplete, justified `linker` flow, so connecting an imported paper costs the
//! same recall as connecting anything else.
//!
//! Mostly a pure module: BibTeX parsing, arXiv-id extraction, the Atom-feed parser, and
//! the shared note-body builder are all string-in/string-out and unit-tested. The single
//! impure seam is [`fetch_arxiv`], which the (already untested) command layer calls.
//!
//! [`note_body`] is the shared "paper-note helper": `clip.rs` (#18e) reuses it verbatim
//! so an imported paper and a clipped web page produce the same structured note.

use anyhow::{anyhow, Context, Result};
use std::time::Duration;

/// Verbatim captured text plus the heading to file it under ("Abstract" for a paper,
/// "Clipped text" for a web page). Kept separate from the user's own summary so the
/// imported material never masquerades as something they wrote from memory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Captured {
    pub heading: String,
    pub text: String,
}

/// A captured source: a paper (from BibTeX/arXiv) or — reused by `clip.rs` — a web page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    pub title: String,
    pub authors: Vec<String>,
    pub year: Option<String>,
    /// Citation refs, already prefixed: `arXiv:1706.03762`, `doi:10.…`, or a bare URL.
    pub refs: Vec<String>,
    pub tags: Vec<String>,
    /// Imported reference text (abstract / page body), if any.
    pub captured: Option<Captured>,
}

/// Build the note body for an imported source: a metadata header, the captured text
/// (clearly marked as imported reference material), an empty in-your-own-words summary,
/// and the standard justified-link Connections footer. This is the "paper-note helper".
pub fn note_body(src: &Source) -> String {
    let mut out = format!("# {}\n\n", src.title.trim());

    let who = src.authors.join(", ");
    let header = match (who.is_empty(), &src.year) {
        (false, Some(y)) => Some(format!("{who} ({y})")),
        (false, None) => Some(who),
        (true, Some(y)) => Some(format!("({y})")),
        (true, None) => None,
    };
    if let Some(h) = header {
        out.push_str(&h);
        out.push_str("\n\n");
    }
    if !src.refs.is_empty() {
        out.push_str(&format!("Refs: {}\n\n", src.refs.join(", ")));
    }

    out.push_str(
        "> Imported source — the capture below is reference only. Write the summary in \
         your own words.\n\n",
    );

    if let Some(c) = &src.captured {
        if !c.text.trim().is_empty() {
            out.push_str(&format!("## {}\n{}\n\n", c.heading, c.text.trim()));
        }
    }

    out.push_str("## Summary\nThe core idea, in a paragraph you could explain aloud.\n\n");
    out.push_str("## Why it matters\n\n");
    out.push_str(
        "## Connections\n\
         Link this to the papers and ideas it builds on with \"Link from memory\" \
         — typed from memory, one justified edge at a time.\n",
    );
    out
}

// --- BibTeX -----------------------------------------------------------------------

/// A pasted BibTeX entry starts with `@type{…`. (An arXiv id/URL never does, which is
/// how the command layer tells the two import inputs apart.)
pub fn looks_like_bibtex(input: &str) -> bool {
    input.trim_start().starts_with('@')
}

/// Parse a single pasted BibTeX entry into a paper [`Source`]. Tolerant: unknown fields
/// are ignored, brace/quote-delimited and bare values are all accepted. Errors only when
/// there is no recognisable entry or no title (a note must have a title).
pub fn parse_bibtex(text: &str) -> Result<Source> {
    let at = text.find('@').ok_or_else(|| anyhow!("not a BibTeX entry (no '@')"))?;
    let rest = &text[at + 1..];
    let brace = rest
        .find('{')
        .ok_or_else(|| anyhow!("malformed BibTeX entry (no '{{')"))?;
    let fields = parse_fields(&rest[brace + 1..]);
    let get = |name: &str| fields.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone());

    let title = get("title")
        .map(|t| clean(&t))
        .filter(|t| !t.is_empty())
        .ok_or_else(|| anyhow!("BibTeX entry has no title"))?;

    let authors = get("author")
        .map(|a| split_authors(&a))
        .unwrap_or_default();
    let year = get("year").map(|y| clean(&y)).filter(|y| !y.is_empty());

    let mut refs = Vec::new();
    // arXiv eprint may be under `eprint` (with archivePrefix) or a bare `arxiv` field.
    let eprint = get("eprint").or_else(|| get("arxiv")).map(|e| trim_value(&e));
    if let Some(e) = eprint.filter(|e| !e.is_empty()) {
        refs.push(format!("arXiv:{e}"));
    }
    if let Some(d) = get("doi").map(|d| trim_value(&d)).filter(|d| !d.is_empty()) {
        refs.push(format!("doi:{d}"));
    }
    if let Some(u) = get("url").map(|u| trim_value(&u)).filter(|u| !u.is_empty()) {
        if !refs.iter().any(|r| r == &u) {
            refs.push(u);
        }
    }

    Ok(Source {
        title,
        authors,
        year,
        refs,
        tags: vec!["paper".into()],
        captured: None,
    })
}

/// Extract `name = value` pairs from the body of a BibTeX entry. Driven off `=` signs so
/// the leading citation key (which has none) is skipped. Values may be `{braced}` (with
/// nesting), `"quoted"`, or a bare token ending at a comma. Names are lower-cased.
fn parse_fields(s: &str) -> Vec<(String, String)> {
    let chars: Vec<char> = s.chars().collect();
    let n = chars.len();
    let mut out = Vec::new();
    let mut i = 0;
    while i < n {
        if chars[i] != '=' {
            i += 1;
            continue;
        }
        // Walk left over whitespace, then collect the field name.
        let mut j = i;
        while j > 0 && chars[j - 1].is_whitespace() {
            j -= 1;
        }
        let mut k = j;
        while k > 0 && (chars[k - 1].is_ascii_alphanumeric() || chars[k - 1] == '_') {
            k -= 1;
        }
        let name: String = chars[k..j].iter().collect::<String>().to_lowercase();

        // Walk right over whitespace to the value.
        let mut p = i + 1;
        while p < n && chars[p].is_whitespace() {
            p += 1;
        }
        let (value, next) = match chars.get(p) {
            Some('{') => read_braced(&chars, p),
            Some('"') => read_quoted(&chars, p),
            _ => read_bare(&chars, p),
        };
        if !name.is_empty() {
            out.push((name, value));
        }
        i = next.max(i + 1);
    }
    out
}

/// Read a `{…}` value with balanced nested braces, starting at the opening brace.
/// Returns the inner content and the index just past the closing brace.
fn read_braced(chars: &[char], start: usize) -> (String, usize) {
    let mut depth = 0i32;
    let mut buf = String::new();
    let mut i = start;
    while i < chars.len() {
        match chars[i] {
            '{' => {
                if depth > 0 {
                    buf.push('{');
                }
                depth += 1;
            }
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return (buf, i + 1);
                }
                buf.push('}');
            }
            c => buf.push(c),
        }
        i += 1;
    }
    (buf, i)
}

/// Read a `"…"` value, starting at the opening quote.
fn read_quoted(chars: &[char], start: usize) -> (String, usize) {
    let mut buf = String::new();
    let mut i = start + 1;
    while i < chars.len() {
        if chars[i] == '"' {
            return (buf, i + 1);
        }
        buf.push(chars[i]);
        i += 1;
    }
    (buf, i)
}

/// Read a bare value up to the next comma, closing brace, or newline.
fn read_bare(chars: &[char], start: usize) -> (String, usize) {
    let mut buf = String::new();
    let mut i = start;
    while i < chars.len() && !matches!(chars[i], ',' | '}' | '\n') {
        buf.push(chars[i]);
        i += 1;
    }
    (buf.trim().to_string(), i)
}

/// Collapse internal whitespace/newlines and strip stray braces — for human-facing
/// fields (title, journal). `split_whitespace` also trims.
fn clean(v: &str) -> String {
    v.replace(['{', '}'], "").split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Trim braces/quotes/whitespace without collapsing internal characters — for opaque
/// identifiers (doi, url, eprint) where internal spacing is meaningless but must not be
/// rewritten.
fn trim_value(v: &str) -> String {
    v.trim().trim_matches(['{', '}', '"', ' ']).trim().to_string()
}

/// Split a BibTeX `author` field on " and " and normalise each "Last, First" to
/// "First Last".
fn split_authors(field: &str) -> Vec<String> {
    clean(field)
        .split(" and ")
        .map(|a| a.trim())
        .filter(|a| !a.is_empty())
        .map(|a| match a.split_once(',') {
            Some((last, first)) => format!("{} {}", first.trim(), last.trim()),
            None => a.to_string(),
        })
        .collect()
}

// --- arXiv ------------------------------------------------------------------------

/// Extract a normalised arXiv id from an id, a `arXiv:` reference, or an arxiv.org URL.
/// Returns `None` if the input isn't recognisably an arXiv id.
pub fn arxiv_id(input: &str) -> Option<String> {
    let mut s = input.trim();
    // arxiv.org/abs/<id>, /pdf/<id>, /<id>
    if let Some(pos) = s.to_lowercase().find("arxiv.org/") {
        s = &s[pos + "arxiv.org/".len()..];
        for prefix in ["abs/", "pdf/", "html/"] {
            if let Some(stripped) = s.strip_prefix(prefix) {
                s = stripped;
                break;
            }
        }
    }
    // "arXiv:1706.03762" prefix (any case)
    if s.len() >= 6 && s[..6].eq_ignore_ascii_case("arxiv:") {
        s = &s[6..];
    }
    let s = s.trim().trim_end_matches(".pdf");
    // Take just the id token (stop at the first slash-free boundary like '?', '#', space).
    let token = s.split([' ', '?', '#']).next().unwrap_or(s);
    if is_arxiv_id(token) {
        Some(token.to_string())
    } else {
        None
    }
}

/// New-style `NNNN.NNNNN(vN)` or old-style `archive(.SUBJ)/NNNNNNN(vN)`.
fn is_arxiv_id(s: &str) -> bool {
    let base = s.split_once('v').map(|(b, v)| {
        if v.chars().all(|c| c.is_ascii_digit()) && !v.is_empty() {
            b
        } else {
            s
        }
    }).unwrap_or(s);

    // New style: 4 digits, dot, 4-5 digits.
    if let Some((a, b)) = base.split_once('.') {
        if a.len() == 4
            && a.chars().all(|c| c.is_ascii_digit())
            && (4..=5).contains(&b.len())
            && b.chars().all(|c| c.is_ascii_digit())
        {
            return true;
        }
    }
    // Old style: archive/7-digits, e.g. hep-th/9901001 or math.GT/0309136.
    if let Some((archive, num)) = base.split_once('/') {
        let archive_ok = !archive.is_empty()
            && archive
                .chars()
                .all(|c| c.is_ascii_alphabetic() || c == '-' || c == '.');
        if archive_ok && num.len() == 7 && num.chars().all(|c| c.is_ascii_digit()) {
            return true;
        }
    }
    false
}

const USER_AGENT: &str = "local-roam/0.1 (research notebook; capture import)";

/// Fetch a paper's metadata from the arXiv API and parse it into a [`Source`]. Network
/// seam — kept thin; the Atom parsing it delegates to is pure and tested.
pub fn fetch_arxiv(id: &str) -> Result<Source> {
    let url = format!("https://export.arxiv.org/api/query?id_list={id}");
    let body = ureq::get(&url)
        .set("User-Agent", USER_AGENT)
        .timeout(Duration::from_secs(20))
        .call()
        .with_context(|| format!("failed to reach arXiv for {id}"))?
        .into_string()
        .context("failed to read the arXiv response")?;
    parse_arxiv_atom(&body, id)
}

/// Parse the arXiv Atom feed for a single paper into a [`Source`]. Pulls the `<entry>`
/// block (skipping the feed-level title), then its title, authors, summary, and year.
pub fn parse_arxiv_atom(xml: &str, id: &str) -> Result<Source> {
    let entry = extract_tag(xml, "entry")
        .ok_or_else(|| anyhow!("arXiv returned no entry for {id} (unknown id?)"))?;

    let title = extract_tag(&entry, "title")
        .map(|t| clean(&unescape_xml(&t)))
        .filter(|t| !t.is_empty())
        .ok_or_else(|| anyhow!("arXiv entry for {id} has no title"))?;

    let authors = extract_all_tags(&entry, "name")
        .into_iter()
        .map(|a| clean(&unescape_xml(&a)))
        .filter(|a| !a.is_empty())
        .collect();

    let year = extract_tag(&entry, "published")
        .filter(|p| p.len() >= 4)
        .map(|p| p[..4].to_string());

    let summary = extract_tag(&entry, "summary").map(|s| clean(&unescape_xml(&s)));

    let mut refs = vec![format!("arXiv:{id}")];
    if let Some(doi) = extract_tag(&entry, "arxiv:doi").map(|d| trim_value(&d)) {
        if !doi.is_empty() {
            refs.push(format!("doi:{doi}"));
        }
    }

    Ok(Source {
        title,
        authors,
        year,
        refs,
        tags: vec!["paper".into()],
        captured: summary
            .filter(|s| !s.is_empty())
            .map(|s| Captured { heading: "Abstract".into(), text: s }),
    })
}

/// Inner text of the first `<tag>…</tag>` (matched case-sensitively on the exact name).
fn extract_tag(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let start = xml.find(&open)?;
    // Skip past the (possibly attributed) opening tag's '>'.
    let gt = xml[start..].find('>')? + start + 1;
    let end = xml[gt..].find(&close)? + gt;
    Some(xml[gt..end].trim().to_string())
}

/// Inner text of every `<tag>…</tag>` occurrence.
fn extract_all_tags(xml: &str, tag: &str) -> Vec<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(s) = rest.find(&open) {
        let from = s + open.len();
        let Some(e) = rest[from..].find(&close) else { break };
        out.push(rest[from..from + e].trim().to_string());
        rest = &rest[from + e + close.len()..];
    }
    out
}

/// Decode the five predefined XML entities.
fn unescape_xml(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENTRY: &str = r#"@article{vaswani2017attention,
  title = {Attention Is All You Need},
  author = {Vaswani, Ashish and Shazeer, Noam and Parmar, Niki},
  year = {2017},
  eprint = {1706.03762},
  archivePrefix = {arXiv},
  doi = {10.5555/3295222.3295349},
}"#;

    #[test]
    fn parses_a_bibtex_entry() {
        let s = parse_bibtex(ENTRY).unwrap();
        assert_eq!(s.title, "Attention Is All You Need");
        assert_eq!(s.authors, vec!["Ashish Vaswani", "Noam Shazeer", "Niki Parmar"]);
        assert_eq!(s.year.as_deref(), Some("2017"));
        assert!(s.refs.contains(&"arXiv:1706.03762".to_string()));
        assert!(s.refs.contains(&"doi:10.5555/3295222.3295349".to_string()));
        assert_eq!(s.tags, vec!["paper"]);
    }

    #[test]
    fn bibtex_handles_quoted_values_and_nested_braces() {
        let s = parse_bibtex(
            r#"@inproceedings{k, title = "A {Study} of Things", author = {Doe, Jane}, url = {http://x.test/p}}"#,
        )
        .unwrap();
        assert_eq!(s.title, "A Study of Things");
        assert_eq!(s.authors, vec!["Jane Doe"]);
        assert_eq!(s.refs, vec!["http://x.test/p".to_string()]);
    }

    #[test]
    fn bibtex_without_title_errors() {
        assert!(parse_bibtex("@misc{k, author = {Nobody}}").is_err());
    }

    #[test]
    fn looks_like_bibtex_only_for_at_entries() {
        assert!(looks_like_bibtex("  @article{...}"));
        assert!(!looks_like_bibtex("1706.03762"));
        assert!(!looks_like_bibtex("https://arxiv.org/abs/1706.03762"));
    }

    #[test]
    fn extracts_arxiv_ids_from_every_form() {
        assert_eq!(arxiv_id("1706.03762").as_deref(), Some("1706.03762"));
        assert_eq!(arxiv_id("arXiv:1706.03762v2").as_deref(), Some("1706.03762v2"));
        assert_eq!(
            arxiv_id("https://arxiv.org/abs/2310.06825").as_deref(),
            Some("2310.06825")
        );
        assert_eq!(
            arxiv_id("http://arxiv.org/pdf/1706.03762v1.pdf").as_deref(),
            Some("1706.03762v1")
        );
        assert_eq!(arxiv_id("hep-th/9901001").as_deref(), Some("hep-th/9901001"));
    }

    #[test]
    fn rejects_non_arxiv_input() {
        assert!(arxiv_id("not an id").is_none());
        assert!(arxiv_id("https://example.com/article").is_none());
        assert!(arxiv_id("12.34").is_none());
    }

    #[test]
    fn parses_the_arxiv_atom_feed() {
        let xml = r#"<?xml version="1.0"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <title>arXiv Query: 1706.03762</title>
  <entry>
    <title>Attention Is All You Need</title>
    <summary>  The dominant sequence transduction models are based on
complex recurrent networks.  </summary>
    <published>2017-06-12T17:57:34Z</published>
    <author><name>Ashish Vaswani</name></author>
    <author><name>Noam Shazeer</name></author>
  </entry>
</feed>"#;
        let s = parse_arxiv_atom(xml, "1706.03762").unwrap();
        assert_eq!(s.title, "Attention Is All You Need");
        assert_eq!(s.authors, vec!["Ashish Vaswani", "Noam Shazeer"]);
        assert_eq!(s.year.as_deref(), Some("2017"));
        assert_eq!(s.refs, vec!["arXiv:1706.03762".to_string()]);
        let cap = s.captured.unwrap();
        assert_eq!(cap.heading, "Abstract");
        assert!(cap.text.starts_with("The dominant sequence transduction"));
        assert!(!cap.text.contains('\n'), "summary whitespace should be collapsed");
    }

    #[test]
    fn atom_without_entry_errors() {
        assert!(parse_arxiv_atom("<feed><title>q</title></feed>", "x").is_err());
    }

    #[test]
    fn unescapes_xml_entities() {
        assert_eq!(unescape_xml("a &amp; b &lt;c&gt; &quot;d&quot;"), r#"a & b <c> "d""#);
    }

    #[test]
    fn note_body_keeps_the_justified_link_prompt_and_marks_imports() {
        let src = Source {
            title: "Attention Is All You Need".into(),
            authors: vec!["Ashish Vaswani".into()],
            year: Some("2017".into()),
            refs: vec!["arXiv:1706.03762".into()],
            tags: vec!["paper".into()],
            captured: Some(Captured { heading: "Abstract".into(), text: "We propose…".into() }),
        };
        let body = note_body(&src);
        assert!(body.contains("# Attention Is All You Need"));
        assert!(body.contains("Ashish Vaswani (2017)"));
        assert!(body.contains("Refs: arXiv:1706.03762"));
        assert!(body.contains("## Abstract\nWe propose…"));
        assert!(body.contains("Imported source"), "imports must be marked, not passed off as recall");
        assert!(
            body.contains("Link from memory"),
            "capture must never become an auto-link shortcut"
        );
    }

    #[test]
    fn note_body_omits_empty_metadata() {
        let src = Source {
            title: "Bare".into(),
            authors: vec![],
            year: None,
            refs: vec![],
            tags: vec!["clip".into()],
            captured: None,
        };
        let body = note_body(&src);
        assert!(!body.contains("Refs:"));
        assert!(!body.contains("()"));
        assert!(body.contains("Link from memory"));
    }
}
