//! What a chapter's title says about it.

/// Whether a chapter's title marks closing credits.
///
/// Chapter names are structural rather than written (`End Credits`, `Outro`,
/// `ED`), so whole words are matched, and a title about the start (`Opening
/// Credits`, `Intro`, `OP`) is refused whatever else it says. So is a scene
/// set among them (`Post-Credits Scene`, `Mid-Credits`, `Stinger`), which is
/// the film's or the episode's own.
pub fn is_credits(title: &str) -> bool {
    let words: Vec<String> = title
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect();
    let has = |word: &str| words.iter().any(|w| w == word);
    if [
        "opening",
        "intro",
        "op",
        "recap",
        "previously",
        "cold",
        "post",
        "mid",
        "after",
        "scene",
        "stinger",
    ]
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

/// Whether a chapter's title marks the title sequence: `Opening`, `Intro`,
/// `OP`, `Opening Credits`, `Main Titles`, `Title Sequence`, `Theme Song`.
///
/// Whole words again, and a title about the end is refused whatever else
/// it says. So is one about what comes before the titles: a recap or a
/// cold open is part of the episode, and skipping it is skipping story.
pub fn is_intro(title: &str) -> bool {
    let words: Vec<String> = title
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect();
    let has = |word: &str| words.iter().any(|w| w == word);
    if [
        "end",
        "ending",
        "closing",
        "outro",
        "ed",
        "recap",
        "previously",
        "cold",
        "scene",
    ]
    .iter()
    .any(|w| has(w))
    {
        return false;
    }
    ["opening", "intro", "introduction", "op", "theme"]
        .iter()
        .any(|w| has(w))
        || ((has("title") || has("titles"))
            && ["main", "sequence", "card", "credits"]
                .iter()
                .any(|w| has(w)))
        || words == ["titles"]
}

/// Whether a chapter's title marks a look at the next episode: `Preview`,
/// `Next Episode`, `Next Time`, `Next Week`, `Yokoku`, `次回予告`. After the
/// credits, anything else is a scene to stay for, and this is more of the
/// way on.
pub fn is_preview(title: &str) -> bool {
    let words: Vec<String> = title
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect();
    let has = |word: &str| words.iter().any(|w| w == word);
    has("preview")
        || has("yokoku")
        || title.contains("予告")
        || (has("next") && ["episode", "time", "week"].iter().any(|w| has(w)))
}

/// Whether a chapter's title is only a placeholder (its own start time,
/// `00:42:22.832`, or `Chapter 7`), as a muxer writes when nobody named it.
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
            "Post-Credits Scene",
            "Mid-Credits",
            "After Credits",
        ] {
            assert!(!is_credits(title), "{title}");
        }
    }
    #[test]
    fn title_sequence_chapters_are_recognised() {
        for title in [
            "Opening",
            "Intro",
            "OP",
            "Opening Credits",
            "Main Titles",
            "Title Sequence",
            "Opening Theme",
            "Theme Song",
            "Titles",
            "op_01",
        ] {
            assert!(is_intro(title), "{title}");
        }
        for title in [
            "End Credits",
            "Ending Theme",
            "ED",
            "Recap",
            "Previously On",
            "Cold Open",
            "Opening Scene",
            "Chapter 1",
            "Episode",
            "The Title Fight",
            "Part A",
            "Open Water",
        ] {
            assert!(!is_intro(title), "{title}");
        }
    }

    #[test]
    fn previews_are_told_from_scenes() {
        for title in [
            "Preview",
            "Next Episode Preview",
            "Next Time",
            "Next Week On",
            "Yokoku",
            "次回予告",
        ] {
            assert!(is_preview(title), "{title}");
        }
        for title in [
            "Post-Credits Scene",
            "Epilogue",
            "Next Morning",
            "The Next Generation",
            "Chapter 9",
        ] {
            assert!(!is_preview(title), "{title}");
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
