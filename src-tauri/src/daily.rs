//! daily.rs — dated quick-capture ("fleeting") notes.
//!
//! A daily note is the one place where fast capture is allowed: a date-titled scratch
//! note for jotting raw thoughts before they're worked into real notes. It still obeys
//! the thesis — it *creates a note, never an edge*. Connecting anything you jotted here
//! goes through the no-autocomplete, justified `linker` flow like everything else.
//!
//! Pure module: it produces the title/tags/body strings and reuses [`templates::render`]
//! to stamp the date. The command layer does the find-or-create against the vault so a
//! given day always resolves to the same note instead of piling up duplicates.

use crate::templates;

/// The canonical title for a day's note: its ISO date. Stable and sortable, so the
/// same day always resolves to the same note (find-or-create keys off this).
pub fn title_for(date: &str) -> String {
    date.to_string()
}

/// Tags applied to a freshly created daily note.
pub fn tags() -> Vec<String> {
    vec!["daily".into(), "fleeting".into()]
}

/// The body skeleton for a daily note, with the date stamped in. Reuses the same
/// placeholder renderer the templates use, so the two stay consistent.
pub fn render_body(date: &str) -> String {
    let skeleton = "# {{date}}\n\n\
                    ## Fleeting notes\n\
                    Jot raw thoughts here — capture now, process later.\n\n\
                    - \n\n\
                    ## To process\n\
                    Turn anything worth keeping into its own note, then connect it with \
                    \"Link from memory\".\n";
    templates::render(skeleton, &title_for(date), date)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_is_the_date() {
        assert_eq!(title_for("2026-06-29"), "2026-06-29");
    }

    #[test]
    fn body_stamps_the_date_and_keeps_the_justified_link_prompt() {
        let body = render_body("2026-06-29");
        assert!(body.contains("2026-06-29"), "date should be stamped into the body");
        assert!(!body.contains("{{date}}"), "no placeholder should survive rendering");
        assert!(
            body.contains("Link from memory"),
            "capture must not become an auto-link shortcut"
        );
    }

    #[test]
    fn tags_mark_it_fleeting() {
        let t = tags();
        assert!(t.contains(&"daily".to_string()));
        assert!(t.contains(&"fleeting".to_string()));
    }
}
