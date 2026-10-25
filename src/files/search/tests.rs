use std::fs;

use super::*;
use crate::test_util::git_repo;

fn paths(names: &[&str]) -> Vec<String> {
    names.iter().map(|n| (*n).to_string()).collect()
}

#[test]
fn indexes_the_files_git_does_not_ignore() {
    let repo = git_repo(&[(".gitignore", "target/\n"), ("src/main.rs", ""), ("README.md", "")]);
    fs::create_dir_all(repo.path().join("target/debug")).expect("mkdir");
    fs::write(repo.path().join("target/debug/app"), "").expect("write");
    assert_eq!(index(repo.path(), &AtomicBool::new(false)), [".gitignore", "README.md", "src/main.rs"]);
}

#[test]
fn finds_names_by_their_letters_best_first() {
    let paths = paths(&["src/relay/pty-handler.test.ts", "src/relay/pty-handler.ts", "docs/handbook.md"]);
    let found: Vec<String> = names(&paths, "ptyhandler").into_iter().map(|n| n.path).collect();
    assert_eq!(found, ["src/relay/pty-handler.ts", "src/relay/pty-handler.test.ts"]);
}

#[test]
fn puts_the_file_whose_name_is_the_query_before_longer_names_in_shorter_paths() {
    let paths = paths(&[
        "src/relay/workspace-session-handler.ts",
        "src/shared/workspace-session-schema.ts",
        "src/renderer/src/lib/workspace-session.ts",
    ]);
    let found: Vec<String> = names(&paths, "workspace-session").into_iter().map(|n| n.path).collect();
    assert_eq!(found[0], "src/renderer/src/lib/workspace-session.ts");
}

#[test]
fn marks_the_letters_that_matched() {
    let found = names(&paths(&["src/main.rs"]), "main");
    assert_eq!(found[0].indices, [4, 5, 6, 7]);
}

#[test]
fn finds_text_with_its_line_and_where_it_matched() {
    let repo = git_repo(&[("a.rs", "fn main() {\n    run_all();\n}\n"), ("b.md", "nothing")]);
    let text = text(repo.path(), &paths(&["a.rs", "b.md"]), "run", &AtomicBool::new(false));
    assert_eq!(text.matches, 1);
    assert_eq!(text.files[0].path, "a.rs");
    let ranges = std::iter::once(0..3).collect();
    assert_eq!(text.files[0].lines, [Line { number: 2, text: "run_all();".into(), ranges }]);
}

#[test]
fn lower_case_finds_any_case_and_upper_case_only_itself() {
    let repo = git_repo(&[("a.txt", "Order\norder\n")]);
    let count = |q| text(repo.path(), &paths(&["a.txt"]), q, &AtomicBool::new(false)).matches;
    assert_eq!((count("order"), count("Order")), (2, 1));
}

#[test]
fn skips_binary_files() {
    let repo = git_repo(&[("a.txt", "needle\n")]);
    fs::write(repo.path().join("b.bin"), b"needle\0\x01").expect("write");
    let text = text(repo.path(), &paths(&["a.txt", "b.bin"]), "needle", &AtomicBool::new(false));
    assert_eq!(text.files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(), ["a.txt"]);
}

#[test]
fn stops_at_the_cap() {
    let repo = git_repo(&[("a.txt", &"x\n".repeat(MAX_MATCHES + 50))]);
    let text = text(repo.path(), &paths(&["a.txt"]), "x", &AtomicBool::new(false));
    assert_eq!((text.matches, text.capped), (MAX_MATCHES, true));
}

#[test]
fn exactly_the_cap_is_not_cut() {
    let repo = git_repo(&[("a.txt", &"x\n".repeat(MAX_MATCHES))]);
    let text = text(repo.path(), &paths(&["a.txt"]), "x", &AtomicBool::new(false));
    assert_eq!((text.matches, text.capped), (MAX_MATCHES, false));
}

#[test]
fn a_match_past_the_cap_in_the_next_file_counts_as_cut() {
    let repo = git_repo(&[("a.txt", &"x\n".repeat(MAX_MATCHES)), ("b.txt", "x\n")]);
    let text = text_on(repo.path(), &paths(&["a.txt", "b.txt"]), "x", &AtomicBool::new(false), 1);
    assert_eq!((text.matches, text.capped), (MAX_MATCHES, true));
}

#[test]
fn threads_never_keep_more_than_the_cap() {
    let names: Vec<String> = (0..32).map(|i| format!("{i:02}.txt")).collect();
    let body = "x\n".repeat(100);
    let files: Vec<(&str, &str)> = names.iter().map(|n| (n.as_str(), body.as_str())).collect();
    let repo = git_repo(&files);
    let text = text_on(repo.path(), &names, "x", &AtomicBool::new(false), MAX_THREADS);
    let kept: usize = text.files.iter().map(|f| f.lines.len()).sum();
    assert_eq!((kept, text.capped), (MAX_MATCHES, true));
}

#[test]
fn a_cancelled_search_finds_nothing_more() {
    let repo = git_repo(&[("a.txt", "needle\n")]);
    let text = text(repo.path(), &paths(&["a.txt"]), "needle", &AtomicBool::new(true));
    assert_eq!(text.matches, 0);
}

#[test]
fn a_cancelled_walk_indexes_nothing() {
    let repo = git_repo(&[("src/a.rs", ""), ("src/b.rs", "")]);
    assert_eq!(index(repo.path(), &AtomicBool::new(true)), Vec::<String>::new());
}
