use std::path::Path;

use super::*;
use crate::changes::diff::{Fold, Hunk, Kind, Line, Status};
use crate::test_util;

pub(super) fn digest(dir: &Path, path: &str) -> u64 {
    let request = git::Request {
        dir: dir.to_path_buf(),
        mode: Mode::Uncommitted,
        base: None,
        last: None,
        previous: Vec::new(),
        expanded: None,
    };
    let diff = git::load(&request).expect("load").diff.expect("diff");
    diff.files.iter().find(|f| f.path == path).expect("the file is listed").viewed_digest
}

pub(super) fn status(dir: &Path, paths: &[&str]) -> String {
    let mut args = vec!["status", "--porcelain", "--"];
    args.extend(paths);
    let out = std::process::Command::new("git").arg("-C").arg(dir).args(args).output().expect("run git");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

pub(super) fn write(dir: &Path, path: &str, text: &str) {
    std::fs::write(dir.join(path), text).expect("write file");
}

pub(super) fn tracked_change(dir: &Path, kind: &str) {
    match kind {
        "modified" => write(dir, "a.txt", "one\nTWO\n"),
        "staged_and_unstaged" => {
            write(dir, "a.txt", "one\nTWO\n");
            test_util::git(dir, &["add", "a.txt"]);
            write(dir, "a.txt", "one\nTWO\nthree\n");
        }
        "deleted" => std::fs::remove_file(dir.join("a.txt")).expect("remove"),
        "deleted_with_git_rm" => test_util::git(dir, &["rm", "--quiet", "a.txt"]),
        "renamed" => test_util::git(dir, &["mv", "a.txt", "c.txt"]),
        _ => {
            test_util::git(dir, &["mv", "a.txt", "c.txt"]);
            write(dir, "c.txt", "one\ntwo\nthree\n");
        }
    }
}

pub(super) fn new_change(dir: &Path, kind: &str) {
    match kind {
        "added_in_a_new_folder" => {
            std::fs::create_dir(dir.join("new")).expect("folder");
            write(dir, "new/b.txt", "b\n");
            test_util::git(dir, &["add", "new/b.txt"]);
        }
        "intent_to_add" => {
            write(dir, "b.txt", "b\n");
            test_util::git(dir, &["add", "-N", "b.txt"]);
        }
        _ => write(dir, "b.txt", "b\n"),
    }
}

fn file(path: &str, digest: u64, lines: usize) -> File {
    let line = |n| Line {
        kind: Kind::Added,
        old: None,
        new: Some(n),
        text: String::new(),
        raw: None,
        syntax: None,
        emphasis: Vec::new(),
    };
    let lines: Vec<Line> = (1..=u32::try_from(lines).expect("lines")).map(line).collect();
    File {
        path: path.into(),
        old_path: None,
        status: Status::Modified,
        added: lines.len(),
        removed: 0,
        fold: Fold::Open,
        hunks: vec![Hunk { old_start: 1, new_start: 1, context: String::new(), lines }],
        digest,
        viewed_digest: digest,
        staged: None,
    }
}

fn diff(files: Vec<File>) -> Diff {
    Diff { files: files.into_iter().map(Arc::new).collect() }
}

fn target(workspace: u64) -> Checkout {
    Checkout { workspace, dir: PathBuf::from("/repo"), base: None }
}

fn loaded(diff: Option<Diff>, digest: u64) -> git::Loaded {
    git::Loaded { base: None, digest, diff }
}

mod refresh {
    use super::*;

    #[test]
    fn one_request_at_a_time() {
        let mut panel = Panel::default();
        let now = Instant::now();
        assert!(panel.request(&target(1), now).is_some());
        assert!(panel.request(&target(1), now + Duration::from_secs(10)).is_none());
    }

    #[test]
    fn waits_a_second_while_open_and_three_while_closed() {
        let mut panel = Panel { watched: true, ..Panel::default() };
        let now = Instant::now();
        let (generation, request) = panel.request(&target(1), now).expect("first");
        panel.loaded(1, generation, &request, Ok(loaded(Some(Diff::default()), 7)));
        assert!(panel.request(&target(1), now + Duration::from_millis(500)).is_none());
        let (generation, request) = panel.request(&target(1), now + OPEN_EVERY).expect("open");
        panel.loaded(1, generation, &request, Ok(loaded(None, 7)));
        panel.watched = false;
        assert!(panel.request(&target(1), now + OPEN_EVERY * 2).is_none());
        assert!(panel.request(&target(1), now + OPEN_EVERY + CLOSED_EVERY).is_some());
    }

    #[test]
    fn passes_the_last_digest_and_files_along() {
        let mut panel = Panel::default();
        let now = Instant::now();
        let (generation, request) = panel.request(&target(1), now).expect("first");
        panel.loaded(1, generation, &request, Ok(loaded(Some(diff(vec![file("a", 1, 1)])), 42)));
        let (_, request) = panel.request(&target(1), now + CLOSED_EVERY).expect("again");
        assert_eq!((request.last, request.previous.len()), (Some(42), 1));
    }

    #[test]
    fn a_new_mode_asks_at_once() {
        let mut panel = Panel::default();
        let now = Instant::now();
        let (generation, request) = panel.request(&target(1), now).expect("first");
        panel.loaded(1, generation, &request, Ok(loaded(Some(Diff::default()), 1)));
        panel.set_mode(Mode::Commits);
        let (_, request) = panel.request(&target(1), now).expect("new mode");
        assert_eq!((request.mode, request.last), (Mode::Commits, None));
    }

    #[test]
    fn an_old_answer_does_not_replace_a_newer_mode() {
        let mut panel = Panel::default();
        let (generation, request) = panel.request(&target(1), Instant::now()).expect("first");
        panel.set_mode(Mode::All);
        panel.loaded(1, generation, &request, Ok(loaded(Some(Diff::default()), 1)));
        assert!(panel.model(1, None).is_none());
    }

    #[rstest::rstest]
    #[case::project(Scope::Project)]
    #[case::every_project(Scope::Every)]
    fn a_job_stays_the_only_one_across_repeated_timeouts(#[case] scope: Scope) {
        let mut panel = Panel { scope, ..Panel::default() };
        let now = Instant::now();
        let ask = |panel: &mut Panel, workspace, at| {
            if scope == Scope::Every {
                panel.request_every(&[(target(workspace), false)], at).map(|(_, g, r)| (g, r))
            } else {
                panel.request(&target(workspace), at)
            }
        };
        let (generation, request) = ask(&mut panel, 1, now).expect("first");
        for step in 1..=3 {
            assert!(ask(&mut panel, 2, now + GIVE_UP_AFTER * step).is_none(), "step {step}");
        }
        panel.loaded(1, generation, &request, Ok(loaded(Some(Diff::default()), 1)));
        let model = if scope == Scope::Every { panel.summary(1) } else { panel.model(1, None) };
        assert!(model.is_some());
        assert!(ask(&mut panel, 2, now + GIVE_UP_AFTER * 4).is_some());
    }

    #[test]
    fn a_timed_out_job_keeps_the_diff_already_shown_and_frees_the_slot() {
        let mut panel = Panel::default();
        let now = Instant::now();
        let (generation, request) = panel.request(&target(1), now).expect("first");
        panel.loaded(1, generation, &request, Ok(loaded(Some(diff(vec![file("a", 1, 1)])), 7)));
        let (generation, request) = panel.request(&target(1), now + CLOSED_EVERY).expect("again");
        panel.loaded(1, generation, &request, Err(crate::error::Error::Git("timed out".into())));
        assert_eq!(panel.model(1, None).and_then(Model::diff).map(|d| d.files.len()), Some(1));
        assert!(panel.request(&target(1), now + CLOSED_EVERY * 2).is_some());
    }

    #[test]
    fn a_closed_panel_asks_for_counts_and_one_open_file() {
        let mut panel = Panel::default();
        let now = Instant::now();
        let preview = vec!["src/main.rs".to_string()];
        let (_, request) = panel.request_counts(&target(1), &preview, now).expect("counts");
        assert_eq!(request.expanded.as_deref(), Some(preview.as_slice()));
        assert!(panel.request_counts(&target(1), &preview, now + GIVE_UP_AFTER * 3).is_none());
        panel.loaded(1, 1, &request, Ok(loaded(Some(Diff::default()), 1)));
        assert!(panel.request_counts(&target(1), &preview, now + CLOSED_EVERY).is_some());
    }

    #[test]
    fn an_unchanged_answer_without_a_model_is_ignored() {
        let mut panel = Panel::default();
        let (generation, request) = panel.request(&target(1), Instant::now()).expect("first");
        panel.loaded(1, generation, &request, Ok(loaded(None, 1)));
        assert!(panel.model(1, None).is_none());
    }

    #[test]
    fn an_error_is_kept_as_the_model() {
        let mut panel = Panel::default();
        let (generation, request) = panel.request(&target(1), Instant::now()).expect("first");
        panel.loaded(1, generation, &request, Err(crate::error::Error::Git("boom".into())));
        assert_eq!(panel.model(1, None).map(|m| m.result.clone().err()), Some(Some("boom".into())));
    }
}

mod folding {
    use super::*;

    #[test]
    fn small_diffs_start_open() {
        let d = diff(vec![file("a", 1, 3)]);
        assert!(!Panel::default().folded(1, &d, &d.files[0]));
    }

    #[test]
    fn big_diffs_start_folded() {
        let d = diff(vec![file("a", 1, AUTO_FOLD_LINES + 1)]);
        assert!(Panel::default().folded(1, &d, &d.files[0]));
    }

    #[test]
    fn a_viewed_file_folds_until_it_changes() {
        let d = diff(vec![file("a", 1, 3)]);
        let mut panel = Panel::default();
        panel.toggle_viewed(1, &d.files[0]);
        let changed = diff(vec![file("a", 2, 3)]);
        assert_eq!((panel.folded(1, &d, &d.files[0]), panel.folded(1, &changed, &changed.files[0])), (true, false));
    }

    #[test]
    fn fold_all_folds_then_unfolds() {
        let d = diff(vec![file("a", 1, 3), file("b", 2, 3)]);
        let mut panel = Panel::default();
        panel.fold_all(1, &d);
        let folded = |panel: &Panel| d.files.iter().map(|f| panel.folded(1, &d, f)).collect::<Vec<_>>();
        assert_eq!(folded(&panel), [true, true]);
        panel.fold_all(1, &d);
        assert_eq!(folded(&panel), [false, false]);
    }

    #[test]
    fn viewed_changes_open_again_in_every_project() {
        let d = diff(vec![file("a", 1, 3)]);
        let mut panel = Panel { scope: Scope::Every, ..Panel::default() };
        panel.toggle_viewed(1, &d.files[0]);
        let changed = diff(vec![file("a", 2, 3)]);
        assert!(panel.folded(1, &d, &d.files[0]));
        assert!(!panel.folded(1, &changed, &changed.files[0]));
        assert!(!panel.viewed(1, &changed.files[0]));
    }

    #[test]
    fn binary_files_never_open() {
        let d = diff(vec![File { fold: Fold::Binary, ..file("a", 1, 0) }]);
        let mut panel = Panel::default();
        panel.toggle_fold(1, &d, &d.files[0]);
        assert!(panel.folded(1, &d, &d.files[0]));
    }
}

mod every_project {
    use super::*;

    #[test]
    fn summaries_take_turns_in_one_slot_and_wait_three_seconds() {
        let mut panel = Panel { watched: true, scope: Scope::Every, ..Panel::default() };
        let targets = [(target(1), false), (target(2), false)];
        let now = Instant::now();
        let (ws, generation, request) = panel.request_every(&targets, now).expect("first");
        assert_eq!((ws, request.expanded.as_deref()), (1, Some([].as_slice())));
        assert!(panel.request_every(&targets, now + Duration::from_secs(10)).is_none());
        assert!(panel.request(&target(1), now + Duration::from_secs(10)).is_none());
        panel.loaded(ws, generation, &request, Ok(loaded(Some(diff(vec![file("a", 1, 1)])), 1)));
        let (ws, generation, request) = panel.request_every(&targets, now).expect("second");
        assert_eq!(ws, 2);
        panel.loaded(ws, generation, &request, Ok(loaded(Some(Diff::default()), 2)));
        assert!(panel.request_every(&targets, now + Duration::from_secs(2)).is_none());
        assert_eq!(panel.request_every(&targets, now + CLOSED_EVERY).expect("round again").0, 1);
    }

    #[test]
    fn unfolding_requests_a_patch_and_header_folding_stops_reading_it() {
        let mut panel = Panel { watched: true, scope: Scope::Every, ..Panel::default() };
        let now = Instant::now();
        let targets = [(target(1), false)];
        let d = diff(vec![file("a", 1, 1)]);
        let (ws, generation, request) = panel.request_every(&targets, now).expect("summary");
        panel.loaded(ws, generation, &request, Ok(loaded(Some(d.clone()), 1)));
        assert!(panel.folded(1, &d, &d.files[0]));
        panel.toggle_fold(1, &d, &d.files[0]);
        let (ws, generation, request) = panel.request_every(&targets, now).expect("patch");
        assert_eq!(request.expanded.as_deref(), Some(["a".to_string()].as_slice()));
        panel.loaded(ws, generation, &request, Ok(loaded(Some(d), 2)));
        panel.invalidate_summary(1);
        let (_, _, request) = panel.request_every(&[(target(1), true)], now).expect("hidden");
        assert_eq!(request.expanded.expect("summary"), Vec::<String>::new());
    }

    #[test]
    fn summary_answers_do_not_replace_the_project_model() {
        let mut panel = Panel::default();
        let now = Instant::now();
        let (generation, request) = panel.request(&target(1), now).expect("project");
        panel.loaded(1, generation, &request, Ok(loaded(Some(diff(vec![file("project", 1, 1)])), 1)));
        panel.scope = Scope::Every;
        let (ws, generation, request) = panel.request_every(&[(target(1), false)], now).expect("summary");
        panel.loaded(ws, generation, &request, Ok(loaded(Some(diff(vec![file("every", 2, 1)])), 2)));
        assert_eq!(panel.model(1, None).expect("project").diff().expect("diff").files[0].path, "project");
        assert_eq!(panel.summary(1).expect("summary").diff().expect("diff").files[0].path, "every");
    }
}

#[rstest::rstest]
#[case::project(Scope::Project)]
#[case::every_project(Scope::Every)]
fn forgetting_a_workspace_releases_all_its_caches_and_ignores_an_in_flight_answer(#[case] scope: Scope) {
    let mut panel = Panel::default();
    let now = Instant::now();
    for workspace in [1, 2] {
        let d = diff(vec![file("a", workspace, 1)]);
        panel.scope = Scope::Project;
        let (generation, request) = panel.request(&target(workspace), now).expect("project");
        panel.loaded(workspace, generation, &request, Ok(loaded(Some(d.clone()), workspace)));
        panel.toggle_viewed(workspace, &d.files[0]);
        panel.set_fold(workspace, &d.files[0], true);
        panel.scope = Scope::Every;
        panel.set_fold(workspace, &d.files[0], true);
        panel.set_gap(workspace, &d.files[0], 1, Vec::new());
        panel.headers.insert(workspace);
        let (_, generation, request) = panel.request_every(&[(target(workspace), false)], now).expect("summary");
        panel.loaded(workspace, generation, &request, Ok(loaded(Some(d), workspace)));
    }
    let (generation, request) = if scope == Scope::Every {
        let (_, generation, request) =
            panel.request_every(&[(target(1), false)], now + CLOSED_EVERY).expect("in flight");
        (generation, request)
    } else {
        panel.request(&target(1), now + CLOSED_EVERY).expect("in flight")
    };
    panel.workspace = Some(1);
    panel.forget(1);
    assert_eq!(panel.workspace, None);
    for keys in [
        panel.models.keys().map(|k| k.0).collect::<Vec<_>>(),
        panel.summaries.keys().copied().collect(),
        panel.requested.keys().map(|k| k.0).collect(),
        panel.summary_requested.keys().copied().collect(),
    ] {
        assert_eq!(keys, [2]);
    }
    for folds in [&panel.folded, &panel.every_folded] {
        assert!(folds.keys().all(|k| k.workspace == 2));
    }
    assert!(panel.viewed.keys().all(|k| k.workspace == 2));
    assert!(panel.gaps.keys().all(|(k, _, _)| k.workspace == 2));
    assert_eq!(panel.headers, HashSet::from([2]));
    panel.loaded(1, generation, &request, Ok(loaded(Some(Diff::default()), 1)));
    assert!(panel.model(1, None).is_none() && panel.summary(1).is_none());
    assert!(panel.request(&target(2), now + CLOSED_EVERY).is_some());
}

#[test]
fn gap_lines_come_from_the_new_side_between_two_hunks() {
    let mut f = file("a.txt", 1, 0);
    let line =
        |kind, old, new| Line { kind, old, new, text: "x".into(), raw: None, syntax: None, emphasis: Vec::new() };
    f.hunks = vec![
        Hunk { old_start: 1, new_start: 1, context: String::new(), lines: vec![line(Kind::Added, None, Some(1))] },
        Hunk { old_start: 4, new_start: 5, context: String::new(), lines: vec![line(Kind::Context, Some(4), Some(5))] },
    ];
    let new_side: Vec<String> = (1..=6).map(|n| format!("line {n}")).collect();
    let gap: Vec<(u32, u32, String)> =
        gap_lines(&f, 1, &new_side).into_iter().map(|g| (g.old, g.new, g.text)).collect();
    assert_eq!(gap, [(1, 2, "line 2".into()), (2, 3, "line 3".into()), (3, 4, "line 4".into())]);
}

#[test]
fn gap_lines_show_tabs_expanded_and_keep_their_own_text() {
    let mut f = file("a.txt", 1, 0);
    let line =
        |kind, old, new| Line { kind, old, new, text: "x".into(), raw: None, syntax: None, emphasis: Vec::new() };
    f.hunks = vec![
        Hunk { old_start: 1, new_start: 1, context: String::new(), lines: vec![line(Kind::Added, None, Some(1))] },
        Hunk { old_start: 3, new_start: 4, context: String::new(), lines: vec![line(Kind::Context, Some(3), Some(4))] },
    ];
    let new_side: Vec<String> = ["one", "\ttwo", "three", "four"].map(String::from).to_vec();
    let gap = gap_lines(&f, 1, &new_side);
    assert_eq!(gap.iter().map(|g| g.text.as_str()).collect::<Vec<_>>(), ["    two", "three"]);
    assert_eq!(gap.iter().map(GapLine::source).collect::<Vec<_>>(), ["\ttwo", "three"]);
}

mod branch_picker {
    use super::*;

    fn picker() -> BranchPicker {
        let branches = ["main", "feature/login", "origin/main"].map(String::from).to_vec();
        BranchPicker::new(1, branches, Some("origin/main".into()), Some("main".into()))
    }

    #[test]
    fn starts_on_the_current_base() {
        assert_eq!(picker().chosen(None).as_deref(), Some("main"));
    }

    #[test]
    fn filters_as_you_type() {
        let mut p = picker();
        "orig".chars().for_each(|c| p.push(c));
        assert_eq!((p.items(), p.chosen(None).as_deref()), (vec!["origin/main"], Some("origin/main")));
    }

    #[test]
    fn tags_the_current_and_default_branches() {
        let p = picker();
        assert_eq!(
            (p.tag("main"), p.tag("origin/main"), p.tag("feature/login")),
            (Some("current"), Some("default"), None)
        );
    }
}

#[test]
fn tints_use_the_palette_without_truecolor() {
    let tints = Tints::of(&HostTheme::default());
    assert_eq!((tints.removed, tints.removed_word), (None, Color::Indexed(52)));
}

#[test]
fn tints_blend_with_the_background_in_truecolor() {
    let background = Some(libghostty_vt::style::RgbColor { r: 0, g: 0, b: 0 });
    let tints = Tints::of(&HostTheme { background, truecolor: true, ..HostTheme::default() });
    assert_eq!(tints.added, Some(Color::Rgb(2, 26, 17)));
}

mod modes {
    use super::*;

    fn load(panel: &mut Panel, request: Option<(u64, git::Request)>, workspace: u64, digest: u64) {
        let (generation, request) = request.expect("a request");
        panel.loaded(workspace, generation, &request, Ok(loaded(Some(Diff::default()), digest)));
    }

    #[test]
    fn a_followed_mode_keeps_its_model_beside_the_windows() {
        let mut panel = Panel::default();
        let now = Instant::now();
        panel.follow(HashSet::from([(1, Mode::Commits)]));
        let uncommitted = panel.request(&target(1), now);
        load(&mut panel, uncommitted, 1, 1);
        let wanted = [(target(1), Mode::Commits)];
        let commits = panel.request_followed(&wanted, now).map(|(_, g, r)| (g, r));
        load(&mut panel, commits, 1, 2);

        assert!(panel.model_in(1, Mode::Uncommitted, None).is_some());
        assert!(panel.model_in(1, Mode::Commits, None).is_some());
    }

    #[test]
    fn an_unfollowed_mode_s_model_goes_when_the_window_s_loads() {
        let mut panel = Panel::default();
        let now = Instant::now();
        panel.follow(HashSet::from([(1, Mode::Commits)]));
        let wanted = [(target(1), Mode::Commits)];
        let commits = panel.request_followed(&wanted, now).map(|(_, g, r)| (g, r));
        load(&mut panel, commits, 1, 1);

        panel.follow(HashSet::new());

        assert!(panel.model_in(1, Mode::Commits, None).is_none());
    }

    #[test]
    fn unfollowing_ignores_a_late_answer() {
        let mut panel = Panel::default();
        panel.follow(HashSet::from([(1, Mode::All)]));
        let wanted = [(target(1), Mode::All)];
        let (_, generation, request) = panel.request_followed(&wanted, Instant::now()).expect("a request");

        panel.follow(HashSet::new());
        panel.loaded(1, generation, &request, Ok(loaded(Some(Diff::default()), 1)));

        assert!(panel.model_in(1, Mode::All, None).is_none());
    }

    #[test]
    fn each_mode_waits_its_own_pacing() {
        let mut panel = Panel::default();
        let now = Instant::now();
        let first = panel.request_in(&target(1), Mode::Uncommitted, OPEN_EVERY, now);
        load(&mut panel, first, 1, 1);
        panel.follow(HashSet::from([(1, Mode::Commits)]));

        assert!(panel.request_in(&target(1), Mode::Uncommitted, OPEN_EVERY, now).is_none());
        assert!(panel.request_in(&target(1), Mode::Commits, OPEN_EVERY, now).is_some());
    }

    #[test]
    fn followed_pairs_take_turns() {
        let mut panel = Panel::default();
        let now = Instant::now();
        panel.follow(HashSet::from([(1, Mode::Uncommitted), (2, Mode::Uncommitted)]));
        let wanted = [(target(1), Mode::Uncommitted), (target(2), Mode::Uncommitted)];
        let mut asked = Vec::new();
        for turn in 0..4_u32 {
            let at = now + OPEN_EVERY * turn;
            let (workspace, generation, request) = panel.request_followed(&wanted, at).expect("a request");
            panel.loaded(workspace, generation, &request, Ok(loaded(Some(Diff::default()), 1)));
            asked.push(workspace);
        }

        assert_eq!(asked, [1, 2, 1, 2]);
    }

    #[test]
    fn a_sweep_without_expanding_asks_for_counts_only() {
        let mut panel = Panel { scope: Scope::Project, ..Panel::default() };
        let now = Instant::now();
        let (_, generation, request) = panel.request_sweep(&[(target(1), false)], now, false).expect("a sweep");
        panel.loaded(1, generation, &request, Ok(loaded(Some(diff(vec![file("a", 1, 1)])), 1)));

        let (_, _, request) = panel.request_sweep(&[(target(1), false)], now + CLOSED_EVERY, false).expect("again");

        assert_eq!(request.expanded, Some(Vec::new()));
    }
}

mod viewed {
    use super::*;

    #[test]
    fn a_mark_set_with_an_old_digest_does_not_mark_the_new_file() {
        let mut panel = Panel::default();
        panel.set_viewed(1, "a", Some(1));

        assert!(!panel.viewed(1, &file("a", 2, 1)));
        assert!(panel.viewed(1, &file("a", 1, 1)));
    }

    #[test]
    fn clearing_a_mark_lets_the_file_fold_by_its_rules() {
        let mut panel = Panel::default();
        let marked = file("a", 1, 1);
        panel.set_viewed(1, "a", Some(1));
        assert!(panel.folded(1, &diff(vec![marked.clone()]), &marked));

        panel.set_viewed(1, "a", None);

        assert!(!panel.folded(1, &diff(vec![marked.clone()]), &marked));
    }

    #[test]
    fn forgetting_a_workspace_drops_every_mode() {
        let mut panel = Panel::default();
        let now = Instant::now();
        panel.follow(HashSet::from([(1, Mode::Commits)]));
        let first = panel.request_in(&target(1), Mode::Uncommitted, OPEN_EVERY, now);
        let (generation, request) = first.expect("a request");
        panel.loaded(1, generation, &request, Ok(loaded(Some(Diff::default()), 1)));

        panel.forget(1);

        assert!(panel.model_in(1, Mode::Uncommitted, None).is_none());
        assert!(panel.models.is_empty() && panel.requested.is_empty());
    }

    #[test]
    fn discarding_a_file_forgets_it_and_the_pacing_of_its_workspace() {
        let mut panel = Panel::default();
        let now = Instant::now();
        let (generation, request) = panel.request_in(&target(1), Mode::Uncommitted, OPEN_EVERY, now).expect("request");
        panel.loaded(1, generation, &request, Ok(loaded(Some(Diff::default()), 1)));
        let (a, b) = (file("a", 1, 1), file("b", 2, 1));
        panel.set_viewed(1, "a", Some(1));
        panel.set_viewed(1, "b", Some(2));
        panel.set_fold(1, &a, false);
        assert!(panel.folds().contains_key(&FileKey { workspace: 1, path: "a".into() }));
        panel.gaps.insert((FileKey { workspace: 1, path: "a".into() }, 1, 1), Arc::new(Vec::new()));
        panel.gaps.insert((FileKey { workspace: 1, path: "b".into() }, 2, 1), Arc::new(Vec::new()));
        assert!(panel.request_in(&target(1), Mode::Uncommitted, OPEN_EVERY, now).is_none());

        panel.discarded(1, "a");

        assert!(!panel.folds().contains_key(&FileKey { workspace: 1, path: "a".into() }));
        assert!(!panel.viewed(1, &a) && panel.viewed(1, &b));
        assert_eq!(panel.gaps.len(), 1);
        assert!(panel.request_in(&target(1), Mode::Uncommitted, OPEN_EVERY, now).is_some());
    }

    #[test]
    fn a_load_in_flight_during_a_discard_does_not_bring_the_file_back() {
        let mut panel = Panel::default();
        let now = Instant::now();
        let (generation, request) = panel.request_in(&target(1), Mode::Uncommitted, OPEN_EVERY, now).expect("request");
        panel.loaded(1, generation, &request, Ok(loaded(Some(diff(vec![file("a", 1, 1)])), 1)));
        let later = now + OPEN_EVERY;
        let (stale, request) = panel.request_in(&target(1), Mode::Uncommitted, OPEN_EVERY, later).expect("request");

        panel.discarded(1, "a");
        panel.loaded(1, stale, &request, Ok(loaded(Some(diff(vec![file("a", 1, 1), file("b", 2, 1)])), 2)));

        let model = panel.model_in(1, Mode::Uncommitted, None).expect("model");
        assert_eq!(model.diff().map(|d| d.files.len()), Some(1));
        let (fresh, request) = panel.request_in(&target(1), Mode::Uncommitted, OPEN_EVERY, later).expect("a new load");
        panel.loaded(1, fresh, &request, Ok(loaded(Some(diff(Vec::new())), 3)));
        let model = panel.model_in(1, Mode::Uncommitted, None).expect("model");
        assert_eq!(model.diff().map(|d| d.files.len()), Some(0));
    }
}

mod rescan {
    use super::*;

    #[test]
    fn keeps_the_viewed_mark_and_drops_the_pacing() {
        let mut panel = Panel::default();
        let now = Instant::now();
        let (generation, request) = panel.request(&target(1), now).expect("first");
        panel.loaded(1, generation, &request, Ok(loaded(Some(diff(vec![file("a.rs", 5, 3)])), 7)));
        panel.set_viewed(1, "a.rs", Some(5));
        assert!(panel.request(&target(1), now + Duration::from_millis(10)).is_none());
        panel.rescan(1);
        assert!(panel.request(&target(1), now + Duration::from_millis(10)).is_some());
        assert!(panel.viewed(1, &file("a.rs", 5, 3)));
    }

    #[test]
    fn ignores_a_load_that_started_before() {
        let mut panel = Panel::default();
        let now = Instant::now();
        let (generation, request) = panel.request(&target(1), now).expect("first");
        panel.rescan(1);
        panel.loaded(1, generation, &request, Ok(loaded(Some(diff(vec![file("a.rs", 5, 3)])), 7)));
        assert!(panel.models.is_empty());
    }
}
