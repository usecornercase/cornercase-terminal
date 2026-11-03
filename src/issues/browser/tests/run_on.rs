use super::*;
use crate::issues::browser::{AgentNote, There};
use crate::machines::run_on::{NEEDS_REMOTE, RunOn, Target};

fn target(name: &str, why: Option<&str>) -> Target {
    Target { name: name.into(), icon: '☁', link: 1, why: why.map(str::to_string) }
}

fn two_machines() -> RunOn {
    RunOn { targets: vec![target("vps", None), target("box", Some("unreachable"))], ..RunOn::default() }
}

fn reading(run_on: Option<RunOn>) -> Browser {
    let mut b = Browser { run_on, ..with_lists() };
    press(&mut b, KeyCode::Enter);
    b
}

fn row(b: &Browser, label: &str) -> Option<String> {
    b.detail_lines(ROOM)
        .into_iter()
        .find(|l| l.spans.first().is_some_and(|s| s.content.trim() == label))
        .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
}

fn rows(b: &Browser) -> Vec<String> {
    b.detail_lines(ROOM).into_iter().map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect()).collect()
}

fn pick(b: &mut Browser, name: &str) -> Action {
    let pos = button(b, "run on…");
    click(b, pos);
    type_text(b, name);
    press(b, KeyCode::Enter)
}

fn on_vps() -> Browser {
    let mut b = reading(Some(two_machines()));
    assert_eq!(pick(&mut b, "vps"), Action::RunOn(Some(0)));
    b
}

fn there(cloned: bool, agent: AgentNote, tracker: Option<&str>) -> There {
    let start_hint = "in its own worktree on vps, in ~/code/shop".into();
    There { start_hint, cloned, agent, command: None, tracker: tracker.map(str::to_string), checking: false }
}

mod without_machines {
    use super::*;

    #[test]
    fn the_detail_and_its_buttons_are_the_ones_from_before() {
        let b = reading(None);

        assert_eq!(row(&b, "run on"), None);
        let with_machines = rows(&reading(Some(two_machines())));
        let without_its_row: Vec<&String> = with_machines.iter().filter(|text| !text.starts_with("run on")).collect();
        assert_eq!(rows(&b).iter().collect::<Vec<_>>(), without_its_row);
        assert_eq!(b.labels(ROOM.buttons), ["start", "schedule", "start a flow", "agent…", "raw", "copy url", "back"]);
    }
}

mod the_row {
    use super::*;

    #[test]
    fn lists_this_computer_chosen_and_each_machine() {
        let b = reading(Some(two_machines()));

        assert_eq!(row(&b, "run on").as_deref(), Some("run on  ● this computer   ○ ☁ vps   ○ ☁ box · unreachable"));
        let shown = rows(&b);
        let at = shown.iter().position(|text| text.starts_with("run on")).expect("the row");
        assert!(shown[at + 1].starts_with("start "), "{shown:?}");
        assert!(b.labels(ROOM.buttons).contains(&"run on…"));
    }

    #[test]
    fn says_a_git_remote_is_needed() {
        let b = reading(Some(RunOn { blocked: Some(NEEDS_REMOTE), ..two_machines() }));

        assert_eq!(
            row(&b, "run on").as_deref(),
            Some("run on  ● this computer   ○ ☁ vps   ○ ☁ box — needs a git remote")
        );
    }

    #[test]
    fn leaves_the_prompt_to_the_machine_s_template() {
        let b = on_vps();

        assert_eq!(row(&b, "prompt").as_deref(), Some("prompt  from vps's template (the issue's URL by default)"));
    }

    #[test]
    fn says_what_the_machine_lacks_for_the_tracker() {
        let mut b = on_vps();
        b.there = Some(there(true, AgentNote::Ready, Some("vps has no Shortcut token for shop")));

        let shown = row(&b, "run on");

        assert_eq!(
            shown.as_deref(),
            Some("run on  ○ this computer   ● ☁ vps   ○ ☁ box · unreachable · vps has no Shortcut token for shop")
        );
    }
}

mod the_picker {
    use super::*;

    #[test]
    fn lists_this_computer_and_each_machine_with_why_it_cannot_run_it() {
        let mut b = reading(Some(two_machines()));
        let pos = button(&b, "run on…");
        click(&mut b, pos);

        let choices: Vec<(String, String, String)> =
            b.choices().into_iter().map(|c| (c.key, c.title, c.meta)).collect();

        assert_eq!(
            choices,
            [
                ("this computer".into(), "▭".into(), String::new()),
                ("vps".into(), "☁".into(), String::new()),
                ("box".into(), "☁".into(), "unreachable".into()),
            ]
        );
        assert_eq!(b.picker.as_ref().map(|p| (p.pick, p.search.selected())), Some((Pick::Machine, 0)));
    }

    #[test]
    fn opens_on_the_chosen_machine() {
        let mut b = on_vps();
        let pos = button(&b, "run on…");
        click(&mut b, pos);

        assert_eq!(b.picker.as_ref().map(|p| p.search.selected()), Some(1));
    }

    #[test]
    fn chooses_a_linked_machine() {
        let b = on_vps();

        assert_eq!(b.picker.as_ref().map(|p| p.pick), None);
        assert_eq!(b.machine().map(|target| target.name.as_str()), Some("vps"));
        assert_eq!(row(&b, "start").as_deref(), Some("start   on vps"));
    }

    #[test]
    fn keeps_a_dimmed_machine_out_of_reach() {
        let mut b = reading(Some(two_machines()));

        assert_eq!(pick(&mut b, "box"), Action::None);

        assert_eq!(b.picker.as_ref().map(|p| p.pick), Some(Pick::Machine));
        assert_eq!(b.machine(), None);
    }

    #[test]
    fn keeps_every_machine_out_of_reach_without_a_git_remote() {
        let mut b = reading(Some(RunOn { blocked: Some(NEEDS_REMOTE), ..two_machines() }));

        assert_eq!(pick(&mut b, "vps"), Action::None);

        assert_eq!(b.machine(), None);
    }

    #[test]
    fn goes_back_to_this_computer_and_forgets_what_the_machine_said() {
        let mut b = on_vps();
        b.there = Some(there(true, AgentNote::SignedOut, None));

        assert_eq!(pick(&mut b, "this"), Action::RunOn(None));

        assert_eq!((b.machine(), &b.there), (None, &None));
        assert_eq!(row(&b, "start"), row(&reading(None), "start"));
    }
}

mod with_a_machine {
    use super::*;

    #[test]
    fn hides_schedule_in_the_detail_and_the_list() {
        let mut b = on_vps();

        assert!(!b.labels(ROOM.buttons).contains(&"schedule"));
        press(&mut b, KeyCode::Esc);
        assert!(matches!(b.screen, Screen::List));
        assert!(!b.labels(ROOM.buttons).contains(&"schedule"));
    }

    #[test]
    fn says_where_it_starts_there() {
        let mut b = on_vps();
        b.there = Some(there(true, AgentNote::Ready, None));

        assert_eq!(row(&b, "start").as_deref(), Some("start   in its own worktree on vps, in ~/code/shop"));
    }

    #[rstest::rstest]
    #[case::not_installed(AgentNote::NotInstalled, "not installed on vps")]
    #[case::not_logged_in(AgentNote::SignedOut, "not logged in on vps")]
    fn says_what_the_agent_lacks_and_offers_a_login_tab(#[case] agent: AgentNote, #[case] said: &str) {
        let mut b = on_vps();
        b.there = Some(there(true, agent, None));

        let shown = row(&b, "agent").expect("an agent row");

        assert!(shown.ends_with(&format!("  {said}")), "{shown}");
        let pos = button(&b, "open login tab");
        assert_eq!(click(&mut b, pos), Action::OpenLoginTab);
    }

    #[rstest::rstest]
    #[case::its_own_command(Some("/opt/claude/bin/claude"), "/opt/claude/bin/claude  ")]
    #[case::the_plain_name(Some("claude"), "")]
    #[case::nothing_known(None, "")]
    fn shows_the_machine_s_command_and_never_this_computer_s(#[case] command: Option<&str>, #[case] shown: &str) {
        let mut b = on_vps();
        let here = AgentStart { mode: Some("plan".into()), command: "here/claude".into() };
        b.agents.starts.insert("claude".into(), here);
        b.there = Some(There { command: command.map(str::to_string), ..there(true, AgentNote::SignedOut, None) });

        let row = row(&b, "agent").expect("an agent row");

        assert!(row.starts_with(&format!("agent   claude  {shown}")), "{row}");
        assert!(!row.contains("here/claude") && !row.contains("plan"), "{row}");
    }

    #[test]
    fn offers_no_login_tab_with_a_ready_agent_or_before_the_project_is_there() {
        let mut b = on_vps();
        b.there = Some(there(true, AgentNote::Ready, None));
        assert!(!b.labels(ROOM.buttons).contains(&"open login tab"));

        b.there = Some(there(false, AgentNote::SignedOut, None));
        assert!(!b.labels(ROOM.buttons).contains(&"open login tab"));
    }

    #[test]
    fn names_the_machine_s_accounts() {
        let claude = AgentAccounts {
            names: vec!["work".into()],
            labels: vec!["Main".into(), "work".into()],
            active: "work".into(),
        };
        let accounts = Accounts { agents: HashMap::from([("claude".to_string(), claude)]), chosen: None };
        let mut b = Browser { accounts: Some(accounts), ..on_vps() };
        b.there = Some(there(true, AgentNote::Ready, None));

        assert_eq!(row(&b, "account").as_deref(), Some("account work  (in use)  (vps's accounts)"));
    }

    #[test]
    fn asks_to_clone_the_project_when_it_is_not_there() {
        let mut b = on_vps();
        b.there = Some(there(false, AgentNote::Ready, None));

        let pos = button(&b, "start");
        let started = click(&mut b, pos);

        assert!(matches!(&started, Action::CloneThere(issue) if issue.key == "sc-2"), "{started:?}");
        let pos = button(&b, "start a flow");
        assert!(matches!(click(&mut b, pos), Action::CloneThere(_)));
    }

    #[test]
    fn starts_as_before_once_the_project_is_there() {
        let mut b = on_vps();
        b.there = Some(there(true, AgentNote::Ready, None));

        let pos = button(&b, "start");

        assert!(matches!(click(&mut b, pos), Action::Name(issue, _, _) if issue.key == "sc-2"));
    }
}

mod a_machine_that_is_gone {
    use super::*;

    fn gone() -> Browser {
        let mut b = on_vps();
        b.run_on = RunOn::kept(None, b.run_on.as_ref());
        b
    }

    #[test]
    fn starts_nothing_and_says_why() {
        let mut b = gone();

        for label in ["start", "start a flow"] {
            let pos = button(&b, label);
            assert_eq!(click(&mut b, pos), Action::None);
            assert_eq!(b.error.as_deref(), Some("vps is no longer listed"));
        }
        assert!(!b.labels(ROOM.buttons).contains(&"schedule"));
        assert!(row(&b, "run on").is_some_and(|row| row.ends_with("○ this computer · vps is no longer listed")));
    }

    #[test]
    fn is_left_once_this_computer_is_chosen() {
        let mut b = gone();

        assert_eq!(pick(&mut b, "this"), Action::RunOn(None));

        assert_eq!(b.run_on.as_ref().and_then(|run_on| run_on.lost.as_deref()), None);
        assert!(b.labels(ROOM.buttons).contains(&"schedule"));
    }

    #[test]
    fn opens_the_picker_with_no_choice_marked() {
        let mut b = gone();
        let pos = button(&b, "run on…");
        click(&mut b, pos);

        assert_eq!(press(&mut b, KeyCode::Enter), Action::None);
        assert!(b.run_on.as_ref().is_some_and(|run_on| run_on.lost.is_some()));
        assert_eq!(press(&mut b, KeyCode::Down), Action::None);
        assert_eq!(press(&mut b, KeyCode::Enter), Action::RunOn(None));
    }

    #[test]
    fn chooses_nothing_on_enter_or_choose_until_a_row_is_marked() {
        let mut b = gone();
        let pos = button(&b, "run on…");
        click(&mut b, pos);

        assert_eq!(press(&mut b, KeyCode::Enter), Action::None);
        assert_eq!(press(&mut b, KeyCode::Enter), Action::None);
        let choose = button(&b, "choose");
        assert_eq!(click(&mut b, choose), Action::None);

        assert!(b.picker.is_some());
        assert!(b.run_on.as_ref().is_some_and(|run_on| run_on.lost.is_some()));
    }

    #[test]
    fn chooses_a_row_pasted_as_one_typed() {
        let mut b = on_vps();
        let moved = RunOn { targets: vec![Target { link: 2, ..target("vps", None) }], ..RunOn::default() };
        b.run_on = RunOn::kept(Some(moved), b.run_on.as_ref());
        let pos = button(&b, "run on…");
        click(&mut b, pos);

        b.paste("vps");

        assert_eq!(press(&mut b, KeyCode::Enter), Action::RunOn(Some(0)));
    }
}

mod the_focused_issue {
    use super::*;

    #[test]
    fn is_lent_not_copied() {
        let mut b = reading(None);
        let Screen::Detail { issue, .. } = &b.screen else { panic!("not reading an issue") };
        assert!(b.focused().is_some_and(|focused| std::ptr::eq(focused, &raw const **issue)));

        press(&mut b, KeyCode::Esc);

        let listed = b.shown()[0];
        assert!(b.focused().is_some_and(|focused| std::ptr::eq(focused, listed)));
    }
}
