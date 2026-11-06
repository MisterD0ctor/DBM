//! Language tags, and the names a reader looks for them by.

use std::hash::{Hash, Hasher};

/// Endonyms: a language written the way it writes itself, which is what
/// someone looking for their own language is scanning the list for.
///
/// Keyed by ISO 639-1. Not every language: the long tail falls back to the
/// raw code, and once that matters, a real CLDR table should replace this.
const LANGUAGES: &[(&str, &str)] = &[
    ("ar", "العربية"),
    ("bg", "Български"),
    ("bn", "বাংলা"),
    ("cs", "Čeština"),
    ("da", "Dansk"),
    ("de", "Deutsch"),
    ("el", "Ελληνικά"),
    ("en", "English"),
    ("es", "Español"),
    ("et", "Eesti"),
    ("fa", "فارسی"),
    ("fi", "Suomi"),
    ("fr", "Français"),
    ("he", "עברית"),
    ("hi", "हिन्दी"),
    ("hr", "Hrvatski"),
    ("hu", "Magyar"),
    ("id", "Indonesia"),
    ("is", "Íslenska"),
    ("it", "Italiano"),
    ("ja", "日本語"),
    ("ko", "한국어"),
    ("lt", "Lietuvių"),
    ("lv", "Latviešu"),
    ("ms", "Melayu"),
    ("nb", "Norsk bokmål"),
    ("nl", "Nederlands"),
    ("nn", "Norsk nynorsk"),
    ("no", "Norsk"),
    ("pl", "Polski"),
    ("pt", "Português"),
    ("ro", "Română"),
    ("ru", "Русский"),
    ("sk", "Slovenčina"),
    ("sl", "Slovenščina"),
    ("sr", "Српски"),
    ("sv", "Svenska"),
    ("th", "ไทย"),
    ("tr", "Türkçe"),
    ("uk", "Українська"),
    ("vi", "Tiếng Việt"),
    ("zh", "中文"),
];

/// Three-letter codes onto their two-letter equivalents. Both the
/// bibliographic and terminological forms, since files carry either.
#[rustfmt::skip]
const ALPHA3: &[(&str, &str)] = &[
    ("ara", "ar"), ("bul", "bg"), ("ben", "bn"), ("cze", "cs"), ("ces", "cs"),
    ("dan", "da"), ("ger", "de"), ("deu", "de"), ("gre", "el"), ("ell", "el"),
    ("eng", "en"), ("spa", "es"), ("est", "et"), ("per", "fa"), ("fas", "fa"),
    ("fin", "fi"), ("fre", "fr"), ("fra", "fr"), ("heb", "he"), ("hin", "hi"),
    ("hrv", "hr"), ("hun", "hu"), ("ind", "id"), ("ice", "is"), ("isl", "is"),
    ("ita", "it"), ("jpn", "ja"), ("kor", "ko"), ("lit", "lt"), ("lav", "lv"),
    ("may", "ms"), ("msa", "ms"), ("nob", "nb"), ("dut", "nl"), ("nld", "nl"),
    ("nno", "nn"), ("nor", "no"), ("pol", "pl"), ("por", "pt"), ("rum", "ro"),
    ("ron", "ro"), ("rus", "ru"), ("slo", "sk"), ("slk", "sk"), ("slv", "sl"),
    ("srp", "sr"), ("swe", "sv"), ("tha", "th"), ("tur", "tr"), ("ukr", "uk"),
    ("vie", "vi"), ("chi", "zh"), ("zho", "zh"),
];

/// A language tag, and what makes two tags the same language.
///
/// The tag is kept as given (trimmed and lower-cased), because a request or a
/// file name may need the exact form: `pt-br` and `pt-pt` are different
/// OpenSubtitles codes even though both are Portuguese. Equality uses the
/// folded key instead, which is the two-letter code where one is known and the
/// tag itself otherwise. So `eng`, `en-US` and `en` are one language, while a
/// tag `base_code` cannot place stays distinct from every other.
#[derive(Clone, Debug)]
pub struct Language {
    tag: String,
    base: Option<&'static str>,
}

impl Language {
    /// Read a tag, or `None` where there is none. Any non-empty tag is
    /// accepted: an unknown language is still something a file or a subtitle
    /// can name.
    pub fn parse(tag: &str) -> Option<Self> {
        let tag = tag.trim().to_ascii_lowercase();
        if tag.is_empty() {
            return None;
        }
        let base = base_code(&tag);
        Some(Self { tag, base })
    }

    /// The tag as it would be written in a request or a file name.
    pub fn tag(&self) -> &str {
        &self.tag
    }

    /// The code that decides whether two tags are the same language.
    pub fn key(&self) -> &str {
        self.base.unwrap_or(&self.tag)
    }

    /// The language's own name for itself, or its tag upper-cased where
    /// there is no name for it.
    pub fn name(&self) -> String {
        match self
            .base
            .and_then(|code| LANGUAGES.iter().find(|(known, _)| *known == code))
        {
            Some((_, name)) => (*name).to_string(),
            None => self.tag.to_ascii_uppercase(),
        }
    }
}

impl PartialEq for Language {
    fn eq(&self, other: &Self) -> bool {
        self.key() == other.key()
    }
}

impl Eq for Language {}

impl Hash for Language {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.key().hash(state);
    }
}

/// The two-letter code a tag reduces to: `en-US` and `eng` both give `en`.
///
/// Whoever muxed the file picked from `en`, `eng` and `en-US` more or less at
/// random, so nothing that compares languages can compare the raw strings.
pub fn base_code(tag: &str) -> Option<&'static str> {
    let tag = tag.trim().to_ascii_lowercase();
    let head = tag.split(['-', '_']).next().unwrap_or(&tag);
    if let Some((code, _)) = LANGUAGES.iter().find(|(c, _)| *c == head) {
        return Some(code);
    }
    ALPHA3
        .iter()
        .find(|(three, _)| *three == head)
        .map(|(_, two)| *two)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn language(tag: &str) -> Language {
        Language::parse(tag).expect("a language tag")
    }

    #[test]
    fn language_codes_fold_together() {
        assert_eq!(language("eng"), language("en-US"));
        assert_eq!(language("SWE"), language("sv"));
        assert_ne!(language("en"), language("sv"));
        assert_eq!(language("swe").name(), "Svenska");
        assert_eq!(language("ja").name(), "日本語");
        assert_eq!(language("qqq").name(), "QQQ");
    }

    #[test]
    fn a_tag_keeps_its_region_for_a_request() {
        let brazil = language("pt-br");
        let portugal = language("pt-pt");
        assert_eq!(brazil, portugal, "both are Portuguese");
        assert_eq!(brazil.tag(), "pt-br");
        assert_eq!(portugal.tag(), "pt-pt");
        assert_eq!(language("  EN_us  ").tag(), "en_us");
    }

    #[test]
    fn nothing_is_not_a_language() {
        assert_eq!(Language::parse("   "), None);
    }
}
