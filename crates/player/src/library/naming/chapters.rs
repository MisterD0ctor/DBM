//! What a chapter's title says about it.

/// Whether a chapter's title marks closing credits.
///
/// Chapter names are structural rather than written — `End Credits`,
/// `Outro`, `ED` — so whole words are matched, and a title about the start
/// (`Opening Credits`, `Intro`, `OP`) is refused whatever else it says.
pub fn is_credits(title: &str) -> bool {
    let words: Vec<String> = title
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect();
    let has = |word: &str| words.iter().any(|w| w == word);
    if ["opening", "intro", "op", "recap", "previously", "cold"]
        .iter()
        .any(|w| has(w))
    {
        return false;
    }
    [
        "credits",
        "credit",
        "endcredits",
        "outro",
        "ending",
        "ed",
        "closing",
    ]
    .iter()
    .any(|w| has(w))
        || (has("end") && (words.len() == 1 || has("titles")))
}

/// Whether a chapter's title is only a placeholder — its own start time,
/// `00:42:22.832`, or `Chapter 7` — which is what a muxer writes when nobody
/// named anything.
pub fn is_unnamed_chapter(title: &str) -> bool {
    let title = title.trim();
    if let Some(rest) = title
        .strip_prefix("Chapter")
        .or_else(|| title.strip_prefix("chapter"))
    {
        let rest = rest.trim_start();
        return !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit());
    }
    title.chars().any(|c| c.is_ascii_digit())
        && title
            .chars()
            .all(|c| c.is_ascii_digit() || c == ':' || c == '.')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credits_chapters_are_recognised() {
        for title in [
            "End Credits",
            "Credits",
            "Closing Credits",
            "Outro",
            "ED",
            "Ending",
            "Ending Theme",
            "End",
            "End Titles",
            "credits_roll",
        ] {
            assert!(is_credits(title), "{title}");
        }
        for title in [
            "Opening Credits",
            "Intro",
            "OP",
            "Chapter 12",
            "Edward's Return",
            "Previously On",
            "The End of the Beginning",
            "Preview",
        ] {
            assert!(!is_credits(title), "{title}");
        }
    }
    #[test]
    fn placeholder_chapter_titles_are_unnamed() {
        for title in [
            "00:42:22.832",
            "00:00:00.000",
            "Chapter 07",
            "chapter 3",
            "01",
        ] {
            assert!(is_unnamed_chapter(title), "{title}");
        }
        for title in [
            "End Credits",
            "Chapter One",
            "Chapter",
            "Act 3",
            "",
            "2001: A Space Odyssey",
        ] {
            assert!(!is_unnamed_chapter(title), "{title}");
        }
    }
}
