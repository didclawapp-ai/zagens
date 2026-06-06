//! Regex-based symbol extractors for TypeScript/JavaScript, Python, Go, C/C++,
//! and Vue/Svelte single-file components.

use super::SymbolEntry;
use std::io::{BufRead, BufReader};
use std::path::Path;

const TS_PATTERNS: &[(&str, &str)] = &[
    (r"^export\s+default\s+(?:async\s+)?function\s+(\w+)", "fn"),
    (r"^export\s+(?:async\s+)?function\s+(\w+)", "fn"),
    (r"^(?:async\s+)?function\s+(\w+)", "fn"),
    (
        r"^export\s+const\s+(\w+)\s*=\s*(?:async\s+)?(?:\(|[A-Za-z_]\w*\s*=>)",
        "fn",
    ),
    (r"^export\s+(?:default\s+)?interface\s+(\w+)", "interface"),
    (r"^interface\s+(\w+)", "interface"),
    (r"^export\s+type\s+(\w+)\s*[=<]", "type"),
    (r"^(?:export\s+(?:default\s+)?)?class\s+(\w+)", "class"),
    (r"^(?:export\s+)?(?:const\s+)?enum\s+(\w+)", "enum"),
    (
        r"^\s{2,}(?:(?:public|private|protected|static|async|readonly|override)\s+)*(\w+)\s*[<(]",
        "method",
    ),
];

const TS_SKIP_NAMES: &[&str] = &[
    "if",
    "for",
    "while",
    "switch",
    "catch",
    "return",
    "new",
    "typeof",
    "instanceof",
    "in",
    "of",
    "from",
    "import",
    "export",
    "constructor",
    "super",
    "extends",
    "implements",
];

/// Extract symbols from TypeScript, TSX, or JavaScript source files.
pub(crate) fn extract_ts_symbols(path: &Path, source_mtime: u64) -> Option<Vec<SymbolEntry>> {
    let content = std::fs::read_to_string(path).ok()?;
    extract_ts_from_source(&content, source_mtime, 0)
}

/// Extract symbols from a Vue or Svelte single-file component `<script>` block.
pub(crate) fn extract_sfc_symbols(path: &Path, source_mtime: u64) -> Option<Vec<SymbolEntry>> {
    let content = std::fs::read_to_string(path).ok()?;
    let (script, line_offset) = extract_script_block(&content)?;
    extract_ts_from_source(script, source_mtime, line_offset)
}

/// Extract symbols from C/C++ source or header files.
pub(crate) fn extract_cpp_symbols(path: &Path, source_mtime: u64) -> Option<Vec<SymbolEntry>> {
    let file = std::fs::File::open(path).ok()?;
    let reader = BufReader::new(file);
    let mut symbols: Vec<SymbolEntry> = Vec::new();

    let re_class = regex::Regex::new(r"^(?:\s*(?:template\s*<[^>]*>\s*)?)?class\s+(\w+)").ok()?;
    let re_struct = regex::Regex::new(r"^struct\s+(\w+)").ok()?;
    let re_enum = regex::Regex::new(r"^enum\s+(?:class\s+)?(\w+)").ok()?;
    let re_namespace = regex::Regex::new(r"^namespace\s+(\w+)").ok()?;
    let re_fn_def = regex::Regex::new(
        r"^(?:[\w:<>,\*&\s~]+?\s+)+(\w+)\s*\([^;]*\)\s*(?:const\s*)?(?:noexcept\s*)?(?:override\s*)?\{",
    )
    .ok()?;
    let re_fn_decl = regex::Regex::new(
        r"^(?:[\w:<>,\*&\s~]+?\s+)+(\w+)\s*\([^;]*\)\s*(?:const\s*)?(?:noexcept\s*)?(?:override\s*)?;",
    )
    .ok()?;

    const SKIP: &[&str] = &["if", "for", "while", "switch", "return", "new", "delete"];

    for (line_idx, line_result) in reader.lines().enumerate() {
        let line = line_result.ok()?;
        let line_num = line_idx + 1;
        let trimmed = line.trim();

        if trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with('*') {
            continue;
        }

        let mut matched = false;
        for (re, kind) in [
            (&re_class, "class"),
            (&re_struct, "struct"),
            (&re_enum, "enum"),
            (&re_namespace, "namespace"),
        ] {
            if let Some(cap) = re.captures(trimmed) {
                if let Some(name) = cap.get(1) {
                    let name = name.as_str();
                    if !SKIP.contains(&name) {
                        symbols.push(SymbolEntry {
                            kind: kind.to_string(),
                            name: name.to_string(),
                            line: line_num,
                            source_mtime,
                            calls: vec![],
                        });
                        matched = true;
                        break;
                    }
                }
            }
        }
        if matched {
            continue;
        }

        if let Some(cap) = re_fn_def.captures(trimmed) {
            if let Some(name) = cap.get(1) {
                let name = name.as_str();
                if !SKIP.contains(&name) {
                    symbols.push(SymbolEntry {
                        kind: "fn".into(),
                        name: name.to_string(),
                        line: line_num,
                        source_mtime,
                        calls: vec![],
                    });
                }
            }
            continue;
        }

        if let Some(cap) = re_fn_decl.captures(trimmed) {
            if let Some(name) = cap.get(1) {
                let name = name.as_str();
                if !SKIP.contains(&name) {
                    symbols.push(SymbolEntry {
                        kind: "fn".into(),
                        name: name.to_string(),
                        line: line_num,
                        source_mtime,
                        calls: vec![],
                    });
                }
            }
        }
    }

    symbols.sort_by_key(|s| s.line);
    symbols.dedup_by(|a, b| a.kind == b.kind && a.name == b.name);
    Some(symbols)
}

/// Extract symbols from Python source files.
pub(crate) fn extract_py_symbols(path: &Path, source_mtime: u64) -> Option<Vec<SymbolEntry>> {
    let file = std::fs::File::open(path).ok()?;
    let reader = BufReader::new(file);
    let mut symbols: Vec<SymbolEntry> = Vec::new();

    let re_class = regex::Regex::new(r"^class\s+(\w+)").ok()?;
    let re_async_fn = regex::Regex::new(r"^async\s+def\s+(\w+)").ok()?;
    let re_fn = regex::Regex::new(r"^def\s+(\w+)").ok()?;
    let re_method = regex::Regex::new(r"^\s+def\s+(\w+)").ok()?;

    const SKIP_NAMES: &[&str] = &["if", "for", "while", "with", "class", "def", "return"];

    let mut current_class: Option<String> = None;

    for (line_idx, line_result) in reader.lines().enumerate() {
        let line = line_result.ok()?;
        let line_num = line_idx + 1;
        let trimmed = line.trim();

        if trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }

        if let Some(cap) = re_class.captures(trimmed) {
            if let Some(name) = cap.get(1) {
                let name = name.as_str().to_string();
                current_class = Some(name.clone());
                symbols.push(SymbolEntry {
                    kind: "class".into(),
                    name,
                    line: line_num,
                    source_mtime,
                    calls: vec![],
                });
            }
            continue;
        }

        if !line.starts_with(' ') && !line.starts_with('\t') {
            current_class = None;
            if let Some(cap) = re_async_fn.captures(trimmed) {
                if let Some(name) = cap.get(1) {
                    let name = name.as_str();
                    if !SKIP_NAMES.contains(&name) {
                        symbols.push(SymbolEntry {
                            kind: "fn".into(),
                            name: name.to_string(),
                            line: line_num,
                            source_mtime,
                            calls: vec![],
                        });
                    }
                }
                continue;
            }
            if let Some(cap) = re_fn.captures(trimmed) {
                if let Some(name) = cap.get(1) {
                    let name = name.as_str();
                    if !SKIP_NAMES.contains(&name) {
                        symbols.push(SymbolEntry {
                            kind: "fn".into(),
                            name: name.to_string(),
                            line: line_num,
                            source_mtime,
                            calls: vec![],
                        });
                    }
                }
                continue;
            }
        } else if let Some(cap) = re_method.captures(&line) {
            if let Some(name) = cap.get(1) {
                let name = name.as_str();
                if name == "self" || SKIP_NAMES.contains(&name) {
                    continue;
                }
                if let Some(cls) = &current_class {
                    symbols.push(SymbolEntry {
                        kind: "method".into(),
                        name: format!("{}::{}", cls, name),
                        line: line_num,
                        source_mtime,
                        calls: vec![],
                    });
                }
            }
        }
    }

    symbols.sort_by_key(|s| s.line);
    symbols.dedup_by(|a, b| a.kind == b.kind && a.name == b.name);
    Some(symbols)
}

/// Extract symbols from Go source files.
pub(crate) fn extract_go_symbols(path: &Path, source_mtime: u64) -> Option<Vec<SymbolEntry>> {
    let file = std::fs::File::open(path).ok()?;
    let reader = BufReader::new(file);
    let mut symbols: Vec<SymbolEntry> = Vec::new();

    let re_method = regex::Regex::new(r"^func\s+\(\s*\w+\s+\*?(\w+)\s*\)\s+(\w+)").ok()?;
    let re_fn = regex::Regex::new(r"^func\s+(\w+)").ok()?;
    let re_struct = regex::Regex::new(r"^type\s+(\w+)\s+struct\b").ok()?;
    let re_iface = regex::Regex::new(r"^type\s+(\w+)\s+interface\b").ok()?;
    let re_type = regex::Regex::new(r"^type\s+(\w+)\s+").ok()?;

    for (line_idx, line_result) in reader.lines().enumerate() {
        let line = line_result.ok()?;
        let line_num = line_idx + 1;
        let trimmed = line.trim();

        if trimmed.starts_with("//") || trimmed.is_empty() {
            continue;
        }

        if let Some(cap) = re_method.captures(trimmed) {
            let recv = cap.get(1)?.as_str();
            let name = cap.get(2)?.as_str();
            symbols.push(SymbolEntry {
                kind: "method".into(),
                name: format!("{}::{}", recv, name),
                line: line_num,
                source_mtime,
                calls: vec![],
            });
            continue;
        }

        if let Some(cap) = re_struct.captures(trimmed) {
            let name = cap.get(1)?.as_str().to_string();
            symbols.push(SymbolEntry {
                kind: "struct".into(),
                name,
                line: line_num,
                source_mtime,
                calls: vec![],
            });
            continue;
        }

        if let Some(cap) = re_iface.captures(trimmed) {
            let name = cap.get(1)?.as_str().to_string();
            symbols.push(SymbolEntry {
                kind: "interface".into(),
                name,
                line: line_num,
                source_mtime,
                calls: vec![],
            });
            continue;
        }

        if let Some(cap) = re_fn.captures(trimmed) {
            let name = cap.get(1)?.as_str().to_string();
            symbols.push(SymbolEntry {
                kind: "fn".into(),
                name,
                line: line_num,
                source_mtime,
                calls: vec![],
            });
            continue;
        }

        if let Some(cap) = re_type.captures(trimmed) {
            let name = cap.get(1)?.as_str().to_string();
            symbols.push(SymbolEntry {
                kind: "type".into(),
                name,
                line: line_num,
                source_mtime,
                calls: vec![],
            });
        }
    }

    symbols.sort_by_key(|s| s.line);
    symbols.dedup_by(|a, b| a.kind == b.kind && a.name == b.name);
    Some(symbols)
}

fn extract_script_block(content: &str) -> Option<(&str, usize)> {
    let lower = content.to_lowercase();
    let script_start = lower.find("<script")?;
    let after_tag = content[script_start..].find('>')? + script_start + 1;
    let close_rel = content[after_tag..].to_lowercase().find("</script>")?;
    let raw = &content[after_tag..after_tag + close_rel];
    let script = raw.trim();
    if script.is_empty() {
        return None;
    }
    let leading = raw.len() - raw.trim_start().len();
    let script_start = after_tag + leading;
    let line_offset = content[..script_start].matches('\n').count();
    Some((script, line_offset))
}

pub(crate) fn extract_ts_from_source(
    content: &str,
    source_mtime: u64,
    line_offset: usize,
) -> Option<Vec<SymbolEntry>> {
    extract_ts_style_lines(
        content.lines(),
        source_mtime,
        line_offset,
        TS_PATTERNS,
        TS_SKIP_NAMES,
    )
}

fn extract_ts_style_lines<I, S>(
    lines: I,
    source_mtime: u64,
    line_offset: usize,
    patterns: &[(&str, &str)],
    skip_names: &[&str],
) -> Option<Vec<SymbolEntry>>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut symbols: Vec<SymbolEntry> = Vec::new();

    let compiled: Vec<(regex::Regex, &str)> = patterns
        .iter()
        .filter_map(|(pat, kind)| regex::Regex::new(pat).ok().map(|r| (r, *kind)))
        .collect();

    let mut current_class: Option<String> = None;
    let mut brace_depth: i32 = 0;
    let mut class_brace_start: i32 = -1;

    for (line_idx, line) in lines.into_iter().enumerate() {
        let line = line.as_ref();
        let line_num = line_idx + 1 + line_offset;

        for ch in line.chars() {
            match ch {
                '{' => brace_depth += 1,
                '}' => {
                    brace_depth -= 1;
                    if current_class.is_some() && brace_depth <= class_brace_start {
                        current_class = None;
                        class_brace_start = -1;
                    }
                }
                _ => {}
            }
        }

        let trimmed = line.trim();
        if trimmed.starts_with("//") || trimmed.starts_with('*') || trimmed.starts_with("/*") {
            continue;
        }

        for (re, kind) in &compiled {
            if let Some(cap) = re.captures(line) {
                if let Some(name_match) = cap.get(1) {
                    let name = name_match.as_str().to_string();

                    if skip_names.contains(&name.as_str()) {
                        continue;
                    }

                    let full_name = if *kind == "method" {
                        match &current_class {
                            Some(cls) => format!("{}::{}", cls, name),
                            None => continue,
                        }
                    } else {
                        if *kind == "class" {
                            current_class = Some(name.clone());
                            class_brace_start = brace_depth;
                        }
                        name
                    };

                    symbols.push(SymbolEntry {
                        kind: kind.to_string(),
                        name: full_name,
                        line: line_num,
                        source_mtime,
                        calls: vec![],
                    });
                    break;
                }
            }
        }
    }

    symbols.sort_by_key(|s| s.line);
    symbols.dedup_by(|a, b| a.kind == b.kind && a.name == b.name);
    Some(symbols)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_py_symbols_finds_def_and_class() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("mod.py");
        std::fs::write(
            &path,
            "class Service:\n    def run(self):\n        pass\n\ndef main():\n    pass\n",
        )
        .unwrap();

        let syms = extract_py_symbols(&path, 0).expect("parse");
        assert!(
            syms.iter()
                .any(|s| s.kind == "class" && s.name == "Service")
        );
        assert!(
            syms.iter()
                .any(|s| s.kind == "method" && s.name == "Service::run")
        );
        assert!(syms.iter().any(|s| s.kind == "fn" && s.name == "main"));
    }

    #[test]
    fn extract_go_symbols_finds_func_and_struct() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("main.go");
        std::fs::write(
            &path,
            "package main\n\nfunc Hello() {}\n\ntype Config struct {}\n\nfunc (c *Config) Load() {}\n",
        )
        .unwrap();

        let syms = extract_go_symbols(&path, 0).expect("parse");
        assert!(syms.iter().any(|s| s.kind == "fn" && s.name == "Hello"));
        assert!(
            syms.iter()
                .any(|s| s.kind == "struct" && s.name == "Config")
        );
        assert!(
            syms.iter()
                .any(|s| s.kind == "method" && s.name == "Config::Load")
        );
    }

    #[test]
    fn extract_ts_symbols_parses_js_export() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("util.js");
        std::fs::write(&path, "export function normalizePath(p) {}\n").unwrap();

        let syms = extract_ts_symbols(&path, 0).expect("parse");
        assert!(
            syms.iter()
                .any(|s| s.kind == "fn" && s.name == "normalizePath")
        );
    }

    #[test]
    fn extract_cpp_symbols_finds_class_and_fn() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("widget.cpp");
        std::fs::write(&path, "class Widget {\n};\n\nvoid reset() {\n}\n").unwrap();

        let syms = extract_cpp_symbols(&path, 0).expect("parse");
        assert!(syms.iter().any(|s| s.kind == "class" && s.name == "Widget"));
        assert!(syms.iter().any(|s| s.kind == "fn" && s.name == "reset"));
    }

    #[test]
    fn extract_sfc_symbols_maps_script_line_numbers() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("App.vue");
        std::fs::write(
            &path,
            "<template><div /></template>\n<script setup>\nexport function boot() {}\n</script>\n",
        )
        .unwrap();

        let syms = extract_sfc_symbols(&path, 0).expect("parse");
        let boot = syms.iter().find(|s| s.name == "boot").expect("boot fn");
        assert_eq!(boot.line, 3);
    }
}
