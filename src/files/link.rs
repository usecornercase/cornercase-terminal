use std::path::{Path, PathBuf};

use crate::config::expand_home;
use crate::emulator::Cell;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub start: u16,
    pub end: u16,
    pub path: String,
    pub lines: Option<(u32, u32)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub path: String,
    pub lines: Option<(u32, u32)>,
}

const TRAILING: [char; 5] = ['.', ',', ':', ';', '#'];

fn path_char(symbol: &str) -> bool {
    let mut chars = symbol.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => c.is_alphanumeric() || "_-./~@+,%:#".contains(c),
        _ => false,
    }
}

fn number(text: &str) -> Option<u32> {
    (!text.is_empty() && text.bytes().all(|b| b.is_ascii_digit())).then(|| text.parse().ok()).flatten()
}

fn location(rest: &str) -> Option<(u32, u32)> {
    let (first, tail) =
        rest.split_once(['-', ':']).map_or((rest, None), |(a, b)| (a, Some((b, rest.as_bytes()[a.len()]))));
    let first = number(first)?;
    match tail {
        Some((last, b'-')) => Some((first, number(last.trim_start_matches('L'))?.max(first))),
        _ => Some((first, first)),
    }
}

fn split(token: &str) -> (&str, Option<(u32, u32)>) {
    if let Some((path, rest)) = token.rsplit_once("#L") {
        return (path, location(rest));
    }
    let name = token.rfind('/').map_or(0, |i| i + 1);
    let colon = token[name..].find(':').map(|i| name + i);
    match colon.and_then(|i| Some((i, location(&token[i + 1..])?))) {
        Some((i, lines)) => (&token[..i], Some(lines)),
        None => (token, None),
    }
}

pub fn at(row: &[Cell], col: u16) -> Option<Link> {
    let col = usize::from(col);
    if !row.get(col).is_some_and(|c| path_char(&c.symbol)) {
        return None;
    }
    let start = (0..col).rev().take_while(|&i| path_char(&row[i].symbol)).last().unwrap_or(col);
    let mut end = (col..row.len()).take_while(|&i| path_char(&row[i].symbol)).last().map_or(col, |i| i + 1);
    let mut token: String = row[start..end].iter().map(|c| c.symbol.as_str()).collect();
    while token.ends_with(TRAILING) {
        token.pop();
        end -= 1;
    }
    if col >= end || token.contains("://") {
        return None;
    }
    let (path, lines) = split(&token);
    if path.is_empty() || path.chars().all(|c| c == '.' || c == '/') {
        return None;
    }
    let column = |i: usize| u16::try_from(i).unwrap_or(u16::MAX);
    Some(Link { start: column(start), end: column(end), path: path.to_string(), lines })
}

pub fn resolve(link: &Link, cwd: Option<&Path>, root: &Path, home: Option<&Path>) -> Option<Target> {
    let root = root.canonicalize().ok()?;
    let given = expand_home(&link.path, home);
    let candidates: Vec<PathBuf> = if given.is_absolute() {
        vec![given]
    } else {
        cwd.map(|dir| dir.join(&given)).into_iter().chain(std::iter::once(root.join(&given))).collect()
    };
    candidates.into_iter().find_map(|candidate| {
        let real = candidate.canonicalize().ok().filter(|p| p.is_file())?;
        let path = real.strip_prefix(&root).unwrap_or(&real).to_string_lossy().into_owned();
        Some(Target { path, lines: link.lines })
    })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::process::Command;

    use ratatui::style::Style;
    use rstest::rstest;

    use super::*;
    use crate::test_util::TempDir;

    fn row(text: &str) -> Vec<Cell> {
        text.chars().map(|c| Cell { symbol: c.to_string(), style: Style::default() }).collect()
    }

    struct Seen {
        path: String,
        lines: Option<(u32, u32)>,
        text: String,
    }

    fn link_in(text: &str, word: &str) -> Option<Seen> {
        let col = u16::try_from(text.find(word).expect("the word is in the line")).expect("fits");
        let cells = row(text);
        at(&cells, col).map(|l| {
            let text = cells[usize::from(l.start)..usize::from(l.end)].iter().map(|c| c.symbol.as_str()).collect();
            Seen { path: l.path, lines: l.lines, text }
        })
    }

    #[rstest]
    #[case::plain("  ⎿  Updated src/files/mod.rs with 3 additions", "files", "src/files/mod.rs", None)]
    #[case::inside_a_tool_call("● Update(src/app.rs)", "app", "src/app.rs", None)]
    #[case::in_backticks("see `src/ui.rs` for that", "ui.rs", "src/ui.rs", None)]
    #[case::a_line("error at src/app.rs:120", "app", "src/app.rs", Some((120, 120)))]
    #[case::a_line_and_column("  --> src/app.rs:120:5", "app", "src/app.rs", Some((120, 120)))]
    #[case::a_range("look at src/app.rs:120-140.", "app", "src/app.rs", Some((120, 140)))]
    #[case::a_github_range("docs/x.md#L3-L9", "docs", "docs/x.md", Some((3, 9)))]
    #[case::grep_output("src/main.rs:2:    run();", "main", "src/main.rs", Some((2, 2)))]
    #[case::the_end_of_a_sentence("I changed README.md.", "README", "README.md", None)]
    #[case::an_absolute_path("open /tmp/a/b.txt now", "tmp", "/tmp/a/b.txt", None)]
    #[case::a_home_path("see ~/.claude/plans/fix.md:3-9", "plans", "~/.claude/plans/fix.md", Some((3, 9)))]
    #[case::a_rust_path_stays_whole("use std::collections::HashMap;", "std", "std::collections::HashMap", None)]
    fn finds_the_path_under_the_pointer(
        #[case] text: &str,
        #[case] word: &str,
        #[case] path: &str,
        #[case] lines: Option<(u32, u32)>,
    ) {
        let seen = link_in(text, word).expect("a link");
        assert_eq!((seen.path.as_str(), seen.lines), (path, lines));
    }

    #[test]
    fn the_link_covers_the_path_and_its_line_but_not_the_punctuation() {
        assert_eq!(link_in("see src/a.rs:12, then", "src").map(|l| l.text), Some("src/a.rs:12".to_string()));
    }

    #[rstest]
    #[case::a_space("a  b", " ")]
    #[case::a_url("go to https://example.com/x", "example")]
    #[case::dots_alone("wait...", "...")]
    fn some_text_is_no_link(#[case] text: &str, #[case] word: &str) {
        assert!(link_in(text, word).is_none());
    }

    #[test]
    fn resolves_from_the_programs_folder_then_the_workspace() {
        let root = TempDir::new();
        fs::create_dir_all(root.path().join("crate/src")).expect("mkdir");
        fs::write(root.path().join("crate/src/lib.rs"), "").expect("write");
        fs::write(root.path().join("README.md"), "").expect("write");
        let link = |path: &str| Link { start: 0, end: 1, path: path.into(), lines: Some((3, 3)) };
        let from = |path: &str, cwd: &Path| resolve(&link(path), Some(cwd), root.path(), None).map(|t| t.path);
        let cwd = root.path().join("crate");
        assert_eq!(from("src/lib.rs", &cwd).as_deref(), Some("crate/src/lib.rs"));
        assert_eq!(from("README.md", &cwd).as_deref(), Some("README.md"));
        assert_eq!(from("missing.rs", &cwd), None);
        assert_eq!(from("crate/src", &cwd), None, "folders are not opened");
    }

    #[test]
    fn a_file_outside_the_workspace_opens_with_its_absolute_path() {
        let (root, other) = (TempDir::new(), TempDir::new());
        fs::write(other.path().join("x.txt"), "").expect("write");
        let path = other.path().join("x.txt").display().to_string();
        let target =
            resolve(&Link { start: 0, end: 1, path: path.clone(), lines: Some((2, 4)) }, None, root.path(), None);
        assert_eq!(target, Some(Target { path, lines: Some((2, 4)) }));
    }

    #[rstest]
    #[case::inside_the_workspace(true)]
    #[case::outside_the_workspace(false)]
    fn expands_home_paths(#[case] inside: bool) {
        let (root, other) = (TempDir::new(), TempDir::new());
        let home = if inside { root.path() } else { other.path() };
        fs::create_dir_all(home.join(".claude/plans")).expect("mkdir");
        let file = home.join(".claude/plans/fix.md");
        fs::write(&file, "# Plan").expect("write");
        let link = Link { start: 0, end: 1, path: "~/.claude/plans/fix.md".into(), lines: Some((3, 9)) };
        let path = if inside { ".claude/plans/fix.md".to_string() } else { file.display().to_string() };
        assert_eq!(
            resolve(&link, Some(root.path()), root.path(), Some(home)),
            Some(Target { path, lines: link.lines })
        );
        assert!(resolve(&link, None, root.path(), None).is_none());
    }

    #[rstest]
    #[case::from_an_outside_cwd("x.txt", "other")]
    #[case::from_a_parent("../other/x.txt", "repo")]
    #[case::through_a_symlink("linked.txt", "repo")]
    fn relative_paths_can_reach_outside_the_workspace(#[case] path: &str, #[case] cwd: &str) {
        let dir = TempDir::new();
        let root = dir.path().join("repo");
        fs::create_dir_all(&root).expect("mkdir");
        fs::create_dir(dir.path().join("other")).expect("mkdir");
        let file = dir.path().join("other/x.txt");
        fs::write(&file, "outside").expect("write");
        std::os::unix::fs::symlink(&file, root.join("linked.txt")).expect("symlink");
        fs::write(root.join("x.txt"), "inside").expect("write");
        let link = Link { start: 0, end: 1, path: path.into(), lines: None };
        let target = resolve(&link, Some(&dir.path().join(cwd)), &root, None);
        assert_eq!(target, Some(Target { path: file.display().to_string(), lines: None }));
    }

    #[test]
    fn an_absolute_path_inside_the_workspace_stays_relative() {
        let root = TempDir::new();
        let file = root.path().join("note.md");
        fs::write(&file, "note").expect("write");
        let link = Link { start: 0, end: 1, path: file.display().to_string(), lines: None };
        assert_eq!(resolve(&link, None, root.path(), None), Some(Target { path: "note.md".into(), lines: None }));
    }

    #[test]
    fn outside_folders_devices_and_fifos_are_not_links() {
        let (root, other) = (TempDir::new(), TempDir::new());
        let fifo = other.path().join("pipe");
        assert!(Command::new("mkfifo").arg(&fifo).status().expect("create fifo").success());
        let device = other.path().join("device");
        std::os::unix::fs::symlink("/dev/zero", &device).expect("symlink");
        for path in [other.path(), fifo.as_path(), device.as_path(), Path::new("/dev/zero")] {
            let link = Link { start: 0, end: 1, path: path.display().to_string(), lines: None };
            assert!(resolve(&link, None, root.path(), None).is_none(), "{} is not a regular file", path.display());
        }
    }
}
