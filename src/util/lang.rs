//! Track-language helpers, built on the platform's `Intl` APIs.
//!
//! Used by the tracks menu to label tracks, and by the caption toggle to
//! decide whether a track on *this* file is the same language as the one
//! last chosen on another file.

use std::collections::{HashMap, HashSet};

use shared::Track;
use wasm_bindgen::prelude::*;

// ============================================================================
// Intl bindings — ported from `languageCodeEndonym` in tracks.js
// ============================================================================

// The Rust type name doubles as the JS class name unless `js_name` overrides
// it — without these, wasm-bindgen would look up `Intl.IntlLocale` /
// `Intl.IntlDisplayNames`, both `undefined`, and every call would throw.
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = Intl, js_name = "Locale")]
    type IntlLocale;

    #[wasm_bindgen(constructor, js_namespace = Intl, js_class = "Locale", catch)]
    fn new(tag: &str) -> Result<IntlLocale, JsValue>;

    #[wasm_bindgen(method, getter, js_class = "Locale")]
    fn language(this: &IntlLocale) -> String;
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = Intl, js_name = "DisplayNames")]
    type IntlDisplayNames;

    #[wasm_bindgen(constructor, js_namespace = Intl, js_class = "DisplayNames", catch)]
    fn new(locales: &js_sys::Array, options: &js_sys::Object) -> Result<IntlDisplayNames, JsValue>;

    #[wasm_bindgen(method, catch, js_class = "DisplayNames")]
    fn of(this: &IntlDisplayNames, code: &str) -> Result<JsValue, JsValue>;
}

/// Base language code of a BCP 47 tag — `"en"` for `"en-US"`, `"es"` for
/// `"es-419"`. Returns `None` when `Intl.Locale` rejects the tag.
pub fn base_language(code: &str) -> Option<String> {
    IntlLocale::new(code).ok().map(|l| l.language())
}

/// Localized language name for a BCP 47 tag, in the language's own script
/// (e.g. `"es"` → `"Español"`, `"ja"` → `"日本語"`). When `with_region` is
/// true the region qualifier is included (`"es-419"` → `"Español (Latinoamérica)"`).
/// First letter uppercased to match the JS port. Returns `None` when Intl
/// can't resolve the code at all.
pub fn language_endonym(code: &str, with_region: bool) -> Option<String> {
    if code.is_empty() {
        return None;
    }
    let locale = IntlLocale::new(code).ok()?;
    let base = locale.language();

    let locales = js_sys::Array::of1(&JsValue::from_str(&base));
    let options = js_sys::Object::new();
    js_sys::Reflect::set(
        &options,
        &JsValue::from_str("type"),
        &JsValue::from_str("language"),
    )
    .ok()?;
    let display = IntlDisplayNames::new(&locales, &options).ok()?;

    let lookup: &str = if with_region { code } else { &base };
    let name = display
        .of(lookup)
        .ok()
        .and_then(|v| v.as_string())
        .or_else(|| display.of(&base).ok().and_then(|v| v.as_string()))?;

    let mut chars = name.chars();
    let first: String = chars.next()?.to_uppercase().collect();
    Some(format!("{first}{}", chars.as_str()))
}

/// Do two track language codes name the same language?
///
/// A plain string compare isn't enough across files: whoever muxed them
/// picked from `"en"`, `"eng"`, and `"en-US"` more or less at random, and
/// the saved preference has to match a track tagged any of those ways.
pub fn same_language(a: &str, b: &str) -> bool {
    let (a, b) = (a.trim(), b.trim());
    if a.is_empty() || b.is_empty() {
        return false;
    }
    if a.eq_ignore_ascii_case(b) {
        return true;
    }
    // `Intl.Locale` canonicalizes the tag, which drops the region qualifier
    // and folds an alpha-3 code onto its alpha-2 equivalent where one exists.
    if let (Some(x), Some(y)) = (base_language(a), base_language(b)) {
        if x.eq_ignore_ascii_case(&y) {
            return true;
        }
    }
    // Last resort, in case the runtime canonicalizes less than we'd like:
    // two tags for one language still resolve to the same display name.
    // Unresolvable tags come back as themselves, so junk won't collide.
    match (language_endonym(a, false), language_endonym(b, false)) {
        (Some(x), Some(y)) => x.eq_ignore_ascii_case(&y),
        _ => false,
    }
}

pub fn title_case(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(first) => first.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

/// Build human-readable titles for a list of tracks, disambiguating by
/// language / numbering when multiple tracks share a language. Mirrors
/// `buildTrackTitles` from death-by-mpv/src/ui/tracks.js — uses the
/// platform's `Intl.DisplayNames` to localize the language code into its
/// endonym (e.g. "es" → "Español"). Region qualifiers (e.g. "es-419" vs
/// "es-ES") are only included when the same base language has multiple
/// regional variants in the track list.
pub fn build_track_titles(tracks: &[Track]) -> HashMap<u32, String> {
    let mut lang_counts: HashMap<String, usize> = HashMap::new();
    let mut base_to_codes: HashMap<String, HashSet<String>> = HashMap::new();
    for t in tracks {
        if let Some(l) = &t.lang {
            *lang_counts.entry(l.clone()).or_insert(0) += 1;
            if let Some(base) = base_language(l) {
                base_to_codes.entry(base).or_default().insert(l.clone());
            }
        }
    }
    // A code needs its region qualifier when its base language has multiple
    // regional siblings in this track list.
    let mut needs_region: HashSet<String> = HashSet::new();
    for codes in base_to_codes.values() {
        if codes.len() > 1 {
            for c in codes {
                needs_region.insert(c.clone());
            }
        }
    }

    let mut pair_counts: HashMap<(String, String), usize> = HashMap::new();
    for t in tracks {
        if let Some(l) = &t.lang {
            let title = t.title.clone().unwrap_or_default();
            *pair_counts.entry((l.clone(), title)).or_insert(0) += 1;
        }
    }

    let mut seen: HashMap<(String, String), usize> = HashMap::new();
    let mut out: HashMap<u32, String> = HashMap::new();

    for t in tracks {
        let lang = t.lang.as_deref().unwrap_or("");
        let title = t.title.as_deref().unwrap_or("");
        let has_lang = !lang.is_empty();
        let has_title = !title.is_empty();

        let label = if !has_lang && !has_title {
            format!("Track {}", t.id)
        } else if !has_lang {
            title.to_string()
        } else {
            let unique = lang_counts.get(lang).copied().unwrap_or(0) == 1;
            // Falls back to a title-cased raw code when Intl rejects the
            // tag (junk codes from the source, very old webviews, …).
            let lang_label = language_endonym(lang, needs_region.contains(lang))
                .unwrap_or_else(|| title_case(lang));
            if unique {
                lang_label
            } else {
                let pair_key = (lang.to_string(), title.to_string());
                let total = pair_counts.get(&pair_key).copied().unwrap_or(0);
                let needs_number = total > 1 || !has_title;
                let n = seen.entry(pair_key).and_modify(|c| *c += 1).or_insert(1);
                let n = *n;
                if has_title && !needs_number {
                    // The track's own title already mentions the language
                    // (e.g. "English (Director's commentary)") — don't
                    // double up. Compare against the localized name, since
                    // that's what the user would see.
                    let title_includes_lang =
                        title.to_lowercase().contains(&lang_label.to_lowercase());
                    if title_includes_lang {
                        title.to_string()
                    } else {
                        format!("{lang_label} - {title}")
                    }
                } else if has_title && needs_number {
                    format!("{lang_label} - {title} {n}")
                } else {
                    format!("{lang_label} - {n}")
                }
            }
        };

        out.insert(t.id, label);
    }
    out
}
