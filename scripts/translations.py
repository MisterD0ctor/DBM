#!/usr/bin/env python3
"""Keep the translation files in step with the words in the source.

    scripts/translations.py check     list what each language is missing
    scripts/translations.py update    add the missing entries, untranslated
    scripts/translations.py add <code>    start a new language

A language is one gettext file, crates/player/lang/<code>/LC_MESSAGES/
dbm-player.po, and it holds both halves of the interface:

  * the words in the .slint files, marked @tr("...") and filed under the
    name of the component they are written in, which is where Slint looks
    them up;
  * the words made in Rust, marked tr("..."), filed under `player`, and the
    words in a Rust table translated where they are shown, marked
    marked("...") where they are written.

`check` exits non-zero when a language is missing an entry or has one left
empty, so it can stand in a release checklist, and when a Rust `label:` is a
bare string, which is a name the interface shows that nothing marked, as is
the `reason:`, `detail:` or `note:` of a failure the way in shows. An entry
the source no longer has is reported and left alone: deleting a translation
is a person's call.
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent / "crates" / "player"
LANG = ROOT / "lang"
RUST_CONTEXT = "player"

STRING = r'"((?:[^"\\]|\\[\s\S])*)"'
SLINT_TR = re.compile(r"@tr\(\s*" + STRING)
SLINT_OWNER = re.compile(r"^(?:export\s+)?(?:component|global)\s+([A-Za-z_][\w-]*)")
RUST_TR = re.compile(r"(?<![\w@])(?:tr|marked)\(\s*" + STRING)
RUST_BARE_LABEL = re.compile(r"\b(?:label|reason|detail|note):\s*\"")
# Rust's line continuation: a backslash at the end of a line drops the break
# and the next line's indentation, so the words are what the program holds.
RUST_CONTINUED = re.compile(r"\\\n\s*")


def wanted():
    """Every (context, words) the source asks for, in the order it is met."""
    found = []
    for file in sorted((ROOT / "ui").glob("*.slint")):
        owner = ""
        for line in file.read_text(encoding="utf-8").splitlines():
            if line.lstrip().startswith("//"):
                continue
            named = SLINT_OWNER.match(line)
            if named:
                owner = named.group(1)
            for said in SLINT_TR.finditer(line):
                found.append((owner, said.group(1)))
    for file in sorted((ROOT / "src").rglob("*.rs")):
        if file.name == "lang.rs":
            continue
        for said in RUST_TR.finditer(file.read_text(encoding="utf-8")):
            found.append((RUST_CONTEXT, RUST_CONTINUED.sub("", said.group(1))))
    seen, out = set(), []
    for entry in found:
        if entry not in seen:
            seen.add(entry)
            out.append(entry)
    return out


def entries(text):
    """The (context, id) -> translation of a .po file's text."""
    out, context, ident, said, into = {}, "", None, "", None

    def close():
        nonlocal context, ident, said
        if ident:
            out[(context, ident)] = said
        context, ident, said = "", None, ""

    for line in text.splitlines():
        line = line.strip()
        field = None
        for name in ("msgctxt", "msgid", "msgstr"):
            if line.startswith(name + " "):
                field, line = name, line[len(name) + 1:]
        if field is None and line.startswith('"'):
            field = into
        if field is None:
            continue
        value = re.fullmatch(STRING, line)
        value = value.group(1) if value else ""
        if field == "msgctxt" or (field == "msgid" and into != "msgctxt"):
            if field != into:
                close()
        if field == "msgctxt":
            context += value
        elif field == "msgid":
            ident = (ident or "") + value
        elif field == "msgstr":
            said += value
        into = field
    close()
    return out


def files():
    return sorted(LANG.glob("*/LC_MESSAGES/dbm-player.po"))


def header(code):
    return (
        'msgid ""\nmsgstr ""\n'
        f'"Language: {code}\\n"\n'
        '"Content-Type: text/plain; charset=UTF-8\\n"\n'
        '"Plural-Forms: nplurals=2; plural=(n != 1);\\n"\n'
    )


def bare_labels():
    """Every Rust `label:`, and failure field, set to a string nothing
    marked, as file:line.

    A file's tests, which come last in it, are not looked at: their labels
    are fixtures, never shown.
    """
    found = []
    for file in sorted((ROOT / "src").rglob("*.rs")):
        for number, line in enumerate(file.read_text(encoding="utf-8").splitlines(), 1):
            if line.strip() == "#[cfg(test)]":
                break
            if RUST_BARE_LABEL.search(line):
                found.append(f"{file.relative_to(ROOT)}:{number}")
    return found


def check():
    asked = wanted()
    short = False
    for where in bare_labels():
        print(f"unmarked  {where}: a label the interface shows, not marked for translation")
        short = True
    for file in files():
        code = file.parts[-3]
        have = entries(file.read_text(encoding="utf-8"))
        missing = [e for e in asked if not have.get(e)]
        stale = [e for e in have if e not in set(asked)]
        print(f"{code}: {len(asked) - len(missing)} of {len(asked)} translated")
        for context, words in missing:
            print(f"  missing  [{context}] {words}")
        for context, words in stale:
            print(f"  unused   [{context}] {words}")
        short = short or bool(missing)
    return 1 if short else 0


def update():
    asked = wanted()
    for file in files():
        text = file.read_text(encoding="utf-8")
        have = entries(text)
        added = [e for e in asked if e not in have]
        if not text.endswith("\n"):
            text += "\n"
        for context, words in added:
            text += f'\nmsgctxt "{context}"\nmsgid "{words}"\nmsgstr ""\n'
        file.write_text(text, encoding="utf-8")
        print(f"{file.parts[-3]}: added {len(added)}")
    return 0


def add(code):
    file = LANG / code / "LC_MESSAGES" / "dbm-player.po"
    if file.exists():
        print(f"{code} is already there")
        return 1
    file.parent.mkdir(parents=True, exist_ok=True)
    file.write_text(header(code), encoding="utf-8")
    return update()


if __name__ == "__main__":
    args = sys.argv[1:]
    if args == ["check"]:
        sys.exit(check())
    if args == ["update"]:
        sys.exit(update())
    if len(args) == 2 and args[0] == "add":
        sys.exit(add(args[1]))
    print(__doc__)
    sys.exit(2)
