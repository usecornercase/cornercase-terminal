use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

const LEAVES: &[&str] =
    &["error", "gh", "http", "list", "log", "panics", "paths", "persist", "process", "text", "time", "version"];

const APP_EXCEPTIONS: &[&str] = &[];
const UI_EXCEPTIONS: &[&str] = &[];
const LONG_FILES: &[&str] = &[];
const INDEX_EXCEPTIONS: &[&str] = &[];

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
    let mut tokens = Vec::new();
    let mut cursor = 0;
    while cursor < source.len() {
        if let Some(end) = skipped_end(source, cursor) {
            cursor = end;
            continue;
        }
        let (token, end) = next_token(source, cursor);
        tokens.push(token);
        cursor = end;
    }
    tokens
}

fn skipped_end(source: &str, cursor: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let rest = &bytes[cursor..];
    if rest.starts_with(b"//") {
        return Some(cursor + source[cursor..].find('\n').unwrap_or(bytes.len() - cursor));
    }
    if rest.starts_with(b"/*") {
        return Some(block_comment_end(bytes, cursor));
    }
    if let Some(end) = raw_string_end(source, cursor) {
        return Some(end);
    }
    if rest[0] == b'"' {
        return Some(string_end(bytes, cursor));
    }
    if rest[0] == b'\''
        && let Some(end) = char_literal_end(source, cursor)
    {
        return Some(end);
    }
    rest[0].is_ascii_whitespace().then_some(cursor + 1)
}

fn block_comment_end(bytes: &[u8], start: usize) -> usize {
    let mut cursor = start + 2;
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
    cursor
}

fn string_end(bytes: &[u8], start: usize) -> usize {
    let mut cursor = start + 1;
    while cursor < bytes.len() {
        let byte = bytes[cursor];
        cursor += 1;
        if byte == b'\\' {
            cursor = (cursor + 1).min(bytes.len());
        } else if byte == b'"' {
            break;
        }
    }
    cursor
}

fn next_token(source: &str, start: usize) -> (&str, usize) {
    let bytes = source.as_bytes();
    if bytes[start..].starts_with(b"::") {
        return (&source[start..start + 2], start + 2);
    }
    let raw_identifier = bytes[start..].starts_with(b"r#");
    let name = start + usize::from(raw_identifier) * 2;
    let first = source[name..].chars().next().expect("character at cursor");
    let mut cursor = name + first.len_utf8();
    if first.is_alphanumeric() || first == '_' {
        cursor = identifier_end(source, cursor);
    }
    (&source[name..cursor], cursor)
}

fn identifier_end(source: &str, mut cursor: usize) -> usize {
    while let Some(next) = source[cursor..].chars().next() {
        if !next.is_alphanumeric() && next != '_' {
            break;
        }
        cursor += next.len_utf8();
    }
    cursor
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

type Visit<'a> = dyn FnMut(&[&str], usize, &[String]) -> usize + 'a;

fn scan_scope(tokens: &[&str], module: &[String], visit: &mut Visit) {
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
            scan_scope(&tokens[cursor + 3..end - 1], &child, visit);
            cursor = end;
        } else {
            cursor = visit(tokens, cursor, module);
        }
    }
}

fn used_modules(module: &[String], source: &str) -> BTreeSet<String> {
    let mut used = BTreeSet::new();
    scan_scope(&code_tokens(source), module, &mut |tokens, cursor, module| {
        if matches!(tokens[cursor], "crate" | "super" | "self") && tokens.get(cursor + 1) == Some(&"::") {
            resolve_path(tokens, cursor, module.to_vec(), &mut used)
        } else {
            cursor + 1
        }
    });
    used
}

fn nested_indexes(module: &[String], source: &str) -> usize {
    let mut found = 0;
    scan_scope(&code_tokens(source), module, &mut |tokens, cursor, _| {
        if matches!(tokens[cursor..], [".", "workspaces" | "tabs", "[", ..]) {
            found += 1;
        }
        cursor + 1
    });
    found
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

fn index_failures(sources: &[Source], exceptions: &[&str]) -> Vec<String> {
    let in_app = |source: &&Source| source.path.starts_with("src") && source.module.first().is_some_and(|m| m == "app");
    let mut failures = Vec::new();
    let mut indexing = Vec::new();
    for source in sources.iter().filter(in_app) {
        let found = nested_indexes(&source.module, &source.text);
        if found > 0 {
            indexing.push(source.path.as_path());
        }
        if found > 0 && !exceptions.iter().any(|exception| source.path == Path::new(exception)) {
            failures.push(format!(
                "{} indexes `.workspaces[` or `.tabs[` {found} times in production code. Reach workspaces and tabs through the places Hierarchy's lookups return and its workspace, workspace_mut, tab and tab_mut (see Hierarchy in DESIGN.md). Do not add it to INDEX_EXCEPTIONS: that list only shrinks.",
                source.path.display()
            ));
        }
    }
    for exception in exceptions {
        if !indexing.contains(&Path::new(exception)) {
            failures.push(format!(
                "{exception} no longer indexes `.workspaces[` or `.tabs[`, or is gone: remove it from INDEX_EXCEPTIONS in tests/layers.rs."
            ));
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
fn the_leaves_are_the_first_group_of_lib_rs() {
    let lib = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs")).expect("read src/lib.rs");
    let first = lib.split("\n\n").next().unwrap_or_default();
    let names: Vec<&str> =
        first.lines().filter_map(|line| line.strip_suffix(';')?.rsplit_once("mod ").map(|(_, name)| name)).collect();
    assert_eq!(names, LEAVES);
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
fn app_reaches_workspaces_and_tabs_through_places() {
    assert_clean(&index_failures(&sources(), INDEX_EXCEPTIONS));
}

const SCANNER_CASES: &[(&str, &str, &[&str])] = &[
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
    ("stats", "#[cfg(test)] #[allow(dead_code)] fn f() { crate::ui::draw(); } crate::control::read();", &["control"]),
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

fn assert_scanned_uses() {
    for (module, source, expected) in SCANNER_CASES {
        let path = PathBuf::from(format!("src/{module}.rs"));
        let source = Source::new(path, (*source).to_owned());
        let expected: BTreeSet<_> = expected.iter().map(|name| (*name).to_owned()).collect();
        assert_eq!(source.production_uses(), expected, "module {module}: {}", source.text);
    }
}

fn assert_module_paths_and_exceptions() {
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
}

fn assert_leaves_and_sizes() {
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

fn assert_nested_indexes() {
    for (path, text, found) in [
        ("src/app/tabs.rs", "fn f(&self) { self.projects[p].workspaces[w].tabs[t].id; }", 2),
        ("src/app.rs", "let tab = &workspace . tabs [t];", 1),
        ("src/app/tabs.rs", "let n = ws.tabs.len(); let s = \".tabs[0]\"; // .workspaces[0]\n/* .tabs[1] */", 0),
        ("src/app/tabs.rs", "#[cfg(test)] mod tests { fn f() { app.projects[0].workspaces[0]; } }", 0),
        ("src/app/tabs.rs", "#[cfg(test)] fn f() { app.projects[0].workspaces[0]; } fn g() { ws.tabs[1]; }", 1),
        ("src/app/tests/tabs.rs", "app.projects[0].workspaces[0].tabs[0];", 0),
        ("src/app/tabs/tests.rs", "app.projects[0].workspaces[0];", 0),
        ("src/ui/tabs.rs", "project.workspaces[w];", 0),
    ] {
        let source = Source::new(PathBuf::from(path), text.to_owned());
        let failures = index_failures(std::slice::from_ref(&source), &[]);
        assert_eq!(failures.len(), usize::from(found > 0), "{path}: {text}");
        assert!(failures.iter().all(|failure| failure.contains(&format!(" {found} times"))), "{path}: {text}");
    }
    let clean = Source::new(PathBuf::from("src/app/tabs.rs"), "ws.tabs.first();".to_owned());
    let indexing = Source::new(PathBuf::from("src/app/tabs.rs"), "ws.tabs[0];".to_owned());
    assert!(index_failures(&[clean], &["src/app/tabs.rs"])[0].contains("no longer indexes"));
    assert_eq!(index_failures(&[indexing], &["src/app/tabs.rs"]), Vec::<String>::new());
}

#[test]
fn the_scanner_resolves_paths_and_skips_strings_and_tests() {
    assert_scanned_uses();
    assert_module_paths_and_exceptions();
    assert_leaves_and_sizes();
    assert_nested_indexes();
}
