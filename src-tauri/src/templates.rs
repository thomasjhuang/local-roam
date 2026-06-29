//! templates.rs — built-in scaffolds for new-note bodies.
//!
//! A template is *only* a body skeleton (plus suggested tags): headings and prompts
//! that make the user write the idea in their own words. It is deliberately NOT a
//! capture shortcut — it pre-fills *structure*, never content, and never edges.
//! Connecting the resulting note still goes through the no-autocomplete, justified
//! `linker` flow. That is why every body ends with a "Connections" prompt that points
//! the user back at "Link from memory" instead of auto-suggesting anything.
//!
//! Pure module: it knows nothing about the vault or the index. It produces strings.
//! `daily.rs` (#18b) reuses `render` to instantiate a dated template, and the command
//! layer is what actually writes the note through the vault.

use serde::Serialize;

/// A new-note body skeleton the user can start from.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Template {
    /// Stable identifier used by the command layer to look the template back up.
    pub id: String,
    /// Human-facing name shown in the picker.
    pub name: String,
    /// One line describing when to reach for it.
    pub description: String,
    /// Suggested tags applied to the new note (the user can still edit them).
    pub tags: Vec<String>,
    /// Body skeleton. May contain `{{title}}` and `{{date}}` placeholders, filled by
    /// [`render`] at instantiation time.
    pub body: String,
}

/// Substitute the supported placeholders (`{{title}}`, `{{date}}`) in a template body.
/// Unknown placeholders are left untouched so a typo never silently eats text.
pub fn render(body: &str, title: &str, date: &str) -> String {
    body.replace("{{title}}", title).replace("{{date}}", date)
}

/// The built-in templates, in display order. Tailored to ML-research note-taking
/// (see CONTEXT.md): each one elaborates rather than captures.
pub fn builtin() -> Vec<Template> {
    vec![
        Template {
            id: "paper".into(),
            name: "Paper".into(),
            description: "A research paper, summarised in your own words.".into(),
            tags: vec!["paper".into()],
            body: "# {{title}}\n\n\
                   > Write each section from memory after reading — don't paste the abstract.\n\n\
                   ## Problem\nWhat gap does it address?\n\n\
                   ## Method\nThe core idea, in a paragraph you could explain aloud.\n\n\
                   ## Results\nWhat did it actually show?\n\n\
                   ## Why it matters\n\n\
                   ## Connections\n\
                   Link this to the papers and ideas it builds on with \"Link from memory\" \
                   — typed from memory, one justified edge at a time.\n"
                .into(),
        },
        Template {
            id: "concept".into(),
            name: "Concept".into(),
            description: "A definition or technique you want to internalise.".into(),
            tags: vec!["concept".into()],
            body: "# {{title}}\n\n\
                   ## In your own words\nDefine it without looking it up.\n\n\
                   ## Intuition\nWhy does it work? What's the mental picture?\n\n\
                   ## Where it shows up\nPapers or methods that rely on it.\n\n\
                   ## Connections\n\
                   Use \"Link from memory\" to tie this to the concepts it depends on.\n"
                .into(),
        },
        Template {
            id: "question".into(),
            name: "Open question".into(),
            description: "Something you don't understand yet and want to track.".into(),
            tags: vec!["question".into()],
            body: "# {{title}}\n\n\
                   ## The question\nState it precisely.\n\n\
                   ## Why it matters\n\n\
                   ## What I've considered\n\n\
                   ## Connections\n\
                   Link to the notes that bear on this question with \"Link from memory\".\n"
                .into(),
        },
    ]
}

/// Look up a template by its stable id.
pub fn by_id(id: &str) -> Option<Template> {
    builtin().into_iter().find(|t| t.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_fills_known_placeholders() {
        let out = render("# {{title}}\n{{date}}", "Attention", "2026-06-29");
        assert_eq!(out, "# Attention\n2026-06-29");
    }

    #[test]
    fn render_leaves_unknown_placeholders_untouched() {
        assert_eq!(render("{{nope}}", "t", "d"), "{{nope}}");
    }

    #[test]
    fn builtin_ids_are_unique_and_lookupable() {
        let all = builtin();
        let mut ids: Vec<&str> = all.iter().map(|t| t.id.as_str()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), all.len(), "template ids must be unique");
        for t in &all {
            assert_eq!(by_id(&t.id).as_ref(), Some(t));
        }
    }

    #[test]
    fn by_id_is_none_for_unknown() {
        assert!(by_id("does-not-exist").is_none());
    }

    #[test]
    fn every_template_keeps_the_justified_link_prompt() {
        // Guardrail: a template must never become an autocomplete/auto-link shortcut.
        for t in builtin() {
            assert!(
                t.body.contains("Link from memory"),
                "template {} dropped the justified-link prompt",
                t.id
            );
        }
    }
}
