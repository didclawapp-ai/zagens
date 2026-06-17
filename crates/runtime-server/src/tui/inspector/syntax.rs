//! Lightweight syntax highlighting for the file-preview pane.
//!
//! Uses a single-pass state machine per line.  No external dependencies.
//! Accuracy is "good enough for narrow TUI columns" — it handles the most
//! visible elements: comments, string literals, and language keywords.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;

// ── palette shortcuts ──────────────────────────────────────────────────────
// All styles use only foreground colour so they can be applied in both the
// inspector pane (dark background) and the transcript view (different bg).

fn comment() -> Style {
    Style::default().fg(Color::Rgb(0x62, 0x72, 0xa4)) // dim blue-gray
}
fn string_lit() -> Style {
    Style::default().fg(Color::Rgb(0xf1, 0xfa, 0x8c)) // Dracula yellow
}
fn keyword() -> Style {
    Style::default()
        .fg(Color::Rgb(0xff, 0x79, 0xc6))
        .add_modifier(Modifier::BOLD) // Dracula pink / bold
}
fn attr_decor() -> Style {
    Style::default().fg(Color::Rgb(0x8b, 0xe9, 0xfd)) // Dracula cyan
}
fn number_lit() -> Style {
    Style::default().fg(Color::Rgb(0xbd, 0x93, 0xf9)) // Dracula purple
}
fn type_hint() -> Style {
    Style::default().fg(Color::Rgb(0x8b, 0xe9, 0xfd)) // cyan (same as attr)
}
/// Plain (non-highlighted) text style — no colour override, inherits caller's style.
fn plain() -> Style {
    Style::default()
}

// ── language detection ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Rust,
    TypeScript,
    JavaScript,
    Python,
    Toml,
    Yaml,
    Json,
    Shell,
    Mermaid,
    Markdown,
    Plain,
}

impl Lang {
    pub fn from_path(path: &str) -> Self {
        let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        match ext.as_str() {
            "rs" => Self::Rust,
            "ts" | "tsx" => Self::TypeScript,
            "js" | "jsx" | "mjs" | "cjs" => Self::JavaScript,
            "py" | "pyi" => Self::Python,
            "toml" => Self::Toml,
            "yaml" | "yml" => Self::Yaml,
            "json" | "jsonc" => Self::Json,
            "sh" | "bash" | "zsh" | "fish" => Self::Shell,
            "md" | "mdx" | "markdown" => Self::Markdown,
            // Language tags used directly in code fences
            "mermaid" => Self::Mermaid,
            _ => Self::Plain,
        }
    }

    fn line_comment_prefix(self) -> &'static str {
        match self {
            Self::Rust | Self::TypeScript | Self::JavaScript => "//",
            Self::Python | Self::Toml | Self::Yaml | Self::Shell => "#",
            _ => "",
        }
    }

    fn keywords(self) -> &'static [&'static str] {
        match self {
            Self::Rust => RUST_KW,
            Self::TypeScript | Self::JavaScript => TS_KW,
            Self::Python => PY_KW,
            _ => &[],
        }
    }

    fn type_names(self) -> &'static [&'static str] {
        match self {
            Self::Rust => RUST_TYPES,
            _ => &[],
        }
    }
}

static RUST_KW: &[&str] = &[
    "pub", "fn", "struct", "enum", "impl", "trait", "use", "mod", "let", "mut", "const", "static",
    "type", "where", "unsafe", "extern", "if", "else", "match", "for", "while", "loop", "return",
    "break", "continue", "crate", "self", "super", "async", "await", "dyn", "ref", "in", "as",
    "move", "true", "false", "Some", "None", "Ok", "Err",
];
static RUST_TYPES: &[&str] = &[
    "String", "str", "bool", "u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64",
    "i128", "isize", "f32", "f64", "Vec", "Option", "Result", "Box", "Rc", "Arc", "HashMap",
    "HashSet", "Path", "PathBuf", "Duration", "Instant",
];
static TS_KW: &[&str] = &[
    "const",
    "let",
    "var",
    "function",
    "class",
    "interface",
    "type",
    "import",
    "export",
    "from",
    "return",
    "if",
    "else",
    "for",
    "while",
    "async",
    "await",
    "new",
    "this",
    "extends",
    "implements",
    "true",
    "false",
    "null",
    "undefined",
    "void",
    "typeof",
    "instanceof",
    "readonly",
    "public",
    "private",
    "protected",
    "static",
    "abstract",
];
static PY_KW: &[&str] = &[
    "def", "class", "import", "from", "return", "if", "elif", "else", "for", "while", "with", "as",
    "try", "except", "finally", "raise", "pass", "break", "continue", "yield", "lambda", "True",
    "False", "None", "and", "or", "not", "in", "is", "async", "await", "global", "nonlocal",
];

// ── main entry point ───────────────────────────────────────────────────────

/// Tokenise `line` and return coloured `Span`s.
/// Falls back to a plain single span for unrecognised languages.
pub fn highlight_line(line: &str, lang: Lang) -> Vec<Span<'static>> {
    if line.is_empty() {
        return vec![Span::raw("".to_string())];
    }
    let trimmed = line.trim_start();

    // ── full-line comment (prefix match) ──────────────────────────────────
    let cp = lang.line_comment_prefix();
    if !cp.is_empty() && trimmed.starts_with(cp) {
        return vec![Span::styled(line.to_string(), comment())];
    }

    match lang {
        Lang::Rust => tokenize_c_like(line, lang),
        Lang::TypeScript | Lang::JavaScript => tokenize_c_like(line, lang),
        Lang::Python => tokenize_python(line),
        Lang::Toml => tokenize_toml(line, trimmed),
        Lang::Json => tokenize_json(line, trimmed),
        Lang::Yaml => tokenize_yaml(line, trimmed),
        Lang::Mermaid => tokenize_mermaid(line, trimmed),
        Lang::Markdown => tokenize_markdown(line, trimmed),
        _ => vec![Span::styled(line.to_string(), plain())],
    }
}

// ── C-like (Rust / TS / JS) tokeniser ─────────────────────────────────────

fn tokenize_c_like(line: &str, lang: Lang) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();
    let mut i = 0;
    // Plain text accumulator
    let mut plain_buf = String::new();

    macro_rules! flush_plain {
        () => {
            if !plain_buf.is_empty() {
                spans.push(Span::styled(plain_buf.clone(), plain()));
                plain_buf.clear();
            }
        };
    }

    while i < len {
        // ── Rust attributes #[...] or #![ ──────────────────────────────
        if lang == Lang::Rust && chars[i] == '#' {
            flush_plain!();
            let rest: String = chars[i..].iter().collect();
            spans.push(Span::styled(rest, attr_decor()));
            return spans;
        }

        // ── line comment ────────────────────────────────────────────────
        let cp = lang.line_comment_prefix();
        if !cp.is_empty() && chars[i..].iter().collect::<String>().starts_with(cp) {
            flush_plain!();
            let rest: String = chars[i..].iter().collect();
            spans.push(Span::styled(rest, comment()));
            return spans;
        }

        // ── string / char literal ────────────────────────────────────────
        if chars[i] == '"' || (chars[i] == '\'' && lang != Lang::Rust) {
            flush_plain!();
            let q = chars[i];
            let mut s = String::new();
            s.push(q);
            i += 1;
            while i < len {
                s.push(chars[i]);
                if chars[i] == q && (s.len() < 2 || chars[i - 1] != '\\') {
                    i += 1;
                    break;
                }
                i += 1;
            }
            spans.push(Span::styled(s, string_lit()));
            continue;
        }

        // ── Rust byte/char literal 'x' or b"..." ─────────────────────────
        if lang == Lang::Rust && chars[i] == '\'' {
            // char literal: collect up to closing ' (simple)
            flush_plain!();
            let mut s = String::from('\'');
            i += 1;
            let mut depth = 0usize;
            while i < len && depth < 4 {
                s.push(chars[i]);
                if chars[i] == '\'' && s.len() > 1 {
                    i += 1;
                    break;
                }
                i += 1;
                depth += 1;
            }
            spans.push(Span::styled(s, string_lit()));
            continue;
        }

        // ── number literal ────────────────────────────────────────────────
        if chars[i].is_ascii_digit() {
            // only highlight if preceded by non-alnum (word boundary)
            let prev_alnum = i > 0 && (chars[i - 1].is_alphanumeric() || chars[i - 1] == '_');
            if !prev_alnum {
                flush_plain!();
                let mut s = String::new();
                while i < len
                    && (chars[i].is_ascii_alphanumeric() || chars[i] == '_' || chars[i] == '.')
                {
                    s.push(chars[i]);
                    i += 1;
                }
                spans.push(Span::styled(s, number_lit()));
                continue;
            }
        }

        // ── identifier / keyword ──────────────────────────────────────────
        if chars[i].is_alphabetic() || chars[i] == '_' {
            // Collect the full identifier
            let start = i;
            while i < len && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let ident: String = chars[start..i].iter().collect();
            // Check keyword / type
            if lang.keywords().contains(&ident.as_str()) {
                flush_plain!();
                spans.push(Span::styled(ident, keyword()));
            } else if lang.type_names().contains(&ident.as_str()) {
                flush_plain!();
                spans.push(Span::styled(ident, type_hint()));
            } else {
                plain_buf.push_str(&ident);
            }
            continue;
        }

        plain_buf.push(chars[i]);
        i += 1;
    }

    flush_plain!();
    if spans.is_empty() {
        spans.push(Span::styled(line.to_string(), plain()));
    }
    spans
}

// ── Python ────────────────────────────────────────────────────────────────

fn tokenize_python(line: &str) -> Vec<Span<'static>> {
    // Python shares enough structure with C-like to reuse the tokeniser.
    tokenize_c_like(line, Lang::Python)
}

// ── TOML ──────────────────────────────────────────────────────────────────

fn tokenize_toml(line: &str, trimmed: &str) -> Vec<Span<'static>> {
    if trimmed.starts_with('[') {
        return vec![Span::styled(
            line.to_string(),
            attr_decor().add_modifier(Modifier::BOLD),
        )];
    }
    if trimmed.starts_with('"') || trimmed.starts_with('\'') {
        return vec![Span::styled(line.to_string(), string_lit())];
    }
    // key = value: colour key part in attr style
    if let Some(eq) = line.find('=') {
        let (k, v) = line.split_at(eq);
        return vec![
            Span::styled(k.to_string(), attr_decor()),
            Span::styled(v.to_string(), plain()),
        ];
    }
    vec![Span::styled(line.to_string(), plain())]
}

// ── JSON ──────────────────────────────────────────────────────────────────

fn tokenize_json(line: &str, trimmed: &str) -> Vec<Span<'static>> {
    // Highlight JSON keys (strings followed by ':')
    if let Some(pos) = trimmed.find("\":") {
        let indent = line.len() - trimmed.len();
        let key_end = pos + 2 + indent;
        let (key_part, rest) = line.split_at(key_end.min(line.len()));
        return vec![
            Span::styled(key_part.to_string(), attr_decor()),
            Span::styled(rest.to_string(), plain()),
        ];
    }
    vec![Span::styled(line.to_string(), plain())]
}

// ── YAML ──────────────────────────────────────────────────────────────────

fn tokenize_yaml(line: &str, trimmed: &str) -> Vec<Span<'static>> {
    // key: value → highlight key
    if let Some(pos) = trimmed.find(": ") {
        let indent = line.len() - trimmed.len();
        let key_end = pos + indent;
        let (k, v) = line.split_at(key_end.min(line.len()));
        return vec![
            Span::styled(k.to_string(), attr_decor()),
            Span::styled(v.to_string(), plain()),
        ];
    }
    // YAML anchor / alias / document markers
    if trimmed.starts_with("- ") || trimmed.starts_with("---") || trimmed.starts_with("...") {
        return vec![Span::styled(line.to_string(), keyword())];
    }
    vec![Span::styled(line.to_string(), plain())]
}

// ── Mermaid ───────────────────────────────────────────────────────────────

/// Mermaid top-level diagram-type keywords.
static MERMAID_DIAGRAM_KW: &[&str] = &[
    "graph",
    "flowchart",
    "sequenceDiagram",
    "classDiagram",
    "stateDiagram",
    "stateDiagram-v2",
    "erDiagram",
    "gantt",
    "pie",
    "gitGraph",
    "mindmap",
    "timeline",
    "journey",
    "quadrantChart",
    "xyChart",
    "block",
];
/// Mermaid structural keywords.
static MERMAID_STRUCT_KW: &[&str] = &[
    "subgraph",
    "end",
    "section",
    "title",
    "participant",
    "actor",
    "activate",
    "deactivate",
    "loop",
    "alt",
    "opt",
    "par",
    "critical",
    "break",
    "rect",
    "note",
    "over",
    "left",
    "right",
    "of",
    "class",
    "link",
    "click",
    "style",
    "classDef",
    "direction",
];
/// Mermaid graph direction tokens.
static MERMAID_DIR_KW: &[&str] = &["TD", "TB", "BT", "LR", "RL", "TOP", "DOWN"];

fn tokenize_mermaid(line: &str, trimmed: &str) -> Vec<Span<'static>> {
    // Comment
    if trimmed.starts_with("%%") {
        return vec![Span::styled(line.to_string(), comment())];
    }

    let mut spans: Vec<Span<'static>> = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();
    let mut i = 0;
    let mut plain_buf = String::new();

    macro_rules! flush {
        () => {
            if !plain_buf.is_empty() {
                spans.push(Span::styled(plain_buf.clone(), plain()));
                plain_buf.clear();
            }
        };
    }

    while i < len {
        // %% inline comment
        if i + 1 < len && chars[i] == '%' && chars[i + 1] == '%' {
            flush!();
            let rest: String = chars[i..].iter().collect();
            spans.push(Span::styled(rest, comment()));
            return spans;
        }

        // String in quotes (node labels)
        if chars[i] == '"' || chars[i] == '\'' {
            flush!();
            let q = chars[i];
            let mut s = String::new();
            s.push(q);
            i += 1;
            while i < len {
                s.push(chars[i]);
                if chars[i] == q {
                    i += 1;
                    break;
                }
                i += 1;
            }
            spans.push(Span::styled(s, string_lit()));
            continue;
        }

        // Arrows: --> --- -.-> ==> --x --o etc.
        // Detect a run of `-`, `.`, `=`, `>`, `x`, `o` that form an edge
        if chars[i] == '-' || chars[i] == '=' {
            let start = i;
            while i < len
                && (chars[i] == '-'
                    || chars[i] == '.'
                    || chars[i] == '>'
                    || chars[i] == '='
                    || chars[i] == 'x'
                    || chars[i] == 'o'
                    || chars[i] == '|')
            {
                i += 1;
            }
            if i > start + 1 {
                flush!();
                let arrow: String = chars[start..i].iter().collect();
                spans.push(Span::styled(arrow, attr_decor()));
                continue;
            } else {
                i = start;
            }
        }

        // Node bracket labels: [text] (text) {text} [(text)] ([text])
        if chars[i] == '[' || chars[i] == '(' || chars[i] == '{' {
            flush!();
            let open = chars[i];
            let close = match open {
                '[' => ']',
                '(' => ')',
                _ => '}',
            };
            let mut s = String::new();
            s.push(open);
            i += 1;
            let mut depth = 1usize;
            while i < len && depth > 0 {
                if chars[i] == open {
                    depth += 1;
                }
                if chars[i] == close {
                    depth -= 1;
                }
                s.push(chars[i]);
                i += 1;
            }
            spans.push(Span::styled(s, string_lit()));
            continue;
        }

        // Identifier / keyword
        if chars[i].is_alphabetic() || chars[i] == '_' {
            let start = i;
            while i < len && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '-') {
                i += 1;
            }
            let ident: String = chars[start..i].iter().collect();
            if MERMAID_DIAGRAM_KW.contains(&ident.as_str())
                || MERMAID_STRUCT_KW.contains(&ident.as_str())
            {
                flush!();
                spans.push(Span::styled(ident, keyword()));
            } else if MERMAID_DIR_KW.contains(&ident.as_str()) {
                flush!();
                spans.push(Span::styled(ident, attr_decor()));
            } else {
                plain_buf.push_str(&ident);
            }
            continue;
        }

        plain_buf.push(chars[i]);
        i += 1;
    }

    flush!();
    if spans.is_empty() {
        spans.push(Span::styled(line.to_string(), plain()));
    }
    spans
}

// ── Markdown ──────────────────────────────────────────────────────────────

/// Dracula foreground — used as the explicit body-text colour so that the
/// bright white is not lost when multi-span lines are clipped or patched.
fn md_body() -> Style {
    Style::default().fg(Color::Rgb(0xf8, 0xf8, 0xf2))
}

fn md_h1() -> Style {
    Style::default()
        .fg(Color::Rgb(0xff, 0x79, 0xc6))
        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
}
fn md_h2() -> Style {
    Style::default()
        .fg(Color::Rgb(0xff, 0x79, 0xc6))
        .add_modifier(Modifier::BOLD)
}
fn md_h3() -> Style {
    Style::default()
        .fg(Color::Rgb(0xbd, 0x93, 0xf9))
        .add_modifier(Modifier::BOLD)
}
fn md_h4() -> Style {
    Style::default().fg(Color::Rgb(0x8b, 0xe9, 0xfd))
}
fn md_code_inline() -> Style {
    Style::default().fg(Color::Rgb(0xf1, 0xfa, 0x8c)) // Dracula yellow
}
fn md_link_text() -> Style {
    Style::default()
        .fg(Color::Rgb(0x8b, 0xe9, 0xfd))
        .add_modifier(Modifier::UNDERLINED)
}
fn md_link_url() -> Style {
    Style::default().fg(Color::Rgb(0x62, 0x72, 0xa4))
}
fn md_bold() -> Style {
    Style::default()
        .fg(Color::Rgb(0xf8, 0xf8, 0xf2))
        .add_modifier(Modifier::BOLD)
}
fn md_italic() -> Style {
    Style::default()
        .fg(Color::Rgb(0xf8, 0xf8, 0xf2))
        .add_modifier(Modifier::ITALIC)
}
fn md_blockquote() -> Style {
    Style::default()
        .fg(Color::Rgb(0x62, 0x72, 0xa4))
        .add_modifier(Modifier::ITALIC)
}
fn md_fence() -> Style {
    Style::default().fg(Color::Rgb(0x62, 0x72, 0xa4))
}
fn md_rule() -> Style {
    Style::default().fg(Color::Rgb(0x44, 0x47, 0x5a))
}
fn md_bullet() -> Style {
    Style::default()
        .fg(Color::Rgb(0xff, 0x79, 0xc6))
        .add_modifier(Modifier::BOLD)
}

fn tokenize_markdown(line: &str, trimmed: &str) -> Vec<Span<'static>> {
    // Blank line
    if trimmed.is_empty() {
        return vec![Span::raw(line.to_string())];
    }

    // Code fence  ``` or ~~~
    if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
        return vec![Span::styled(line.to_string(), md_fence())];
    }

    // Horizontal rule  --- / *** / ___  (only dashes/stars/underscores + spaces)
    {
        let hr_stripped = trimmed.replace(['-', '*', '_', ' '], "");
        if hr_stripped.is_empty()
            && trimmed.len() >= 3
            && (trimmed.contains("---") || trimmed.contains("***") || trimmed.contains("___"))
        {
            let rule = "─".repeat(line.len().min(60));
            return vec![Span::styled(rule, md_rule())];
        }
    }

    // ATX Headings  # / ## / ### / ####
    for (marker, style) in [
        ("#### ", md_h4()),
        ("### ", md_h3()),
        ("## ", md_h2()),
        ("# ", md_h1()),
    ] {
        if let Some(heading) = trimmed.strip_prefix(marker) {
            let indent_len = line.len() - trimmed.len();
            let indent = &line[..indent_len];
            let mut spans = Vec::new();
            if !indent.is_empty() {
                spans.push(Span::raw(indent.to_string()));
            }
            spans.push(Span::styled(marker.to_string(), style));
            spans.extend(md_inline_spans(heading, style));
            return spans;
        }
    }

    // Blockquote  >
    if let Some(rest) = trimmed
        .strip_prefix("> ")
        .or_else(|| if trimmed == ">" { Some("") } else { None })
    {
        let indent_len = line.len() - trimmed.len();
        let mut spans = Vec::new();
        if indent_len > 0 {
            spans.push(Span::raw(line[..indent_len].to_string()));
        }
        spans.push(Span::styled("> ".to_string(), md_blockquote()));
        if !rest.is_empty() {
            spans.extend(md_inline_spans(rest, md_blockquote()));
        }
        return spans;
    }

    // Task list items first (before generic bullets)
    for (prefix, marker) in [("- [ ] ", "☐ "), ("- [x] ", "☑ "), ("- [X] ", "☑ ")] {
        if let Some(item) = trimmed.strip_prefix(prefix) {
            let indent_len = line.len() - trimmed.len();
            let mut spans = Vec::new();
            if indent_len > 0 {
                spans.push(Span::raw(line[..indent_len].to_string()));
            }
            spans.push(Span::styled(marker.to_string(), md_bullet()));
            spans.extend(md_inline_spans(item, md_body()));
            return spans;
        }
    }

    // Unordered list  - / * / +
    for bullet in ["* ", "+ ", "- "] {
        if let Some(item) = trimmed.strip_prefix(bullet) {
            let indent_len = line.len() - trimmed.len();
            let mut spans = Vec::new();
            if indent_len > 0 {
                spans.push(Span::raw(line[..indent_len].to_string()));
            }
            spans.push(Span::styled("• ".to_string(), md_bullet()));
            spans.extend(md_inline_spans(item, md_body()));
            return spans;
        }
    }

    // Ordered list  1. / 2.
    {
        let mut num_end = 0;
        for ch in trimmed.chars() {
            if ch.is_ascii_digit() {
                num_end += 1;
            } else {
                break;
            }
        }
        if num_end > 0 && num_end < 6 {
            let after = &trimmed[num_end..];
            if let Some(item) = after
                .strip_prefix(". ")
                .or_else(|| after.strip_prefix(") "))
            {
                let indent_len = line.len() - trimmed.len();
                let num_marker = &trimmed[..num_end + 2]; // "1. "
                let mut spans = Vec::new();
                if indent_len > 0 {
                    spans.push(Span::raw(line[..indent_len].to_string()));
                }
                spans.push(Span::styled(num_marker.to_string(), md_bullet()));
                spans.extend(md_inline_spans(item, md_body()));
                return spans;
            }
        }
    }

    // Table row  | col | col |
    if trimmed.starts_with('|') {
        return tokenize_md_table_row(line, trimmed);
    }

    // Plain paragraph: apply inline markup
    let indent_len = line.len() - trimmed.len();
    let mut spans = Vec::new();
    if indent_len > 0 {
        spans.push(Span::raw(line[..indent_len].to_string()));
    }
    spans.extend(md_inline_spans(trimmed, md_body()));
    spans
}

fn tokenize_md_table_row(line: &str, trimmed: &str) -> Vec<Span<'static>> {
    let sep_style = Style::default().fg(Color::Rgb(0x62, 0x72, 0xa4));
    // Separator row (e.g. |---|---|  or |:---|:---:|)
    let inner = trimmed.trim_start_matches('|').trim_end_matches('|');
    if !inner.is_empty() && inner.replace(['-', ':', ' ', '|'], "").is_empty() {
        return vec![Span::styled(line.to_string(), md_rule())];
    }
    let indent_len = line.len() - trimmed.len();
    let mut spans: Vec<Span<'static>> = Vec::new();
    if indent_len > 0 {
        spans.push(Span::raw(line[..indent_len].to_string()));
    }
    for (idx, part) in trimmed.split('|').enumerate() {
        if idx == 0 && part.is_empty() {
            // Leading pipe
            spans.push(Span::styled("|".to_string(), sep_style));
            continue;
        }
        if part.is_empty() {
            // Trailing pipe or consecutive pipes
            spans.push(Span::styled("|".to_string(), sep_style));
            continue;
        }
        spans.extend(md_inline_spans(part, md_body()));
        spans.push(Span::styled("|".to_string(), sep_style));
    }
    // Remove the last spurious trailing | we added
    if spans
        .last()
        .map(|s| s.content.as_ref() == "|")
        .unwrap_or(false)
    {
        spans.pop();
    }
    spans
}

/// Parse inline Markdown markup within a single text segment.
fn md_inline_spans(text: &str, default_style: Style) -> Vec<Span<'static>> {
    if text.is_empty() {
        return vec![];
    }
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut buf = String::new();
    let mut i = 0;

    macro_rules! flush {
        () => {
            if !buf.is_empty() {
                spans.push(Span::styled(buf.clone(), default_style));
                buf.clear();
            }
        };
    }

    while i < len {
        // Inline code  `...`
        if chars[i] == '`' {
            flush!();
            let mut s = String::from('`');
            i += 1;
            while i < len && chars[i] != '`' {
                s.push(chars[i]);
                i += 1;
            }
            if i < len {
                s.push('`');
                i += 1;
            }
            // Show without backticks for cleaner display
            let inner: String = s
                .chars()
                .skip(1)
                .take(s.chars().count().saturating_sub(2))
                .collect();
            spans.push(Span::styled(inner, md_code_inline()));
            continue;
        }

        // Bold  **text** or __text__
        if i + 1 < len
            && ((chars[i] == '*' && chars[i + 1] == '*')
                || (chars[i] == '_' && chars[i + 1] == '_'))
        {
            let delim = chars[i];
            flush!();
            i += 2;
            let mut s = String::new();
            while i + 1 < len && !(chars[i] == delim && chars[i + 1] == delim) {
                s.push(chars[i]);
                i += 1;
            }
            if i + 1 < len {
                i += 2;
            } // consume closing **
            spans.push(Span::styled(s, md_bold()));
            continue;
        }

        // Italic  *text* or _text_  (only if not start-of-word underscore in code)
        if (chars[i] == '*' || chars[i] == '_') && i + 1 < len && chars[i + 1] != ' ' {
            let delim = chars[i];
            flush!();
            i += 1;
            let mut s = String::new();
            while i < len && chars[i] != delim {
                s.push(chars[i]);
                i += 1;
            }
            if i < len {
                i += 1;
            }
            spans.push(Span::styled(s, md_italic()));
            continue;
        }

        // Link  [text](url)
        if chars[i] == '[' {
            let bracket_start = i;
            i += 1;
            let mut link_text = String::new();
            while i < len && chars[i] != ']' {
                link_text.push(chars[i]);
                i += 1;
            }
            if i < len && i + 1 < len && chars[i] == ']' && chars[i + 1] == '(' {
                i += 2; // skip ](
                let mut url = String::new();
                while i < len && chars[i] != ')' {
                    url.push(chars[i]);
                    i += 1;
                }
                if i < len {
                    i += 1;
                }
                flush!();
                spans.push(Span::styled(link_text, md_link_text()));
                // Show URL in dim parens
                spans.push(Span::styled(format!(" ({url})"), md_link_url()));
                continue;
            } else {
                // Not a valid link — rewind and emit as plain text
                let literal: String = chars[bracket_start..i].iter().collect();
                buf.push_str(&literal);
                continue;
            }
        }

        buf.push(chars[i]);
        i += 1;
    }

    flush!();
    spans
}
