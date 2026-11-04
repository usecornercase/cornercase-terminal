use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::super::{Action, Page, PluginRow, Row, Settings, Status};
use super::Goal;
use crate::config::Config;
use crate::issues::Source;
use crate::messenger::Via;

fn settings() -> Settings {
    let tokens = Source::REMOTE.into_iter().map(|source| (source, Status::Missing)).collect();
    Settings::new(Config::default(), Some(PathBuf::from("/home/ana")), tokens, Page::default())
}

fn paired() -> Settings {
    let mut s = settings();
    s.messenger_mut(Via::Discord).bot = Some("@bot".into());
    s.messenger_mut(Via::Discord).chat = Some("Ana".into());
    s
}

fn titles(s: &Settings, query: &str) -> Vec<String> {
    s.search(query).into_iter().map(|hit| hit.title).collect()
}

fn press(s: &mut Settings, code: KeyCode) -> Action {
    s.key(KeyEvent::new(code, KeyModifiers::NONE), 10)
}

fn typed(s: &mut Settings, text: &str) {
    text.chars().for_each(|c| {
        press(s, KeyCode::Char(c));
    });
}

mod matching {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::every_word_by_its_start("def ag", "Agents › default agent")]
    #[case::the_case_does_not_matter("DEFAULT", "Agents › default agent")]
    #[case::a_section_name("telegram", "Remote › Telegram › bot")]
    #[case::a_value("per tracker", "Worktrees › issue branches")]
    #[case::a_note("ram", "UI › memory")]
    fn finds(#[case] query: &str, #[case] title: &str) {
        let s = settings();
        assert!(titles(&s, query).iter().any(|found| found == title), "{:?}", titles(&s, query));
    }

    #[rstest]
    #[case::the_middle_of_a_word("ose")]
    #[case::a_fuzzy_spelling("dflt")]
    #[case::one_word_of_two_missing("default zebra")]
    #[case::only_spaces("   ")]
    fn finds_nothing(#[case] query: &str) {
        assert_eq!(titles(&settings(), query), Vec::<String>::new());
    }

    #[test]
    fn reaches_every_tab() {
        let s = settings();
        let pages: Vec<Page> = Page::ALL.iter().filter(|page| !s.search(page.name()).is_empty()).copied().collect();
        assert_eq!(pages, Page::ALL);
    }

    #[test]
    fn a_plugin_is_a_row_of_its_tab() {
        let mut s = settings();
        let plugin = PluginRow {
            id: "acme/deploy".into(),
            label: "deploy".into(),
            on: false,
            note: String::new(),
            broken: false,
        };
        s.plugins.push(plugin);
        assert_eq!(titles(&s, "deploy"), ["Plugins › deploy"]);
    }

    #[test]
    fn the_flows_tab_is_a_result_and_a_flow_is_not() {
        let hits = settings().search("flows");
        assert_eq!(hits[0].goal, Goal::Page(Page::Flows));
    }
}

mod ranking {
    use super::*;

    #[test]
    fn a_label_comes_before_a_section_and_a_section_before_a_note() {
        let s = settings();
        let found = titles(&s, "agents");
        let at = |title: &str| found.iter().position(|t| t == title).unwrap_or(usize::MAX);
        assert!(at("Agents") < at("Agents › default agent"), "{found:?}");
        assert!(at("UI › agents section") < at("Agents › default agent"), "{found:?}");
        assert!(at("Agents › default agent") < at("Worktrees › pull requests").min(found.len()), "{found:?}");
    }

    #[test]
    fn ties_keep_the_order_of_the_tabs() {
        let pages: Vec<Page> = settings().search("hidden").into_iter().map(|hit| hit.page).collect();
        assert_eq!(pages.first(), Some(&Page::Worktrees), "{pages:?}");
        assert_eq!(pages.last(), Some(&Page::Ui), "{pages:?}");
    }

    #[test]
    fn a_label_two_rows_share_names_its_section() {
        let found = titles(&settings(), "bot");
        assert!(found.contains(&"Remote › Telegram › bot".to_string()), "{found:?}");
        assert!(found.contains(&"Remote › Discord › bot".to_string()), "{found:?}");
    }
}

mod hidden_rows {
    use super::*;

    #[test]
    fn are_found_muted_with_their_reason_while_their_parent_keeps_them_away() {
        let s = paired();
        assert!(!s.every_row().contains(&Row::ForumClose(Via::Discord)));
        let hit = s.search("closed workspaces").remove(0);
        assert!(hit.hidden());
        assert_eq!((hit.value.as_str(), hit.note.as_str()), ("", "shows once forum mode is on"));
    }

    #[test]
    fn match_what_they_show_and_not_the_note_they_hide() {
        let s = paired();
        let found = |query: &str| s.search(query).iter().any(|hit| hit.label == "closed workspaces");
        assert_eq!((found("once forum"), found("archive")), (true, false));
    }

    #[test]
    fn stop_being_hidden_once_their_row_shows() {
        let mut s = paired();
        s.config.discord_forum = true;
        assert_eq!(s.search("closed workspaces").remove(0).goal, Goal::Row(Row::ForumClose(Via::Discord)));
    }

    #[test]
    fn a_kept_result_lands_on_the_parent_once_its_row_no_longer_shows() {
        let mut s = paired();

        s.land_on_current(Goal::Row(Row::ForumClose(Via::Discord)));

        assert_eq!((s.page, s.rows()[s.cursor].clone()), (Page::Remote, Row::Forum(Via::Discord)));
        assert_eq!(s.notice.as_deref(), Some("closed workspaces shows once forum mode is on"));
    }

    #[test]
    fn a_kept_result_lands_on_the_row_once_it_shows() {
        let mut hidden = paired();
        let goal = hidden.search("closed workspaces").remove(0).goal;
        hidden.config.discord_forum = true;

        hidden.land_on_current(goal);

        assert_eq!(hidden.rows()[hidden.cursor], Row::ForumClose(Via::Discord));
        assert_eq!(hidden.notice, None);
    }

    #[test]
    fn land_on_the_nearest_row_that_shows_and_change_nothing() {
        let mut s = paired();
        let config = s.config.clone();
        typed(&mut s, "closed workspaces");
        assert_eq!(press(&mut s, KeyCode::Enter), Action::None);
        assert_eq!((s.page, s.rows()[s.cursor].clone()), (Page::Remote, Row::Forum(Via::Discord)));
        assert_eq!(s.notice.as_deref(), Some("closed workspaces shows once forum mode is on"));
        assert_eq!(s.config, config);
    }

    #[test]
    fn walk_up_past_a_parent_that_is_hidden_too() {
        let mut s = settings();
        typed(&mut s, "closed workspaces");
        press(&mut s, KeyCode::Enter);
        assert_eq!(s.rows()[s.cursor], Row::Bot(Via::Discord));
    }
}

mod the_field {
    use super::*;

    #[test]
    fn takes_what_is_typed_when_settings_open() {
        let mut s = settings();
        typed(&mut s, "sound");
        assert_eq!(s.find.query(), "sound");
    }

    #[test]
    fn leaves_the_list_its_keys_while_empty() {
        let mut s = settings();
        press(&mut s, KeyCode::Down);
        assert_eq!(s.cursor, 1);
        press(&mut s, KeyCode::Tab);
        assert_eq!(s.page, Page::Agents);
        assert_eq!(press(&mut s, KeyCode::Esc), Action::Close);
    }

    #[test]
    fn esc_clears_it_first_then_closes() {
        let mut s = settings();
        typed(&mut s, "sound");
        assert_eq!(press(&mut s, KeyCode::Esc), Action::None);
        assert_eq!((s.find.query().as_str(), press(&mut s, KeyCode::Esc)), ("", Action::Close));
    }

    #[test]
    fn arrows_pick_a_result_and_enter_lands_on_its_tab_and_row() {
        let mut s = settings();
        typed(&mut s, "sound");
        press(&mut s, KeyCode::Down);
        let wanted = s.hits().remove(1);
        press(&mut s, KeyCode::Enter);
        let Goal::Row(row) = wanted.goal else { panic!("not a row") };
        assert_eq!((s.page, s.rows()[s.cursor].clone(), s.find.searching()), (row.page(), row, false));
    }

    #[test]
    fn enter_on_a_tab_result_opens_the_tab() {
        let mut s = settings();
        typed(&mut s, "plugins");
        press(&mut s, KeyCode::Enter);
        assert_eq!(s.page, Page::Plugins);
    }

    #[test]
    fn enter_with_no_results_stays_put() {
        let mut s = settings();
        typed(&mut s, "zebra");
        assert_eq!(press(&mut s, KeyCode::Enter), Action::None);
        assert_eq!((s.page, s.find.query().as_str()), (Page::Worktrees, "zebra"));
    }

    #[test]
    fn a_key_being_captured_never_reaches_it() {
        let mut s = settings();
        s.open_page(Page::Ui);
        s.capturing = true;
        press(&mut s, KeyCode::F(12));
        assert_eq!((s.config.prefix_key.as_str(), s.find.searching()), ("f12", false));
        s.capturing = true;
        let key = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::ALT);
        s.key(key, 10);
        assert_eq!((s.config.prefix_key.as_str(), s.find.searching()), ("alt+k", false));
    }

    #[test]
    fn backspace_clearing_it_does_not_go_on_to_remove_a_row() {
        let mut s = settings();
        s.open_page(Page::Issues);
        s.tokens = vec![(Source::Linear, Status::Saved(None))];
        typed(&mut s, "x");
        press(&mut s, KeyCode::Backspace);
        assert_eq!(press(&mut s, KeyCode::Backspace), Action::None);
        s.select(0);
        assert_eq!(press(&mut s, KeyCode::Backspace), Action::RemoveToken(Source::Linear));
    }

    #[test]
    fn switching_tabs_forgets_the_query() {
        let mut s = settings();
        typed(&mut s, "sound");
        s.open_page(Page::Agents);
        assert!(!s.find.searching());
    }
}

mod for_clients {
    use super::*;

    #[test]
    fn names_each_result_s_key() {
        let keyed = |query: &str| settings().found(query).remove(0).key;
        assert_eq!(keyed("default agent").as_deref(), Some("agent"));
        assert_eq!(keyed("linear api").as_deref(), Some("linear"));
    }

    #[test]
    fn says_a_tab_has_no_key_and_a_hidden_row_is_hidden() {
        let s = paired();
        assert_eq!(s.found("flows")[0].key, None);
        let hidden = s.found("closed workspaces").remove(0);
        assert_eq!((hidden.key.as_deref(), hidden.hidden), (Some("discord_forum_close"), true));
    }
}
