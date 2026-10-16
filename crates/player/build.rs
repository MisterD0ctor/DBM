use std::path::Path;

/// Where the translations live: `lang/<code>/LC_MESSAGES/dbm-player.po`.
const LANG: &str = "lang";
/// The context the Rust side's own words are filed under in those files.
/// Everything else in them is Slint's, filed under a component's name.
const CONTEXT: &str = "player";

fn main() {
    // The words in `app.slint` are bundled by Slint itself, which picks the
    // language off the system and can be told another — see `lang`.
    let config = slint_build::CompilerConfiguration::new().with_bundled_translations(LANG);
    slint_build::compile_with_config("ui/app.slint", config).expect("failed to compile app.slint");
    write_the_catalogues();
    pass_on_the_keys();
    stamp_the_executable();
}

/// Hand the code the keys it was built with, if any — see `keys/README.md`.
///
/// The folder is watched rather than the file: a file that does not exist
/// would have Cargo run this, and rebuild the interface, on every build.
fn pass_on_the_keys() {
    println!("cargo:rerun-if-env-changed=DBM_OPENSUBTITLES_KEY");
    println!("cargo:rerun-if-changed=keys");
    let key = std::env::var("DBM_OPENSUBTITLES_KEY")
        .ok()
        .or_else(|| std::fs::read_to_string("keys/opensubtitles").ok())
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty());
    if let Some(key) = key {
        println!("cargo:rustc-env=DBM_OPENSUBTITLES_KEY={key}");
    }
}

/// Turn each language's `player` entries into a table `lang.rs` can search.
///
/// The same files Slint reads, so a language is one file and not two that
/// have to be kept in step.
fn write_the_catalogues() {
    println!("cargo:rerun-if-changed={LANG}");
    let mut codes: Vec<String> = std::fs::read_dir(LANG)
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
        let file = Path::new(LANG).join(code).join("LC_MESSAGES/dbm-player.po");
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

/// The icon and version block Windows reads off the file itself.
///
/// Separate from the window icon, which Slint sets from a PNG at runtime:
/// this is the one Explorer, the taskbar and the installer's Add/Remove entry
/// show, and it has to be compiled into the binary as a resource.
///
/// Deliberately not fatal. It needs `rc.exe` from the Windows SDK, and a
/// machine without one should still get a working player — with a default
/// icon, which is a cosmetic loss rather than a broken build.
#[cfg(windows)]
fn stamp_the_executable() {
    println!("cargo:rerun-if-changed=icons/icon.ico");
    let mut resource = winresource::WindowsResource::new();
    resource.set_icon("icons/icon.ico");
    resource.set("ProductName", "Death by MPV");
    resource.set("FileDescription", "Death by MPV");
    if let Err(e) = resource.compile() {
        println!("cargo:warning=could not embed the icon or version block: {e}");
    }
}

#[cfg(not(windows))]
fn stamp_the_executable() {}
