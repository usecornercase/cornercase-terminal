use super::*;
use crate::changes::diff;

fn entries(names: &[(&str, bool)]) -> Vec<Entry> {
    names.iter().map(|(name, dir)| Entry { name: (*name).to_string(), dir: *dir }).collect()
}

fn panel_with_listings() -> Panel {
    let mut panel = Panel::default();
    let now = Instant::now();
    let (generation, _) = panel.list(1, Path::new("/repo"), now).expect("a first listing");
    panel.listed(1, generation, vec![(String::new(), Some(entries(&[("src", true), ("README.md", false)])))]);
    panel.listed(1, 0, vec![("src".into(), Some(entries(&[("ui", true), ("main.rs", false)])))]);
    panel
}

fn paths(rows: &[Row]) -> Vec<(&str, u16)> {
    rows.iter().map(|r| (r.path.as_str(), r.depth)).collect()
}

mod tree {
    use super::*;

    #[test]
    fn shows_the_root_until_a_folder_opens() {
        let mut panel = panel_with_listings();
        assert_eq!(paths(&panel.rows(1)), [("src", 0), ("README.md", 0)]);
        panel.toggle(1, "src");
        assert_eq!(paths(&panel.rows(1)), [("src", 0), ("src/ui", 1), ("src/main.rs", 1), ("README.md", 0)]);
        panel.toggle(1, "src");
        assert_eq!(paths(&panel.rows(1)).len(), 2);
    }

    #[test]
    fn an_opened_folder_is_listed_at_once() {
        let mut panel = panel_with_listings();
        let now = Instant::now();
        assert!(panel.list(1, Path::new("/repo"), now).is_none());
        panel.toggle(1, "src/ui");
        let (_, list) = panel.list(1, Path::new("/repo"), now).expect("the new folder is listed");
        assert_eq!(list.folders, ["", "src/ui"]);
    }

    #[test]
    fn lists_again_after_a_while() {
        let mut panel = Panel::default();
        let now = Instant::now();
        let (generation, _) = panel.list(1, Path::new("/repo"), now).expect("listing");
        assert!(panel.list(1, Path::new("/repo"), now).is_none(), "one listing at a time");
        panel.listed(1, generation, vec![(String::new(), Some(Vec::new()))]);
        assert!(panel.list(1, Path::new("/repo"), now + LIST_EVERY / 2).is_none());
        assert!(panel.list(1, Path::new("/repo"), now + LIST_EVERY).is_some());
    }

    #[test]
    fn a_root_that_went_away_shows_no_files() {
        let mut panel = panel_with_listings();
        panel.listed(1, 0, vec![(String::new(), None)]);
        assert_eq!((panel.rows(1).len(), panel.listed_root(1)), (0, true));
    }

    #[test]
    fn a_folder_that_went_away_closes() {
        let mut panel = panel_with_listings();
        panel.toggle(1, "src");
        panel.listed(1, 0, vec![("src".into(), None)]);
        assert_eq!(paths(&panel.rows(1)), [("src", 0), ("README.md", 0)]);
        assert!(!panel.rows(1)[0].open);
    }
}

mod viewer {
    use super::*;

    fn content(lines: &[&str]) -> Content {
        let lines = lines.iter().map(|l| (*l).to_string()).collect();
        Content { stamp: Stamp::default(), language: None, body: disk::Body::Text { lines, styles: None } }
    }

    #[test]
    fn reads_the_open_file_and_drops_answers_for_another() {
        let mut panel = Panel::default();
        let now = Instant::now();
        panel.open_file(1, "a.rs");
        let (first, load) = panel.load(1, Path::new("/repo"), now).expect("a read");
        assert_eq!(load.file, Path::new("/repo/a.rs"));
        panel.open_file(1, "b.rs");
        panel.read(1, first, "a.rs", Some(content(&["old"])), true);
        assert!(panel.viewer(1).and_then(|v| v.content.as_ref()).is_none());
        let (second, _) = panel.load(1, Path::new("/repo"), now).expect("b is read");
        panel.read(1, second, "b.rs", Some(content(&["new"])), true);
        assert_eq!(
            panel.viewer(1).and_then(|v| v.content.as_ref()).map(|c| c.lines().to_vec()),
            Some(vec!["new".to_string()])
        );
    }

    #[test]
    fn checks_the_file_again_every_second() {
        let mut panel = Panel::default();
        let now = Instant::now();
        panel.open_file(1, "a.rs");
        let (generation, _) = panel.load(1, Path::new("/repo"), now).expect("a read");
        panel.read(1, generation, "a.rs", Some(content(&["a"])), false);
        assert!(panel.load(1, Path::new("/repo"), now + CHECK_EVERY).is_none(), "the highlighting is still on its way");
        panel.read(1, generation, "a.rs", None, true);
        assert!(panel.load(1, Path::new("/repo"), now + CHECK_EVERY / 2).is_none());
        let (_, load) = panel.load(1, Path::new("/repo"), now + CHECK_EVERY).expect("checked again");
        assert!(load.previous.is_some());
    }

    #[test]
    fn going_back_keeps_the_last_file() {
        let mut panel = Panel::default();
        panel.open_file(1, "src/a.rs");
        panel.close_file(1);
        assert_eq!((panel.viewer(1).is_none(), panel.last(1)), (true, Some("src/a.rs")));
    }
}

mod searching {
    use super::*;

    fn indexed(panel: &mut Panel, mode: Mode, query: &str) {
        if mode == Mode::Name {
            panel.toggle_mode(1);
        }
        panel.query_mut(1).text = query.into();
        let (generation, _) = panel.index_request(1, Path::new("/repo"), Instant::now()).expect("indexing");
        panel.indexed(1, generation, vec!["src/main.rs".into(), "README.md".into()]);
    }

    #[test]
    fn indexes_once_the_search_bar_is_used() {
        let mut panel = Panel::default();
        assert!(panel.index_request(1, Path::new("/repo"), Instant::now()).is_none());
        panel.query_mut(1).focused = true;
        assert!(panel.index_request(1, Path::new("/repo"), Instant::now()).is_some());
    }

    #[test]
    fn the_focused_bar_takes_the_keys() {
        let mut panel = Panel::default();
        panel.query_mut(1).focused = true;
        assert!(panel.typing(1));
        panel.unfocus();
        assert!(!panel.typing(1));
    }

    #[test]
    fn the_file_icon_switches_to_names_and_keeps_the_query() {
        let mut panel = Panel::default();
        panel.query_mut(1).text = "main".into();
        panel.toggle_mode(1);
        assert_eq!((panel.mode(1), panel.searched(1), panel.typing(1)), (Mode::Name, Some("main"), true));
        panel.toggle_mode(1);
        assert_eq!(panel.mode(1), Mode::Text);
    }

    #[test]
    fn looks_for_names_again_when_the_query_changes() {
        let mut panel = Panel::default();
        indexed(&mut panel, Mode::Name, "main");
        let now = Instant::now();
        let (generation, search) = panel.names_request(1, now).expect("a search");
        panel.named(1, generation, search, Vec::new());
        assert!(panel.names_request(1, now).is_none());
        panel.query_mut(1).text = "read".into();
        assert_eq!(panel.names_request(1, now).map(|(_, s)| s.query), Some("read".to_string()));
    }

    #[test]
    fn a_new_text_search_cancels_the_running_one() {
        let mut panel = Panel::default();
        indexed(&mut panel, Mode::Text, "fn");
        let now = Instant::now();
        let (_, _, first) = panel.grep_request(1, now).expect("a search");
        assert!(panel.grep_request(1, now).is_none(), "the same query is already running");
        panel.query_mut(1).text = "fn main".into();
        let (generation, search, _) = panel.grep_request(1, now).expect("a new search");
        assert!(first.load(Ordering::Relaxed));
        panel.grepped(1, generation, search, search::Text::default());
        assert_eq!(panel.found_text(1).map(|(q, _)| q), Some("fn main"));
    }

    #[test]
    fn opening_a_line_selects_it_and_remembers_the_text() {
        let mut panel = Panel::default();
        panel.open_at(1, "src/main.rs", (40, 40), Some("run".into()));
        let viewer = panel.viewer(1).expect("a viewer");
        assert_eq!((viewer.selection, viewer.scroll, viewer.find.as_deref()), (Some((40, 40)), 36, Some("run")));
    }
}

mod marks {
    use super::*;

    const PATCH: &str = "diff --git a/src/a.rs b/src/a.rs
--- a/src/a.rs
+++ b/src/a.rs
@@ -1,7 +1,6 @@
-gone at the top
 one
-two
+TWO
 three
+new
 four
-five
 six
";

    fn file() -> Changed {
        diff::parse(PATCH).remove(0)
    }

    #[test]
    fn marks_added_modified_and_deleted_lines() {
        let gutter = gutter(&file(), 6);
        let marks: Vec<Option<Mark>> = (1..=6).map(|n| gutter.mark(n)).collect();
        assert_eq!(
            marks,
            [Some(Mark::DeletedAbove), Some(Mark::Modified), None, Some(Mark::Added), Some(Mark::Deleted), None]
        );
    }

    #[test]
    fn keeps_the_removed_lines_where_they_were() {
        let gutter = gutter(&file(), 6);
        let removed: Vec<(u32, Vec<&str>)> =
            gutter.removed.iter().map(|(k, v)| (*k, v.iter().map(String::as_str).collect())).collect();
        assert_eq!(removed, [(1, vec!["gone at the top"]), (2, vec!["two"]), (6, vec!["five"])]);
        assert_eq!((gutter.block(2), gutter.block(4), gutter.block(5)), (Some(2), None, Some(6)));
    }

    #[test]
    fn a_new_file_is_all_added() {
        let file = diff::untracked("new.rs", 4, Some(b"a\nb\n"));
        let gutter = gutter(&file, 2);
        assert_eq!((gutter.mark(1), gutter.mark(2), gutter.mark(3)), (Some(Mark::Added), Some(Mark::Added), None));
    }

    #[test]
    fn folders_show_what_changed_inside() {
        let added = diff::untracked("src/ui/new.rs", 1, Some(b"a"));
        let diff = Diff { files: vec![Arc::new(added), Arc::new(file())] };
        let marks = statuses(&diff);
        let mark = |path: &str| marks.get(path).copied();
        assert_eq!(
            [mark("src/a.rs"), mark("src/ui/new.rs"), mark("src"), mark("src/ui"), mark("README.md")],
            [Some(Status::Modified), Some(Status::Untracked), Some(Status::Modified), Some(Status::Added), None]
        );
    }

    #[test]
    fn unfolded_removals_take_rows_before_their_line() {
        let gutter = gutter(&file(), 6);
        let unfolded = HashSet::from([2, 6]);
        let lines = Lines::new(6, Some(&gutter), &unfolded);
        let rows: Vec<Line> = (0..lines.len()).filter_map(|r| lines.at(r)).collect();
        assert_eq!(
            rows,
            [
                Line::Code(1),
                Line::Removed(2, 0),
                Line::Code(2),
                Line::Code(3),
                Line::Code(4),
                Line::Code(5),
                Line::Removed(6, 0),
                Line::Code(6)
            ]
        );
        assert_eq!((lines.row_of(2), lines.row_of(6), lines.removed(6, 0)), (2, 7, Some("five")));
    }
}
