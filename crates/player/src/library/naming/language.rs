//! Language tags, and the names a reader looks for them by.

/// Endonyms: a language written the way it writes itself, which is what
/// someone looking for their own language is scanning the list for.
///
/// Keyed by ISO 639-1. Not every language — the long tail falls back to the
/// raw code, and the point where that starts to matter is the point to bring
/// in a real CLDR table rather than to keep extending this one.
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

/// How to write a language code for a reader. Unknown codes come back
/// upper-cased, which at least reads as a code rather than as a word.
pub fn language_name(tag: &str) -> String {
    match base_code(tag).and_then(|c| LANGUAGES.iter().find(|(code, _)| *code == c)) {
        Some((_, name)) => (*name).to_string(),
        None => tag.trim().to_ascii_uppercase(),
    }
}

/// Do two tags name the same language?
pub fn same_language(a: &str, b: &str) -> bool {
    let (a, b) = (a.trim(), b.trim());
    if a.is_empty() || b.is_empty() {
        return false;
    }
    if a.eq_ignore_ascii_case(b) {
        return true;
    }
    match (base_code(a), base_code(b)) {
        (Some(x), Some(y)) => x == y,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_codes_fold_together() {
        assert!(same_language("eng", "en-US"));
        assert!(same_language("SWE", "sv"));
        assert!(!same_language("en", "sv"));
        assert_eq!(language_name("swe"), "Svenska");
        assert_eq!(language_name("ja"), "日本語");
        assert_eq!(language_name("qqq"), "QQQ");
    }
}
