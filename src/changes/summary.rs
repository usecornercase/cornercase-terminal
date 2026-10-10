use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use super::diff::{self, Diff, File, Fold, Status};
use super::git::{self, Loaded, Request};
use crate::error::Result;

fn statuses(raw: &[u8], prefix: &str) -> (HashMap<String, Status>, Vec<String>, HashMap<String, String>) {
    let mut records = raw.split(|b| *b == 0);
    let mut statuses = HashMap::new();
    let mut untracked = Vec::new();
    let mut renames = HashMap::new();
    while let Some(record) = records.next() {
        let text = String::from_utf8_lossy(record);
        let old_path = if record.first() == Some(&b'2') {
            records.next().map(|old| String::from_utf8_lossy(old).into_owned())
        } else {
            None
        };
        let fields: Vec<&str> = match record.first() {
            Some(b'1') => text.splitn(9, ' ').collect(),
            Some(b'2') => text.splitn(10, ' ').collect(),
            Some(b'?') => {
                if let Some(path) = text.strip_prefix("? ").and_then(|p| p.strip_prefix(prefix)) {
                    untracked.push(path.to_string());
                }
                continue;
            }
            Some(b'u') => text.splitn(11, ' ').collect(),
            _ => continue,
        };
        let (Some(xy), Some(path)) = (fields.get(1), fields.last()) else { continue };
        let Some(path) = path.strip_prefix(prefix) else { continue };
        if let Some(old) = old_path.and_then(|old| old.strip_prefix(prefix).map(str::to_string)) {
            renames.insert(path.to_string(), old);
        }
        let status = if xy.contains('R') {
            Status::Renamed
        } else if xy.contains('D') {
            Status::Deleted
        } else if xy.contains('A') {
            Status::Added
        } else {
            Status::Modified
        };
        statuses.insert(path.to_string(), status);
    }
    (statuses, untracked, renames)
}

fn counted(raw: &[u8], statuses: &HashMap<String, Status>) -> Vec<File> {
    let mut records = raw.split(|b| *b == 0);
    let mut files = Vec::new();
    while let Some(record) = records.next().filter(|r| !r.is_empty()) {
        let text = String::from_utf8_lossy(record);
        let fields: Vec<&str> = text.splitn(3, '\t').collect();
        let [added, removed, path] = fields.as_slice() else { continue };
        let (path, old_path) = if path.is_empty() {
            let old = records.next().map(|r| String::from_utf8_lossy(r).into_owned());
            let Some(new) = records.next() else { continue };
            (String::from_utf8_lossy(new).into_owned(), old)
        } else {
            ((*path).to_string(), None)
        };
        let status =
            if old_path.is_some() { Status::Renamed } else { statuses.get(&path).copied().unwrap_or(Status::Modified) };
        let binary = *added == "-";
        let (added, removed) = (added.parse().unwrap_or(0), removed.parse().unwrap_or(0));
        let fold = Fold::of(&path, binary, added, removed);
        files.push(File {
            path,
            old_path,
            status,
            added,
            removed,
            fold,
            hunks: Vec::new(),
            digest: 0,
            viewed_digest: 0,
        });
    }
    files
}

pub(super) fn load(request: &Request, expanded: &[String]) -> Result<Loaded> {
    let dir = request.dir.as_path();
    let from = git::head(dir)?;
    let status = git::run(dir, &["status", "--porcelain=v2", "-z", "--untracked-files=all", "--", "."])?;
    let numstat = git::run(
        dir,
        &[
            "diff",
            "--numstat",
            "-z",
            "--find-renames",
            "--relative",
            "--no-ext-diff",
            "--no-textconv",
            &from,
            "--",
            ".",
        ],
    )?;
    let prefix = dir
        .ancestors()
        .find(|root| root.join(".git").exists())
        .and_then(|root| dir.strip_prefix(root).ok())
        .filter(|prefix| !prefix.as_os_str().is_empty())
        .map_or_else(String::new, |prefix| format!("{}/", prefix.display()));
    let (statuses, paths, renames) = statuses(&status, &prefix);
    let untracked = git::read_untracked(dir, paths);
    let previous: HashMap<&str, &Arc<File>> = request.previous.iter().map(|f| (f.path.as_str(), f)).collect();
    let paths: Vec<&str> = expanded
        .iter()
        .filter(|path| !untracked.iter().any(|u| u.path == **path))
        .flat_map(|path| {
            let old = renames
                .get(path)
                .map(String::as_str)
                .or_else(|| previous.get(path.as_str()).and_then(|file| file.old_path.as_deref()));
            std::iter::once(path.as_str()).chain(old)
        })
        .collect();
    let patch = if paths.is_empty() { Vec::new() } else { git::patch(dir, &from, &paths)? };
    let mut hasher = DefaultHasher::new();
    (&from, &status, &numstat, &untracked, expanded, &patch).hash(&mut hasher);
    let mut tracked: Vec<_> = statuses.keys().collect();
    tracked.sort();
    for path in tracked {
        (path, git::stamp(&dir.join(path))).hash(&mut hasher);
    }
    let digest = hasher.finish();
    if request.last == Some(digest) {
        return Ok(Loaded { base: None, digest, diff: None });
    }
    let mut files = counted(&numstat, &statuses);
    files.extend(untracked.iter().map(|u| diff::untracked_file(&u.path, u.size, u.bytes.as_deref(), false)));
    for file in &mut files {
        file.viewed_digest = git::fingerprint(dir, &from, file);
        file.digest = file.viewed_digest;
    }
    let mut patches: HashMap<String, File> =
        diff::parse(&String::from_utf8_lossy(&patch)).into_iter().map(|f| (f.path.clone(), f)).collect();
    let files = files
        .into_iter()
        .map(|mut file| {
            let open = expanded.contains(&file.path);
            if open {
                let loaded = if file.status == Status::Untracked {
                    untracked
                        .iter()
                        .find(|u| u.path == file.path)
                        .map(|u| diff::untracked(&u.path, u.size, u.bytes.as_deref()))
                } else {
                    patches.remove(&file.path)
                };
                if let Some(mut loaded) = loaded {
                    loaded.viewed_digest = file.viewed_digest;
                    file = loaded;
                }
            }
            git::reuse(file, &previous)
        })
        .collect::<Vec<_>>();
    let mut files = files;
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(Loaded { base: None, digest, diff: Some(Diff { files }) })
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;
    use crate::changes::{Checkout, Mode, Panel, Scope};
    use crate::test_util::{TempDir, git, git_repo};

    fn request(dir: &std::path::Path, paths: &[&str]) -> Request {
        Request {
            dir: dir.to_path_buf(),
            mode: Mode::Uncommitted,
            base: None,
            last: None,
            previous: Vec::new(),
            expanded: Some(paths.iter().map(|s| (*s).to_string()).collect()),
        }
    }

    #[rstest::rstest]
    #[case::modified("edit")]
    #[case::staged("stage")]
    #[case::renamed("rename")]
    #[case::deleted("delete")]
    #[case::binary("binary")]
    #[case::untracked("untracked")]
    #[case::unborn("unborn")]
    #[case::lockfile("lockfile")]
    #[case::large_tracked("large")]
    #[case::large_lockfile("large-lockfile")]
    #[case::large_replacement("large-replacement")]
    #[case::large_untracked("large-untracked")]
    #[case::at_the_limit("limit")]
    fn folded_counts_match_the_project_diff(#[case] change: &str) {
        let path = if change.ends_with("lockfile") { "Cargo.lock" } else { "a.txt" };
        let original = if change == "large-replacement" {
            "old\n".repeat(diff::MAX_LINES / 2 + 1)
        } else {
            "one\ntwo\n".to_string()
        };
        let repo = if change == "unborn" {
            let repo = TempDir::new();
            git(repo.path(), &["init", "--quiet"]);
            repo
        } else {
            git_repo(&[(path, &original)])
        };
        match change {
            "rename" => git(repo.path(), &["mv", "a.txt", "a new.txt"]),
            "delete" => std::fs::remove_file(repo.path().join("a.txt")).expect("delete"),
            "binary" => std::fs::write(repo.path().join("a.txt"), b"a\0b").expect("binary"),
            "untracked" | "unborn" => std::fs::write(repo.path().join("new.txt"), "new\nlines\n").expect("new"),
            "large" | "large-lockfile" | "large-replacement" | "large-untracked" | "limit" => {
                let lines = match change {
                    "large-replacement" => diff::MAX_LINES / 2 + 1,
                    "limit" => diff::MAX_LINES - 2,
                    _ => diff::MAX_LINES + 1,
                };
                let path = if change == "large-untracked" { "new.txt" } else { path };
                std::fs::write(repo.path().join(path), "new\n".repeat(lines)).expect("many changed lines");
            }
            _ => {
                std::fs::write(repo.path().join(path), "one\nTWO\n").expect("edit");
                if change == "stage" {
                    git(repo.path(), &["add", "a.txt"]);
                }
            }
        }
        let summary = git::load(&request(repo.path(), &[])).expect("summary").diff.expect("diff");
        let full =
            git::load(&Request { expanded: None, ..request(repo.path(), &[]) }).expect("full").diff.expect("diff");
        let counts = |diff: &Diff| {
            diff.files
                .iter()
                .map(|f| (f.path.clone(), f.old_path.clone(), f.status, f.added, f.removed, f.fold, f.viewed_digest))
                .collect::<Vec<_>>()
        };
        assert_eq!(counts(&summary), counts(&full));
        assert!(summary.files.iter().all(|f| f.hunks.is_empty()));
    }

    #[rstest::rstest]
    #[case::generated_file("generated.txt")]
    #[case::lockfile("Cargo.lock")]
    fn an_unfold_request_keeps_large_tracked_files_folded_without_reading_their_patch(#[case] path: &str) {
        let repo = git_repo(&[(path, "one\n")]);
        std::fs::write(repo.path().join(path), "new\n".repeat(diff::MAX_LINES + 1)).expect("large edit");
        let targets = [(Checkout { workspace: 1, dir: repo.path().to_path_buf(), base: None }, false)];
        let now = Instant::now();
        let mut panel = Panel { scope: Scope::Every, open: true, ..Panel::default() };
        let (workspace, generation, request) = panel.request_every(&targets, now).expect("summary request");
        let result = git::load(&request).expect("summary");
        panel.loaded(workspace, generation, &request, Ok(result));
        let initial = panel.summary(workspace).expect("model").diff().expect("diff").clone();
        panel.set_fold(workspace, &initial.files[0], false);
        for visit in 0..3 {
            let (_, generation, request) = panel
                .request_every(&targets, now + Duration::from_secs(visit * 3))
                .expect("refresh after the unfold request");
            assert_eq!(request.expanded, Some(Vec::new()));
            let result = git::load(&request).expect("summary refresh");
            assert!(result.diff.is_none());
            panel.loaded(workspace, generation, &request, Ok(result));
            let diff = panel.summary(workspace).expect("model").diff().expect("diff");
            let file = &diff.files[0];
            assert_eq!(
                (file.fold, file.added, file.removed, file.hunks.len()),
                (Fold::Large, diff::MAX_LINES + 1, 1, 0)
            );
            assert!(panel.folded(workspace, diff, file));
        }
    }

    #[test]
    fn reads_only_open_files_and_refreshes_equal_count_edits() {
        let repo = git_repo(&[("a.txt", "one\n"), ("b.txt", "one\n")]);
        for path in ["a.txt", "b.txt"] {
            std::fs::write(repo.path().join(path), "two\n").expect("edit");
        }
        let mut request = request(repo.path(), &[]);
        let first = git::load(&request).expect("folded");
        request.last = Some(first.digest);
        request.previous = first.diff.expect("diff").files;
        assert!(git::load(&request).expect("unchanged").diff.is_none());
        request.expanded = Some(vec!["a.txt".into()]);
        let opened = git::load(&request).expect("open");
        let files = opened.diff.expect("diff").files;
        assert_eq!(files[0].hunks.len(), 1);
        assert_eq!(files[1].hunks, Vec::new());
        request.last = Some(opened.digest);
        request.previous = files;
        std::fs::write(repo.path().join("a.txt"), "six\n").expect("same counts");
        let changed = git::load(&request).expect("refresh").diff.expect("changed");
        assert_ne!(changed.files[0].digest, request.previous[0].digest);
        assert_eq!(changed.files[0].hunks[0].lines[1].text, "six");
    }

    #[test]
    fn unfolding_a_rename_preserves_its_old_path_status_and_line_counts() {
        let repo = git_repo(&[("old.txt", "one\ntwo\nthree\nfour\n")]);
        git(repo.path(), &["mv", "old.txt", "new.txt"]);
        std::fs::write(repo.path().join("new.txt"), "one\nTWO\nthree\nfour\n").expect("edit");
        let mut request = request(repo.path(), &[]);
        let folded = git::load(&request).expect("folded").diff.expect("diff");
        request.previous = folded.files.clone();
        request.expanded = Some(vec!["new.txt".into()]);
        let open = git::load(&request).expect("open").diff.expect("diff");
        let file = &open.files[0];
        assert_eq!(
            (file.status, file.old_path.as_deref(), file.added, file.removed),
            (Status::Renamed, Some("old.txt"), 1, 1)
        );
        assert_eq!(file.viewed_digest, folded.files[0].viewed_digest);
        assert_eq!(file.hunks.len(), 1);
        assert!(file.hunks[0].lines.iter().any(|l| l.text == "TWO"));
        request.expanded = Some(Vec::new());
        request.previous = open.files;
        let again = git::load(&request).expect("fold again").diff.expect("diff");
        assert_eq!(again.files, folded.files);
    }

    #[rstest::rstest]
    #[case::main_worktree(false)]
    #[case::linked_worktree(true)]
    fn a_subfolder_excludes_changes_outside_it_without_resolving_its_prefix_with_git(#[case] linked: bool) {
        let repo = git_repo(&[("app/a.txt", "one\n"), ("b.txt", "one\n")]);
        let checkout = TempDir::new();
        let path = checkout.path().join("topic");
        let root = if linked {
            git(repo.path(), &["worktree", "add", "--quiet", "-b", "topic", path.to_str().expect("path")]);
            path.as_path()
        } else {
            repo.path()
        };
        std::fs::write(root.join("app/a.txt"), "two\n").expect("inside");
        std::fs::write(root.join("b.txt"), "two\n").expect("outside");
        std::fs::write(root.join("app/new.txt"), "new\n").expect("new");
        let loaded = git::load(&request(&root.join("app"), &[])).expect("summary").diff.expect("diff");
        assert_eq!(
            loaded.files.iter().map(|f| (f.path.as_str(), f.status)).collect::<Vec<_>>(),
            [("a.txt", Status::Modified), ("new.txt", Status::Untracked)]
        );
    }

    #[test]
    fn a_summary_leaves_the_index_and_its_lock_alone() {
        let repo = git_repo(&[("a.txt", "one\n")]);
        std::fs::write(repo.path().join("a.txt"), "two\n").expect("edit");
        let index = repo.path().join(".git/index");
        let before = std::fs::read(&index).expect("index");
        let time = std::fs::metadata(&index).expect("metadata").modified().expect("time");
        std::fs::write(repo.path().join(".git/index.lock"), "held").expect("lock");
        git::load(&request(repo.path(), &["a.txt"])).expect("locked index");
        assert_eq!(std::fs::read(index).expect("index"), before);
        assert_eq!(
            std::fs::metadata(repo.path().join(".git/index")).expect("metadata").modified().expect("time"),
            time
        );
    }
}
