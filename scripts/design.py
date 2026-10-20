#!/usr/bin/env python3
"""Generate DESIGN.md's front matter and .impeccable/design.json from the code.

    scripts/design.py write    regenerate both from the templates
    scripts/design.py check    fail if either is stale, a template or
                               DESIGN.md's prose writes out a value, or the
                               prose names a token the code no longer has

The code is the only place a size, a colour or a duration is written. The
templates in scripts/design/ say which token each value is, as `{{ expr }}`:
a path such as `Rows.gap`, `MainWindow.btn`, `StepButton.width` or
`MainWindow.menu.width` (a named element), `pipeline.BEVEL` for a Rust or
GLSL constant, and arithmetic over them. `X@then` and `X@else` take one branch
of a property that is a conditional. `| n` drops the unit. `{{ demo 640px }}`
is a stand-in with no source in the code, for the illustrative CSS.

The sidecar's narrative is read out of DESIGN.md itself, as impeccable's
format asks, so that is not written twice either. The prose names tokens in
backticks, the same paths as the templates, and `Component.element` for a named
element as a whole.
"""

import datetime
import json
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
PLAYER = REPO / "crates" / "player"
DESIGN_MD = REPO / "DESIGN.md"
SIDECAR = REPO / ".impeccable" / "design.json"
TEMPLATES = Path(__file__).resolve().parent / "design"
FRONTMATTER_TEMPLATE = TEMPLATES / "frontmatter.yml"
SIDECAR_TEMPLATE = TEMPLATES / "sidecar.json"

# Rust and GLSL files a template may take a constant from, by a short name.
MODULES = {
    "pipeline": PLAYER / "src/gpu/pipeline.rs",
    "shadow": PLAYER / "src/gpu/shadow.rs",
    "fades": PLAYER / "src/interface/fades.rs",
    "chrome": PLAYER / "src/interface/chrome.rs",
    "way": PLAYER / "src/gpu/way.rs",
    "commands": PLAYER / "src/playback/commands.rs",
    "state": PLAYER / "src/playback/state.rs",
    "arrival": PLAYER / "src/playback/arrival.rs",
    "preview": PLAYER / "src/library/preview.rs",
    "glass": PLAYER / "shaders/glass.frag",
    "backdrop": PLAYER / "shaders/backdrop.frag",
}

# A value written out, which the generated files and the prose must not do.
RAW_VALUE = re.compile(r"(?<![\w.#-])\d+(?:\.\d+)?(?:px|ms)\b|#[0-9a-fA-F]{6}(?:[0-9a-fA-F]{2})?\b")
PLACEHOLDER = re.compile(r"\{\{((?:(?!\}\}).)*)\}\}")
# A token named in the prose; a file name such as `theme.slint` is not one.
NAMED = re.compile(r"`([A-Za-z][\w-]*(?:\.[A-Za-z_][\w-]*)+(?:@(?:then|else))*)`")
FILE_NAME = re.compile(r"\.(?:slint|rs|frag|vert|glsl|md|json|yml|py|sh)$")


class DesignError(Exception):
    pass


# ---------------------------------------------------------------------------
# Values
# ---------------------------------------------------------------------------


class Value:
    """A number with its unit (`px`, `ms` or none), or a colour, string or bool."""

    def __init__(self, value, unit=None):
        self.value = value
        self.unit = unit

    def text(self, bare=False):
        v = self.value
        if isinstance(v, bool):
            return "true" if v else "false"
        if isinstance(v, (int, float)):
            number = number_text(v)
            return number if bare or not self.unit else number + self.unit
        if isinstance(v, list):
            return json.dumps([x.value if isinstance(x, Value) else x for x in v])
        return str(v)

    def json(self):
        v = self.value
        if isinstance(v, (int, float)) and not isinstance(v, bool) and not self.unit:
            return int(v) if float(v).is_integer() else v
        if isinstance(v, list):
            return [x.json() if isinstance(x, Value) else x for x in v]
        return self.text()


def number_text(n):
    n = float(n)
    if n.is_integer():
        return str(int(n))
    return f"{n:.4f}".rstrip("0").rstrip(".")


def combine_units(a, b, op):
    if op in "+-":
        if a.unit and b.unit and a.unit != b.unit:
            raise DesignError(f"cannot {op} {a.unit} and {b.unit}")
        return a.unit or b.unit
    if op == "*":
        return a.unit or b.unit
    # Division: px / px is a ratio.
    return None if a.unit and b.unit else a.unit


# ---------------------------------------------------------------------------
# Expressions: the subset of Slint the tokens are written in
# ---------------------------------------------------------------------------

TOKEN = re.compile(
    r"\s*(?:"
    r"(?P<num>\d+(?:\.\d+)?)(?P<unit>px|ms)?"
    r"|(?P<color>#[0-9a-fA-F]{3,8})"
    r'|(?P<str>"[^"]*")'
    r"|(?P<id>[A-Za-z_][A-Za-z0-9_]*(?:-[A-Za-z0-9_]+)*(?:\.[A-Za-z_][A-Za-z0-9_]*(?:-[A-Za-z0-9_]+)*)*(?:@(?:then|else))*)"
    r"|(?P<op>\|\||&&|==|!=|<=|>=|[-+*/(),?:<>!])"
    r")"
)


def tokenize(text):
    tokens, at = [], 0
    text = text.strip()
    while at < len(text):
        m = TOKEN.match(text, at)
        if not m or m.end() == at:
            raise DesignError(f"cannot read {text[at:]!r} in {text!r}")
        at = m.end()
        kind = m.lastgroup if m.lastgroup != "unit" else "num"
        if m.group("num") is not None:
            tokens.append(("num", (float(m.group("num")), m.group("unit"))))
        else:
            tokens.append((kind, m.group(kind)))
    return tokens


class Parser:
    """Precedence climbing over the tokens, evaluating as it goes."""

    def __init__(self, tokens, resolve):
        self.tokens = tokens
        self.at = 0
        self.resolve = resolve

    def peek(self):
        return self.tokens[self.at] if self.at < len(self.tokens) else (None, None)

    def take(self, op=None):
        kind, value = self.peek()
        if op is not None and value != op:
            raise DesignError(f"expected {op!r}, found {value!r}")
        self.at += 1
        return kind, value

    def parse(self):
        value = self.ternary()
        if self.at != len(self.tokens):
            raise DesignError(f"unexpected {self.peek()[1]!r}")
        return value

    def ternary(self):
        condition = self.binary(0)
        if self.peek()[1] != "?":
            return condition
        self.take("?")
        then = self.ternary()
        self.take(":")
        otherwise = self.ternary()
        return then if condition.value else otherwise

    LEVELS = [("||",), ("&&",), ("==", "!=", "<", ">", "<=", ">="), ("+", "-"), ("*", "/")]

    def binary(self, level):
        if level == len(self.LEVELS):
            return self.unary()
        left = self.binary(level + 1)
        while self.peek()[0] == "op" and self.peek()[1] in self.LEVELS[level]:
            _, op = self.take()
            right = self.binary(level + 1)
            left = apply(op, left, right)
        return left

    def unary(self):
        kind, value = self.peek()
        if value == "-":
            self.take()
            inner = self.unary()
            return Value(-inner.value, inner.unit)
        if value == "!":
            self.take()
            return Value(not self.unary().value)
        return self.primary()

    def primary(self):
        kind, value = self.take()
        if kind == "num":
            number, unit = value
            return Value(number, unit)
        if kind == "color":
            return Value(value, "color")
        if kind == "str":
            return Value(value[1:-1], "string")
        if value == "(":
            inner = self.ternary()
            self.take(")")
            return inner
        if kind == "id":
            if value in ("true", "false"):
                return Value(value == "true")
            if self.peek()[1] == "(":
                return self.call(value)
            return self.resolve(value)
        raise DesignError(f"unexpected {value!r}")

    def call(self, name):
        self.take("(")
        args = [self.ternary()]
        while self.peek()[1] == ",":
            self.take(",")
            args.append(self.ternary())
        self.take(")")
        functions = {"max": max, "min": min, "Math.max": max, "Math.min": min}
        if name not in functions:
            raise DesignError(f"unknown function {name}")
        unit = next((a.unit for a in args if a.unit), None)
        return Value(functions[name](a.value for a in args), unit)


def apply(op, a, b):
    if op in "+-*/":
        unit = combine_units(a, b, op)
        x, y = a.value, b.value
        result = {"+": x + y, "-": x - y, "*": x * y, "/": x / y}[op]
        return Value(result, unit)
    if op == "&&":
        return Value(bool(a.value) and bool(b.value))
    if op == "||":
        return Value(bool(a.value) or bool(b.value))
    compare = {"==": a.value == b.value, "!=": a.value != b.value, "<": a.value < b.value,
               ">": a.value > b.value, "<=": a.value <= b.value, ">=": a.value >= b.value}
    return Value(compare[op])


def split_ternary(expr):
    """`a ? b : c` at the top level, as its three parts, or None."""
    depth, question = 0, None
    for i, c in enumerate(expr):
        if c in "([{":
            depth += 1
        elif c in ")]}":
            depth -= 1
        elif depth == 0 and c == "?" and question is None:
            question = i
        elif depth == 0 and c == ":" and question is not None:
            # The colon that pairs with the first question mark: skip nested ones.
            inner = expr[question + 1:i]
            if inner.count("?") == inner.count(":"):
                return expr[:question], expr[question + 1:i], expr[i + 1:]
    return None


# ---------------------------------------------------------------------------
# Reading the Slint
# ---------------------------------------------------------------------------

BLOCK = re.compile(r"^(?:export\s+)?(component|global)\s+([A-Za-z_][\w-]*)\b.*\{\s*$")
ELEMENT = re.compile(r"^(\s*)([a-z][\w-]*)\s*:=\s*[A-Za-z_][\w]*\s*\{\s*$")
PROPERTY = re.compile(
    r"^(?:(?:in|out|in-out|private)\s+)?property\s*<([^>]+)>\s*([\w-]+)\s*(?::(.*))?$"
)
ASSIGNMENT = re.compile(r"^([a-z][\w-]*)\s*:(?!=)\s*(.*)$")
KEYWORDS = {"callback", "function", "pure", "public", "animate", "changed", "if", "for",
            "states", "transitions", "import", "export"}
UNITS = {"length": "px", "duration": "ms", "color": "color", "string": "string"}


class Scope:
    """A component, global or named element, and the properties set at its top level."""

    def __init__(self, name):
        self.name = name
        self.props = {}  # name -> (unit hint, expression)
        self.elements = {}  # named elements, for a component
        self.component = self


def indent_of(line):
    return len(line) - len(line.lstrip(" "))


def block_end(lines, start, indent):
    for j in range(start + 1, len(lines)):
        if lines[j].strip().startswith("}") and indent_of(lines[j]) == indent:
            return j
    return len(lines)


def read_props(lines, start, end, indent, scope):
    """Collect the `name: expr;` and `property <T> name: expr;` lines at `indent`."""
    i = start
    while i < end:
        line = lines[i]
        text = line.strip()
        if indent_of(line) != indent or not text or text.startswith("//"):
            i += 1
            continue
        first = re.split(r"[\s:<]", text, maxsplit=1)[0]
        m = PROPERTY.match(text)
        hint = None
        if m:
            hint, name, rest = UNITS.get(m.group(1).strip()), m.group(2), m.group(3)
        elif first not in KEYWORDS and ASSIGNMENT.match(text) and ":=" not in text:
            m = ASSIGNMENT.match(text)
            name, rest = m.group(1), m.group(2)
        else:
            i += 1
            continue
        if rest is None:
            i += 1
            continue
        expr, depth = rest, 0
        # Carry on over continuation lines until the statement's semicolon.
        while True:
            depth = sum(expr.count(c) for c in "([{") - sum(expr.count(c) for c in ")]}")
            if depth <= 0 and expr.rstrip().endswith(";"):
                break
            i += 1
            if i >= end:
                break
            expr += " " + lines[i].strip()
        expr = expr.rstrip().rstrip(";").strip()
        if not expr.startswith(("{", "[", "@")):
            scope.props[name] = (hint, re.sub(r"//.*", "", expr).strip())
        i += 1


def read_slint():
    scopes = {}
    for file in sorted((PLAYER / "ui").glob("*.slint")):
        lines = file.read_text(encoding="utf-8").split("\n")
        i = 0
        while i < len(lines):
            m = BLOCK.match(lines[i])
            if not m:
                i += 1
                continue
            scope = Scope(m.group(2))
            end = block_end(lines, i, 0)
            read_props(lines, i + 1, end, 4, scope)
            for j in range(i + 1, end):
                e = ELEMENT.match(lines[j])
                if e:
                    element = Scope(e.group(2))
                    element.component = scope
                    indent = len(e.group(1))
                    read_props(lines, j + 1, block_end(lines, j, indent), indent + 4, element)
                    scope.elements[e.group(2)] = element
            scopes[scope.name] = scope
            i = end + 1
    return scopes


# ---------------------------------------------------------------------------
# Reading the Rust and GLSL
# ---------------------------------------------------------------------------


def rust_value(text):
    text = text.strip()
    duration = re.fullmatch(r"(?:[\w:]*::)?Duration::from_(millis|secs)\((\d+(?:\.\d+)?)\)", text)
    if duration:
        scale = 1000 if duration.group(1) == "secs" else 1
        return Value(float(duration.group(2)) * scale, "ms")
    if re.fullmatch(r"-?\d+(?:\.\d+)?(?:_?f(?:32|64))?", text):
        return Value(float(re.sub(r"_?f(?:32|64)$", "", text)))
    array = re.fullmatch(r"\[(.*)\]", text)
    if array:
        return Value([rust_value(x).value for x in array.group(1).split(",") if x.strip()])
    raise DesignError(f"cannot read the constant {text!r}")


def module_value(module, path):
    file = MODULES.get(module)
    if file is None:
        raise DesignError(f"unknown module {module}")
    text = file.read_text(encoding="utf-8")
    if len(path) == 1:
        m = re.search(rf"\bconst\s+(?:float\s+|int\s+)?{path[0]}\s*(?::[^=]+)?=\s*([^;]+);", text)
        if not m:
            raise DesignError(f"no constant {path[0]} in {file.name}")
        return rust_value(m.group(1))
    # `Type.field`: a field of the type's `Default`.
    m = re.search(rf"impl Default for {path[0]}\b.*?\n}}", text, re.S)
    if not m:
        raise DesignError(f"no Default for {path[0]} in {file.name}")
    field = re.search(rf"\b{path[1]}\s*:\s*(\[[^\]]*\]|[^,\n]+),", m.group(0))
    if not field:
        raise DesignError(f"no {path[1]} in {path[0]}'s Default")
    return rust_value(field.group(1))


# ---------------------------------------------------------------------------
# Resolving a path to a value
# ---------------------------------------------------------------------------


class Resolver:
    def __init__(self):
        self.scopes = read_slint()
        self.seen = []

    def scope_prop(self, scope, name, branches):
        if name not in scope.props:
            raise DesignError(f"{scope.name} has no {name}")
        hint, expr = scope.props[name]
        for branch in branches:
            parts = split_ternary(expr)
            if parts is None:
                raise DesignError(f"{scope.name}.{name} is not a conditional")
            expr = parts[1] if branch == "then" else parts[2]
        key = (scope.component.name, scope.name, name, tuple(branches))
        if key in self.seen:
            raise DesignError(f"{scope.name}.{name} depends on itself")
        self.seen.append(key)
        try:
            value = self.evaluate(expr, scope)
        finally:
            self.seen.pop()
        if hint == "px" and value.unit is None and isinstance(value.value, (int, float)):
            value.unit = "px"
        return value

    def resolve(self, path, scope=None):
        branches = re.findall(r"@(then|else)", path)
        parts = re.sub(r"@(then|else)", "", path).split(".")
        if parts[0] in MODULES:
            return module_value(parts[0], parts[1:])
        if scope is not None and parts[0] in ("root", "self"):
            target = scope.component if parts[0] == "root" else scope
            return self.scope_prop(target, parts[1], branches)
        if scope is not None and parts[0] in scope.component.elements and len(parts) == 2:
            return self.scope_prop(scope.component.elements[parts[0]], parts[1], branches)
        if parts[0] in self.scopes:
            owner = self.scopes[parts[0]]
            if len(parts) == 3:
                if parts[1] not in owner.elements:
                    raise DesignError(f"{parts[0]} has no element {parts[1]}")
                return self.scope_prop(owner.elements[parts[1]], parts[2], branches)
            if len(parts) == 2:
                return self.scope_prop(owner, parts[1], branches)
        raise DesignError(f"cannot resolve {path}")

    def names(self, path):
        """Raise unless the prose's `path` is something in the code. Only that
        it exists: a value the prose names may depend on what is playing."""
        parts = re.sub(r"@(then|else)", "", path).split(".")
        if parts[0] in MODULES:
            module_value(parts[0], parts[1:])
            return
        scope = self.scopes[parts[0]]
        if len(parts) == 3:
            if parts[1] not in scope.elements:
                raise DesignError(f"{parts[0]} has no element {parts[1]}")
            scope, parts = scope.elements[parts[1]], parts[1:]
        if len(parts) != 2 or (parts[1] not in scope.props and parts[1] not in scope.elements):
            raise DesignError(f"{scope.name} has no {'.'.join(parts[1:])}")

    def evaluate(self, expr, scope=None):
        return Parser(tokenize(expr), lambda path: self.resolve(path, scope)).parse()

    def placeholder(self, inner):
        """The value of one `{{ ... }}`."""
        inner = inner.strip()
        if inner.startswith("demo "):
            return Value(inner[5:].strip(), "string")
        bare = False
        if inner.endswith("| n"):
            inner, bare = inner[:-3].strip(), True
        value = self.evaluate(inner)
        if bare:
            value.unit = None
        return value


# ---------------------------------------------------------------------------
# Rendering
# ---------------------------------------------------------------------------


def raw_values(text, where):
    stripped = PLACEHOLDER.sub("", text)
    return [f"{where}: {m.group(0)}" for m in RAW_VALUE.finditer(stripped)]


def unknown_names(text, where, resolver):
    problems = []
    for m in NAMED.finditer(text):
        path = m.group(1)
        first = path.split(".")[0]
        if FILE_NAME.search(path) or (first not in MODULES and first not in resolver.scopes):
            continue
        try:
            resolver.names(path)
        except DesignError as e:
            problems.append(f"{where}: `{path}` is not in the code ({e})")
    return problems


def render_text(template, resolver):
    def substitute(m):
        try:
            return resolver.placeholder(m.group(1)).text()
        except DesignError as e:
            raise DesignError(f"{{{{{m.group(1)}}}}}: {e}") from None

    return PLACEHOLDER.sub(substitute, template)


def render_frontmatter(template, resolver):
    """The front matter, each value followed by the token it came from."""
    out = []
    for line in template.split("\n"):
        sources = [source.strip() for source in PLACEHOLDER.findall(line)]
        rendered = render_text(line, resolver)
        out.append(f"{rendered}  # {', '.join(sources)}" if sources else rendered)
    return "\n".join(out)


def render_json(node, resolver):
    if isinstance(node, dict):
        return {k: render_json(v, resolver) for k, v in node.items()}
    if isinstance(node, list):
        return [render_json(v, resolver) for v in node]
    if isinstance(node, str):
        whole = PLACEHOLDER.fullmatch(node.strip())
        if whole:
            try:
                value = resolver.placeholder(whole.group(1))
            except DesignError as e:
                raise DesignError(f"{node}: {e}") from None
            if value.unit in (None,) and not isinstance(value.value, str):
                return value.json()
        return render_text(node, resolver)
    return node


def split_design_md():
    text = DESIGN_MD.read_text(encoding="utf-8")
    m = re.match(r"---\n(.*?)\n---\n", text, re.S)
    if not m:
        raise DesignError("DESIGN.md has no front matter")
    return m.group(1), text[m.end():]


def paragraphs(lines):
    out, current = [], []
    for line in lines:
        if line.strip():
            current.append(line.strip())
        elif current:
            out.append(" ".join(current))
            current = []
    if current:
        out.append(" ".join(current))
    return out


def bullets(lines):
    out = []
    for line in lines:
        if line.startswith("- "):
            out.append(line[2:].strip())
        elif line.startswith("  ") and out:
            out[-1] += " " + line.strip()
    return out


def narrative(body):
    """The sidecar's narrative, read from DESIGN.md's prose."""
    # Each h2 section's lines, and separately the lines under a "Named Rules"
    # heading, where every bold-led paragraph is a rule.
    sections, named, current, in_named = {}, [], None, False
    for line in body.split("\n"):
        heading = re.match(r"^(##|###) (.+)$", line)
        if heading and heading.group(1) == "##":
            current, in_named = heading.group(2), False
            sections[current] = []
            continue
        if heading:
            in_named = heading.group(2) == "Named Rules"
            named.append((current, ""))
        if current is not None:
            sections[current].append(line)
            if in_named and not heading:
                named.append((current, line))

    overview = sections.get("Overview", [])
    north = re.search(r'\*\*Creative North Star: "([^"]+)"\*\*', "\n".join(overview))
    key_at = next((i for i, l in enumerate(overview) if l.startswith("**Key Characteristics:**")), len(overview))
    prose = [l for l in overview[:key_at] if not l.startswith("**Creative North Star")]

    names = {"Colors": "colors", "Typography": "typography", "Layout": "layout",
             "Elevation & Depth": "elevation", "Shapes": "shapes", "Components": "components"}
    rules, found = [], set()

    def add(section, name, text):
        if name not in found:
            found.add(name)
            rules.append({"name": name, "section": names.get(section, section.lower()), "body": text})

    for section, lines in sections.items():
        for paragraph in paragraphs(lines):
            rule = re.match(r"\*\*(The .+? Rule)\.\*\* (.+)", paragraph)
            if rule:
                add(section, rule.group(1), rule.group(2))
    by_section = {}
    for section, line in named:
        by_section.setdefault(section, []).append(line)
    for section, lines in by_section.items():
        for paragraph in paragraphs(lines):
            rule = re.match(r"\*\*(.+?)\.\*\* (.+)", paragraph)
            if rule:
                add(section, rule.group(1), rule.group(2))
    order = {name: i for i, name in enumerate(re.findall(r"^\*\*(.+?)\.\*\*", body, re.M))}
    rules.sort(key=lambda r: order.get(r["name"], len(order)))

    dos_donts = sections.get("Do's and Don'ts", [])
    split = next((i for i, l in enumerate(dos_donts) if l.startswith("### Don't")), len(dos_donts))
    return {
        "northStar": north.group(1) if north else "",
        "overview": "\n\n".join(paragraphs(prose)),
        "keyCharacteristics": bullets(overview[key_at + 1:]),
        "rules": rules,
        "dos": bullets(dos_donts[:split]),
        "donts": bullets(dos_donts[split:]),
    }


def render():
    """The front matter and the sidecar as they should be, and any problems."""
    resolver = Resolver()
    problems = []
    frontmatter_template = FRONTMATTER_TEMPLATE.read_text(encoding="utf-8")
    sidecar_template = SIDECAR_TEMPLATE.read_text(encoding="utf-8")
    problems += raw_values(frontmatter_template, FRONTMATTER_TEMPLATE.name)
    problems += raw_values(sidecar_template, SIDECAR_TEMPLATE.name)

    _, body = split_design_md()
    for number, line in enumerate(body.split("\n"), start=1):
        problems += raw_values(line, f"DESIGN.md prose, line {number}")
        problems += unknown_names(line, f"DESIGN.md prose, line {number}", resolver)

    frontmatter = render_frontmatter(frontmatter_template, resolver).rstrip("\n")
    sidecar = render_json(json.loads(sidecar_template), resolver)
    sidecar["narrative"] = narrative(body)
    return frontmatter, sidecar, problems


def sidecar_text(sidecar):
    return json.dumps(sidecar, indent=2, ensure_ascii=False) + "\n"


def without_date(sidecar):
    return {k: v for k, v in sidecar.items() if k != "generatedAt"}


def write():
    frontmatter, sidecar, problems = render()
    for problem in problems:
        print(f"warning: written out rather than named: {problem}", file=sys.stderr)
    _, body = split_design_md()
    DESIGN_MD.write_text(f"---\n{frontmatter}\n---\n{body}", encoding="utf-8")

    old = json.loads(SIDECAR.read_text(encoding="utf-8")) if SIDECAR.exists() else {}
    stamp = old.get("generatedAt") if without_date(old) == without_date(sidecar) else None
    sidecar["generatedAt"] = stamp or datetime.datetime.now(datetime.timezone.utc).strftime(
        "%Y-%m-%dT%H:%M:%SZ")
    ordered = {"schemaVersion": sidecar.pop("schemaVersion"), "generatedAt": sidecar.pop("generatedAt")}
    ordered.update(sidecar)
    SIDECAR.write_text(sidecar_text(ordered), encoding="utf-8")
    print("wrote DESIGN.md's front matter and .impeccable/design.json")


def check():
    frontmatter, sidecar, problems = render()
    current_frontmatter, _ = split_design_md()
    if current_frontmatter != frontmatter:
        problems.append("DESIGN.md's front matter is stale: run scripts/design.py write")
    current = json.loads(SIDECAR.read_text(encoding="utf-8")) if SIDECAR.exists() else {}
    if without_date(current) != without_date(sidecar):
        problems.append(".impeccable/design.json is stale: run scripts/design.py write")
    for problem in problems:
        print(problem, file=sys.stderr)
    if problems:
        sys.exit(1)
    print("design files are in step with the code")


def main():
    command = sys.argv[1] if len(sys.argv) > 1 else ""
    try:
        if command == "write":
            write()
        elif command == "check":
            check()
        else:
            print(__doc__.strip())
            sys.exit(2)
    except DesignError as e:
        print(f"error: {e}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
