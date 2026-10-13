use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

const LEAVES: &[&str] =
    &["error", "log", "panics", "process", "version", "paths", "time", "persist", "text", "list", "http"];

const APP_EXCEPTIONS: &[&str] = &["src/term.rs", "src/web/mod.rs"];
const UI_EXCEPTIONS: &[&str] = &[
    "src/config.rs",
    "src/issues/browser.rs",
    "src/project.rs",
    "src/settings.rs",
    "src/state.rs",
    "src/usage/mod.rs",
];
const LONG_FILES: &[&str] = &["src/app/control.rs", "src/issues/browser.rs", "src/settings.rs"];

struct Source {
    path: PathBuf,
    module: Vec<String>,
    text: String,
}

impl Source {
    fn new(path: PathBuf, text: String) -> Self {
        let module = module_path(&path);
        Self { path, module, text }
    }

    fn production_uses(&self) -> BTreeSet<String> {
        if !self.path.starts_with("src") {
            return BTreeSet::new();
        }
        used_modules(&self.module, &self.text)
    }
}

fn module_path(path: &Path) -> Vec<String> {
    let mut parts: Vec<_> = path
        .strip_prefix("src")
        .or_else(|_| path.strip_prefix("tests"))
        .expect("source path is under src or tests")
        .with_extension("")
        .iter()
        .map(|part| part.to_str().expect("source path is UTF-8").to_owned())
        .collect();
    if parts.last().is_some_and(|part| matches!(part.as_str(), "mod" | "lib" | "main")) {
        parts.pop();
    }
    parts
}

fn sources() -> Vec<Source> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut paths = Vec::new();
    for folder in ["src", "tests"] {
        collect_paths(&root.join(folder), &mut paths);
    }
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let text = fs::read_to_string(&path).expect("read Rust source");
            let path = path.strip_prefix(root).expect("source is under crate root").to_owned();
            Source::new(path, text)
        })
        .collect()
}

fn collect_paths(folder: &Path, paths: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(folder).expect("read source directory") {
        let entry = entry.expect("read source entry");
        let kind = entry.file_type().expect("read source entry type");
        if kind.is_dir() {
            collect_paths(&entry.path(), paths);
        } else if kind.is_file() && entry.path().extension().is_some_and(|extension| extension == "rs") {
            paths.push(entry.path());
        }
    }
}

fn raw_string_end(source: &str, start: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut cursor = start;
    if matches!(bytes[cursor], b'b' | b'c') {
        cursor += 1;
    }
    if bytes.get(cursor) != Some(&b'r') {
        return None;
    }
    cursor += 1;
    let hashes = cursor;
    while bytes.get(cursor) == Some(&b'#') {
        cursor += 1;
    }
    if bytes.get(cursor) != Some(&b'"') {
        return None;
    }
    let ending = format!("\"{}", "#".repeat(cursor - hashes));
    let body = cursor + 1;
    Some(source[body..].find(&ending).map_or(bytes.len(), |offset| body + offset + ending.len()))
}

fn char_literal_end(source: &str, start: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut cursor = start + 1;
    if bytes.get(cursor) == Some(&b'\\') {
        cursor += 1;
        match bytes.get(cursor)? {
            b'x' => cursor += 3,
            b'u' if bytes.get(cursor + 1) == Some(&b'{') => {
                cursor += source[cursor..].find('}')? + 1;
            }
            _ => cursor += 1,
        }
    } else {
        cursor += source.get(cursor..)?.chars().next()?.len_utf8();
    }
    (bytes.get(cursor) == Some(&b'\'')).then_some(cursor + 1)
}

fn code_tokens(source: &str) -> Vec<&str> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut cursor = 0;
    while cursor < bytes.len() {
        let start = cursor;
        if bytes[cursor..].starts_with(b"//") {
            cursor += source[cursor..].find('\n').unwrap_or(bytes.len() - cursor);
        } else if bytes[cursor..].starts_with(b"/*") {
            cursor += 2;
            let mut depth = 1;
            while cursor < bytes.len() && depth > 0 {
                if bytes[cursor..].starts_with(b"/*") {
                    depth += 1;
                    cursor += 2;
                } else if bytes[cursor..].starts_with(b"*/") {
                    depth -= 1;
                    cursor += 2;
                } else {
                    cursor += 1;
                }
            }
        } else if let Some(end) = raw_string_end(source, cursor) {
            cursor = end;
        } else if bytes[cursor] == b'"' {
            cursor += 1;
            while cursor < bytes.len() {
                let byte = bytes[cursor];
                cursor += 1;
                if byte == b'\\' {
                    cursor = (cursor + 1).min(bytes.len());
                } else if byte == b'"' {
                    break;
                }
            }
        } else if bytes[cursor] == b'\''
            && let Some(end) = char_literal_end(source, cursor)
        {
            cursor = end;
        } else if bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        } else if bytes[cursor..].starts_with(b"::") {
            cursor += 2;
            tokens.push(&source[start..cursor]);
        } else {
            let raw_identifier = bytes[cursor..].starts_with(b"r#");
            cursor += usize::from(raw_identifier) * 2;
            let name = cursor;
            let first = source[cursor..].chars().next().expect("character at cursor");
            cursor += first.len_utf8();
            if first.is_alphanumeric() || first == '_' {
                while let Some(next) = source[cursor..].chars().next() {
                    if !next.is_alphanumeric() && next != '_' {
                        break;
                    }
                    cursor += next.len_utf8();
                }
            }
            tokens.push(&source[name..cursor]);
        }
    }
    tokens
}

fn block_end(tokens: &[&str], start: usize) -> usize {
    let mut depth = 0;
    for (index, token) in tokens.iter().enumerate().skip(start) {
        match *token {
            "{" => depth += 1,
            "}" => {
                depth -= 1;
                if depth == 0 {
                    return index + 1;
                }
            }
            _ => {}
        }
    }
    tokens.len()
}

fn item_end(tokens: &[&str], start: usize) -> usize {
    let mut depth = 0;
    for (index, token) in tokens.iter().enumerate().skip(start) {
        match *token {
            "(" | "[" => depth += 1,
            ")" | "]" => depth -= 1,
            ";" if depth == 0 => return index + 1,
            "{" if depth == 0 => return block_end(tokens, index),
            _ => {}
        }
    }
    tokens.len()
}

fn resolve_path(tokens: &[&str], mut cursor: usize, mut path: Vec<String>, used: &mut BTreeSet<String>) -> usize {
    if tokens.get(cursor) == Some(&"{") {
        cursor += 1;
        while cursor < tokens.len() && tokens[cursor] != "}" {
            cursor = resolve_path(tokens, cursor, path.clone(), used);
            while cursor < tokens.len() && !matches!(tokens[cursor], "," | "}") {
                cursor += 1;
            }
            if tokens.get(cursor) == Some(&",") {
                cursor += 1;
            }
        }
        return cursor + 1;
    }
    match tokens.get(cursor).copied() {
        Some("crate") => path.clear(),
        Some("super") => {
            path.pop();
        }
        Some("self" | "*") => {}
        Some(name) => path.push(name.to_owned()),
        None => return cursor,
    }
    cursor += 1;
    if tokens.get(cursor) == Some(&"::") {
        return resolve_path(tokens, cursor + 1, path, used);
    }
    if let Some(module) = path.first() {
        used.insert(module.clone());
    }
    cursor
}

fn scan_scope(tokens: &[&str], module: &[String], used: &mut BTreeSet<String>) {
    if module.iter().any(|part| matches!(part.as_str(), "tests" | "test_util")) {
        return;
    }
    let mut cursor = 0;
    while cursor < tokens.len() {
        if tokens[cursor..].starts_with(&["#", "[", "cfg", "(", "test", ")", "]"]) {
            cursor = item_end(tokens, cursor + 7);
        } else if tokens[cursor] == "mod" && tokens.get(cursor + 2) == Some(&"{") {
            let mut child = module.to_vec();
            child.push(tokens[cursor + 1].to_owned());
            let end = block_end(tokens, cursor + 2);
            scan_scope(&tokens[cursor + 3..end - 1], &child, used);
            cursor = end;
        } else if matches!(tokens[cursor], "crate" | "super" | "self") && tokens.get(cursor + 1) == Some(&"::") {
            cursor = resolve_path(tokens, cursor, module.to_vec(), used);
        } else {
            cursor += 1;
        }
    }
}

fn used_modules(module: &[String], source: &str) -> BTreeSet<String> {
    let mut used = BTreeSet::new();
    scan_scope(&code_tokens(source), module, &mut used);
    used
}

fn covers_import(exception: &Path, path: &Path) -> bool {
    path == exception
        || (exception.file_name().is_some_and(|name| name != "mod.rs")
            && path.starts_with(exception.with_extension("")))
}

fn import_failures(sources: &[Source], target: &str, allowed: &[&str], exceptions: &[&str]) -> Vec<String> {
    let users: Vec<_> = sources.iter().filter(|source| source.production_uses().contains(target)).collect();
    let mut failures = Vec::new();
    for source in &users {
        if !source.module.first().is_some_and(|module| allowed.contains(&module.as_str()))
            && !exceptions.iter().any(|exception| covers_import(Path::new(exception), &source.path))
        {
            failures.push(format!(
                "{} (module {}) uses crate::{target}. Only {} may: move what it needs out of {target}, or move the code that needs it there (see Code layout in DESIGN.md). Do not add it to the exceptions: that list only shrinks.",
                source.path.display(), source.module.join("::"), allowed.split_last().map_or_else(String::new, |(last, rest)| format!("{} and {last}", rest.join(", ")))
            ));
        }
    }
    for exception in exceptions {
        if !users.iter().any(|source| covers_import(Path::new(exception), &source.path)) {
            failures.push(format!(
                "{exception} no longer uses crate::{target}, or is gone: remove it from the exceptions in tests/layers.rs, or rename it if the file moved."
            ));
        }
    }
    failures
}

fn leaf_failures(sources: &[Source]) -> Vec<String> {
    let mut failures = Vec::new();
    for source in sources {
        let Some(module) = source.module.first() else { continue };
        if !LEAVES.contains(&module.as_str()) || source.module.iter().any(|part| part == "tests") {
            continue;
        }
        for used in source.production_uses() {
            if used != *module && !LEAVES.contains(&used.as_str()) {
                failures.push(format!(
                    "{} (module {}) uses crate::{used}. Shared leaves may use only other leaves (see Code layout in DESIGN.md). Move the dependency out of the leaf.",
                    source.path.display(), source.module.join("::")
                ));
            }
        }
    }
    failures
}

fn size_failures(sources: &[Source], exceptions: &[&str]) -> Vec<String> {
    let mut failures = Vec::new();
    for source in sources {
        let lines = source.text.lines().count();
        if lines > 1000 && !exceptions.iter().any(|exception| source.path == Path::new(exception)) {
            failures.push(format!(
                "{} has {lines} lines, over 1000. Split it by concern into child modules, and move its tests to {}/tests.rs (see Code layout in DESIGN.md). Do not add it to LONG_FILES: that list only shrinks.",
                source.path.display(), source.module.join("/")
            ));
        }
    }
    for exception in exceptions {
        match sources.iter().find(|source| source.path == Path::new(exception)) {
            Some(source) if source.text.lines().count() <= 1000 => failures.push(format!(
                "{exception} has {} lines now: remove it from LONG_FILES in tests/layers.rs.",
                source.text.lines().count()
            )),
            None => failures.push(format!(
                "{exception} is gone: remove it from LONG_FILES in tests/layers.rs, or rename it if the file moved."
            )),
            Some(_) => {}
        }
    }
    failures
}

fn assert_clean(failures: &[String]) {
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn production_modules_follow_the_dependency_rules() {
    let sources = sources();
    assert_clean(&import_failures(&sources, "app", &["app", "server"], APP_EXCEPTIONS));
    assert_clean(&leaf_failures(&sources));
}

#[test]
fn only_ui_app_server_and_client_use_ui() {
    assert_clean(&import_failures(&sources(), "ui", &["ui", "app", "server", "client"], UI_EXCEPTIONS));
}

#[test]
fn no_file_is_over_1000_lines() {
    assert_clean(&size_failures(&sources(), LONG_FILES));
}

#[test]
fn the_scanner_resolves_paths_and_skips_strings_and_tests() {
    let cases: &[(&str, &str, &[&str])] = &[
        ("term", "use crate::{app::AppEvent, control};", &["app", "control"]),
        ("stats", "crate::ui::Sidebar::default()", &["ui"]),
        ("issues/browser", "use super::super::ui;", &["ui"]),
        ("stats", "let message = \"crate::ui::draw\";", &[]),
        ("stats", "#[cfg(test)] mod tests { use crate::ui; }", &[]),
        ("process", "mod imp { use super::{ui, io}; }", &["process"]),
        ("lib", "use self::{ui, app::{self, AppEvent}};", &["app", "ui"]),
        ("stats", "$crate::ui::draw();", &["ui"]),
        ("stats", "pub(super) fn f<'a>(x: &'a str) { crate::ui::draw(); }", &["ui"]),
        ("stats", "#[cfg(any(target_os = \"macos\", test))] use crate::ui;", &["ui"]),
        ("stats", "#[cfg(test)] use crate::ui; use crate::control;", &["control"]),
        (
            "stats",
            "#[cfg(test)] #[allow(dead_code)] fn f() { crate::ui::draw(); } crate::control::read();",
            &["control"],
        ),
        ("stats", "mod nested { mod deeper { use super::super::super::ui; } }", &["ui"]),
        ("stats", "mod nested { use self::ui; } use self::ui;", &["stats"]),
        ("stats", "use crate::r#ui;", &["ui"]),
        ("stats/tests", "use crate::ui;", &[]),
        ("test_util", "use crate::app;", &[]),
        ("stats", "// crate::ui::draw\n/* crate::app /* nested */ ::App */", &[]),
        ("stats", r####"let x = r###"crate::ui::draw \" # "###; let y = br#"crate::app"#;"####, &[]),
        ("stats", r#"let x = b"crate::ui"; let y = "escaped \" crate::app";"#, &[]),
        ("stats", r#"let x = '\''; let y = '"'; let z = '\u{1f600}'; let b = b'\x22'; crate::ui::draw();"#, &["ui"]),
    ];
    for (module, source, expected) in cases {
        let path = PathBuf::from(format!("src/{module}.rs"));
        let source = Source::new(path, (*source).to_owned());
        let expected: BTreeSet<_> = expected.iter().map(|name| (*name).to_owned()).collect();
        assert_eq!(source.production_uses(), expected, "module {module}: {}", source.text);
    }
    assert_eq!(module_path(Path::new("src/usage/mod.rs")), ["usage"]);
    assert_eq!(module_path(Path::new("src/lib.rs")), Vec::<String>::new());
    assert_eq!(module_path(Path::new("src/main.rs")), Vec::<String>::new());
    for (exception, path, expected) in [
        ("src/issues/browser.rs", "src/issues/browser/rows.rs", true),
        ("src/issues/browser.rs", "src/issues/browser_extra.rs", false),
        ("src/issues/mod.rs", "src/issues/github.rs", false),
        ("src/issues/mod.rs", "src/issues/mod.rs", true),
    ] {
        assert_eq!(covers_import(Path::new(exception), Path::new(path)), expected);
    }
    for (module, text, rejected) in [
        ("log", "$crate::log::write(); crate::time::Stamp();", false),
        ("error", "use crate::update;", true),
        ("persist", "use crate::state::State;", true),
        ("time/tests", "use crate::issues;", false),
        ("time", "#[cfg(test)] mod tests { use crate::issues; }", false),
        ("process", "#[cfg(target_os = \"macos\")] mod imp { use crate::ui; }", true),
    ] {
        let source = Source::new(PathBuf::from(format!("src/{module}.rs")), text.to_owned());
        assert_eq!(!leaf_failures(&[source]).is_empty(), rejected, "module {module}: {text}");
    }
    let child = Source::new(PathBuf::from("src/app/control.rs"), "\n".repeat(1001));
    assert!(size_failures(&[child], &["src/app.rs"])[0].contains("over 1000"));
}
