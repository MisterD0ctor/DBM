use std::path::Path;

/// Where the translations live: `po/<code>/LC_MESSAGES/dbm-player.po`.
const PO: &str = "po";
/// The context the Rust side's own words are filed under in those files.
/// Everything else in them is Slint's, filed under a component's name.
const CONTEXT: &str = "player";

/// Turn each language's `player` entries into a table `lib.rs` can search.
///
/// The same files Slint reads, so a language is one file and not two that
/// have to be kept in step.
fn main() {
    println!("cargo:rerun-if-changed={PO}");
    let mut codes: Vec<String> = std::fs::read_dir(PO)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter(|e| e.path().is_dir())
                .filter_map(|e| e.file_name().into_string().ok())
                .collect()
        })
        .unwrap_or_default();
    codes.sort();

    let mut out = String::from("static CATALOGUES: &[(&str, &[(&str, &str)])] = &[\n");
    for code in &codes {
        let file = Path::new(PO).join(code).join("LC_MESSAGES/dbm-player.po");
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        let mut words: Vec<(String, String)> = entries(&text)
            .into_iter()
            .filter(|e| e.context == CONTEXT && !e.id.is_empty() && !e.text.is_empty())
            .map(|e| (e.id, e.text))
            .collect();
        words.sort();
        words.dedup_by(|a, b| a.0 == b.0);
        out.push_str(&format!("    ({code:?}, &[\n"));
        for (english, theirs) in words {
            out.push_str(&format!("        ({english:?}, {theirs:?}),\n"));
        }
        out.push_str("    ]),\n");
    }
    out.push_str("];\n");
    let dest = Path::new(&std::env::var("OUT_DIR").expect("OUT_DIR")).join("catalogues.rs");
    std::fs::write(dest, out).expect("writing the catalogues");
}

#[derive(Default)]
struct Entry {
    context: String,
    id: String,
    text: String,
}

/// The entries of a gettext file: `msgctxt`, `msgid` and `msgstr`, each a
/// quoted string that may carry on over the lines after it. Plural forms are
/// not read; the Rust side has none.
fn entries(po: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    let mut entry = Entry::default();
    // Which of the three the next bare quoted line carries on.
    let mut into = 0;
    for line in po.lines().map(str::trim) {
        let (field, rest) = if let Some(rest) = line.strip_prefix("msgctxt ") {
            (1, rest)
        } else if let Some(rest) = line.strip_prefix("msgid ") {
            (2, rest)
        } else if let Some(rest) = line.strip_prefix("msgstr ") {
            (3, rest)
        } else if line.starts_with('"') {
            (into, line)
        } else {
            if line.is_empty() {
                out.push(std::mem::take(&mut entry));
                into = 0;
            }
            continue;
        };
        // A new entry begins at its context, or at its id where it has none.
        if (field == 1 || (field == 2 && into != 1)) && field != into {
            out.push(std::mem::take(&mut entry));
        }
        into = field;
        let said = unquote(rest);
        match field {
            1 => entry.context.push_str(&said),
            2 => entry.id.push_str(&said),
            3 => entry.text.push_str(&said),
            _ => {}
        }
    }
    out.push(entry);
    out
}

/// What a quoted gettext string says, with its escapes undone.
fn unquote(quoted: &str) -> String {
    let inner = quoted
        .trim()
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or_default();
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}
