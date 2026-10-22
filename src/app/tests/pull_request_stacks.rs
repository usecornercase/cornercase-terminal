use super::pull_requests::{Repo, VIEW, notes, repo_on, row_text};
use super::*;
use crate::app::pull_requests::PullRequests;
use crate::test_util::{fake_gh, fixture};

const STACK: &str = fixture!("gh/2.101.0/stack.json");

const NONE: &str = fixture!("gh/2.101.0/stack-none.json");

const UNSUPPORTED: &str = fixture!("gh/2.101.0/stack-unsupported.json");

const MERGEABLE_NEXT: &str = fixture!("gh/2.101.0/pr-view-conflicting.json");

const SCRIPT: &str = "echo \"$1 $2 $(git rev-parse --abbrev-ref HEAD)\" >> \"$0.calls\"
case \"$1 $2\" in
'pr view')
  f=\"$0.pr-$(git rev-parse --abbrev-ref HEAD | tr / -)\"
  if [ -f \"$f\" ]; then cat \"$f\"; else echo 'no pull requests found for branch' >&2; exit 1; fi;;
'api graphql')
  if [ -f \"$0.stack-err\" ]; then cat \"$0.stack\" 2>/dev/null; cat \"$0.stack-err\" >&2; exit 1; fi
  cat \"$0.stack\";;
esac";

fn next_view() -> String {
    MERGEABLE_NEXT.replace("486", "483").replace("CONFLICTING", "MERGEABLE")
}

fn set(repo: &Repo, name: &str, contents: &str) {
    std::fs::write(repo.tmp.path().join(format!("gh.{name}")), contents).expect("an answer");
}

fn stacked() -> Repo {
    stacked_by(STACK)
}

fn stacked_by(stack: &str) -> Repo {
    let mut repo = repo_on(&["feat/x", "feat/y"], |dir| fake_gh(dir, SCRIPT), |_| {});
    set(&repo, "pr-feat-x", VIEW);
    set(&repo, "pr-feat-y", &next_view());
    set(&repo, "stack", stack);
    repo.ask("gh answers");
    repo
}

fn row_of(repo: &Repo, branch: &str) -> usize {
    let on = |w: &Workspace| git::checked_out(&w.path).as_deref() == Some(branch);
    repo.app.projects[0].workspaces.iter().position(on).expect("a workspace on the branch")
}

fn id_of(repo: &Repo, branch: &str) -> u64 {
    repo.app.projects[0].workspaces[row_of(repo, branch)].id
}

fn text(repo: &mut Repo, branch: &str) -> String {
    let row = row_of(repo, branch);
    row_text(&mut repo.app, WorkspaceRow::Workspace(row))
}

fn calls_of(repo: &Repo, what: &str) -> usize {
    repo.calls().iter().filter(|c| c.starts_with(what)).count()
}

fn graphql_calls(repo: &Repo) -> usize {
    calls_of(repo, "api graphql")
}

fn view_calls(repo: &Repo) -> usize {
    calls_of(repo, "pr view")
}

fn ask_in(repo: &mut Repo, minutes: u64) {
    repo.app.ask_pull_requests(Instant::now() + Duration::from_mins(minutes));
    pump_until(&mut repo.app, &repo.rx, "an answer", |a| a.git.pulls.asking.is_empty());
}

fn place_of(repo: &mut Repo, branch: &str, symbol: &str) -> Position {
    let row = row_rect(&repo.app, WorkspaceRow::Workspace(row_of(repo, branch)));
    let terminal = render(&mut repo.app, AREA);
    let buffer = terminal.backend().buffer();
    let x =
        (row.left()..row.right()).rev().find(|&x| buffer[(x, row.y)].symbol() == symbol).expect("the symbol is drawn");
    Position::new(x, row.y)
}

fn colour_at(repo: &mut Repo, at: Position) -> ratatui::style::Color {
    let terminal = render(&mut repo.app, AREA);
    terminal.backend().buffer()[(at.x, at.y)].fg
}

mod row {
    use ratatui::style::Color;

    use super::*;

    #[test]
    fn a_layer_shows_its_place_after_the_number() {
        let mut repo = stacked();

        assert!(text(&mut repo, "feat/x").contains("#482✗ 1/3"), "{}", text(&mut repo, "feat/x"));
        assert!(text(&mut repo, "feat/y").contains("#483✓ 2/3"));
    }

    #[test]
    fn a_failing_layer_below_turns_it_red() {
        let mut repo = stacked();
        let at = place_of(&mut repo, "feat/y", "/");

        assert_eq!(colour_at(&mut repo, at), Color::Red);
    }

    #[test]
    fn a_conflicting_layer_below_turns_it_orange() {
        let mut repo = stacked_by(&STACK.replacen("MERGEABLE", "CONFLICTING", 1));
        let at = place_of(&mut repo, "feat/y", "/");

        assert_eq!(colour_at(&mut repo, at), Color::Indexed(208));
    }

    #[test]
    fn the_bottom_layer_is_never_red() {
        let mut repo = stacked();
        let at = place_of(&mut repo, "feat/x", "/");

        assert_ne!(colour_at(&mut repo, at), Color::Red);
    }

    #[test]
    fn a_click_on_the_layer_opens_the_dialog() {
        let mut repo = stacked();
        let at = place_of(&mut repo, "feat/y", "/");

        click(&mut repo.app, at);

        let id = id_of(&repo, "feat/y");
        assert!(matches!(repo.app.overlay, Some(Overlay::PullRequest { workspace, .. }) if workspace == id));
    }

    #[test]
    fn in_the_tree_it_shows_too() {
        let mut repo = stacked();
        repo.app.config.sidebar = config::Sidebar::Tree.id().into();

        let terminal = render(&mut repo.app, AREA);
        let buffer = terminal.backend().buffer();
        let screen: Vec<String> =
            (0..AREA.height).map(|y| (0..AREA.width).map(|x| buffer[(x, y)].symbol().to_string()).collect()).collect();

        assert!(screen.iter().any(|row| row.contains("#483✓ 2/3")), "{screen:#?}");
    }
}

mod pacing {
    use ratatui::style::Color;

    use super::*;

    #[test]
    fn one_query_covers_every_workspace_in_the_stack() {
        let repo = stacked();

        assert_eq!((view_calls(&repo), graphql_calls(&repo)), (2, 1));
    }

    #[test]
    fn an_answer_for_one_layer_updates_the_others() {
        let mut repo = stacked();
        set(&repo, "stack", &STACK.replace("FAILURE", "SUCCESS"));
        let id = id_of(&repo, "feat/x");

        repo.app.open_pull_request(id);
        pump_until(&mut repo.app, &repo.rx, "the answer", |a| a.git.pulls.asking.is_empty());

        assert_eq!(view_calls(&repo), 3);
        repo.app.overlay = None;
        let at = place_of(&mut repo, "feat/y", "/");
        assert_ne!(colour_at(&mut repo, at), Color::Red);
    }

    #[test]
    fn a_pull_request_in_a_stack_is_asked_at_its_pace() {
        let mut repo = stacked();

        ask_in(&mut repo, 2);

        assert_eq!(graphql_calls(&repo), 2);
    }

    #[test]
    fn one_in_no_stack_is_asked_every_five_minutes() {
        let mut repo = stacked_by(NONE);
        let first = graphql_calls(&repo);

        ask_in(&mut repo, 2);
        assert_eq!(graphql_calls(&repo), first);
        ask_in(&mut repo, 6);

        assert_eq!(graphql_calls(&repo), first * 2);
    }

    #[test]
    fn opening_the_dialog_asks_the_stack_at_once() {
        let mut repo = stacked_by(NONE);
        assert!(!text(&mut repo, "feat/x").contains("/3"));
        set(&repo, "stack", STACK);
        let id = id_of(&repo, "feat/x");

        repo.app.open_pull_request(id);
        pump_until(&mut repo.app, &repo.rx, "the answer", |a| a.git.pulls.asking.is_empty());

        repo.app.overlay = None;
        assert!(text(&mut repo, "feat/x").contains("1/3"));
    }

    #[test]
    fn a_host_without_stacks_is_asked_once() {
        let mut repo = stacked_by(UNSUPPORTED);
        set(&repo, "stack-err", "gh: Field 'stack' doesn't exist on type 'PullRequest'");
        repo.app.git.pulls = PullRequests::default();
        repo.ask("gh answers");
        let note = repo.app.pull_requests_note();
        let asked = graphql_calls(&repo);

        ask_in(&mut repo, 10);

        assert_eq!(graphql_calls(&repo), asked);
        assert_eq!(repo.app.pull_requests_note(), note);
        assert!(!text(&mut repo, "feat/x").contains("/3"));
        assert!(text(&mut repo, "feat/x").contains("#482✗"));
    }

    #[test]
    fn a_failing_stack_query_keeps_the_last_stack() {
        let mut repo = stacked();
        let note = repo.app.pull_requests_note();
        set(&repo, "stack-err", "HTTP 502: Bad Gateway");
        std::fs::remove_file(repo.tmp.path().join("gh.stack")).expect("no body");

        ask_in(&mut repo, 2);

        assert!(text(&mut repo, "feat/x").contains("#482✗ 1/3"));
        assert_eq!(repo.app.pull_requests_note(), note);
    }

    #[test]
    fn a_merged_one_keeps_its_last_answer() {
        let mut repo = stacked();
        set(&repo, "pr-feat-x", &VIEW.replace("\"OPEN\"", "\"MERGED\""));
        ask_in(&mut repo, 2);
        let asked = calls_of(&repo, "api graphql feat/x");

        ask_in(&mut repo, 10);

        assert_eq!(calls_of(&repo, "api graphql feat/x"), asked);
        assert!(text(&mut repo, "feat/x").contains("1/3"));
    }
}

mod dialog {
    use super::*;

    fn open(repo: &mut Repo, branch: &str) {
        let id = id_of(repo, branch);
        repo.app.open_pull_request(id);
        pump_until(&mut repo.app, &repo.rx, "the stack", |a| a.git.pulls.asking.is_empty());
    }

    #[test]
    fn it_lists_the_layers_top_first_with_who_holds_them() {
        let mut repo = stacked();
        open(&mut repo, "feat/y");

        let lines = [
            "open · 1 passed",
            "",
            "stack #484 on main · layer 2 of 3",
            "  3 #485 • Pay with a gift card at checkout",
            "▸ 2 #483 ✓ Dark mode for the checkout                feat/y",
            "  1 #482 ✗ Returns page crashes on an empty address  feat/x",
            "cannot merge until #482 below passes its checks",
            "",
            "✓ review-bot",
        ];
        assert_eq!(notes(&repo.app), lines);
    }
}

mod dialog_clicks {
    use super::*;

    #[test]
    fn a_click_on_a_layer_goes_to_its_workspace() {
        let mut repo = stacked();
        let id = id_of(&repo, "feat/y");
        repo.app.open_pull_request(id);
        pump_until(&mut repo.app, &repo.rx, "the stack", |a| a.git.pulls.asking.is_empty());
        let body = ui::update_notes(AREA);

        click(&mut repo.app, Position::new(body.x + 4, body.y + 5));

        assert!(repo.app.overlay.is_none());
        let first = id_of(&repo, "feat/x");
        assert_eq!(repo.app.project().and_then(|p| p.workspace()).map(|w| w.id), Some(first));
    }

    #[test]
    fn a_click_on_a_line_nobody_holds_stays_in_the_dialog() {
        let mut repo = stacked();
        let id = id_of(&repo, "feat/y");
        repo.app.open_pull_request(id);
        pump_until(&mut repo.app, &repo.rx, "the stack", |a| a.git.pulls.asking.is_empty());
        let body = ui::update_notes(AREA);

        click(&mut repo.app, Position::new(body.x + 4, body.y + 3));

        assert!(repo.app.overlay.is_some());
    }

    #[test]
    fn a_merged_layer_below_and_a_branch_behind_add_the_note() {
        let mut repo = stacked_by(&STACK.replacen("\"OPEN\"", "\"MERGED\"", 1));
        let id = id_of(&repo, "feat/y");
        let row = row_of(&repo, "feat/y");
        repo.app.projects[0].workspaces[row].behind = 2;
        repo.app.open_pull_request(id);
        pump_until(&mut repo.app, &repo.rx, "the stack", |a| a.git.pulls.asking.is_empty());
        repo.app.projects[0].workspaces[row].behind = 2;

        let lines = notes(&repo.app);

        assert!(lines.contains(&"#482 below was merged and this branch is behind".to_string()), "{lines:#?}");
        assert!(lines.contains(&"its remote: pull it before pushing again.".to_string()));
        assert!(!lines.iter().any(|l| l.starts_with("cannot merge")));
    }

    #[test]
    fn without_a_merge_below_there_is_no_note() {
        let mut repo = stacked();
        let id = id_of(&repo, "feat/y");
        let row = row_of(&repo, "feat/y");
        repo.app.projects[0].workspaces[row].behind = 2;
        repo.app.open_pull_request(id);
        pump_until(&mut repo.app, &repo.rx, "the stack", |a| a.git.pulls.asking.is_empty());

        assert!(!notes(&repo.app).iter().any(|l| l.contains("was merged")));
    }
}

mod scripts {
    use super::*;

    #[test]
    fn they_see_the_stack_in_the_status() {
        let repo = stacked();
        let id = id_of(&repo, "feat/y");

        let report = repo.app.report(None);

        let workspaces = &report.projects[0].workspaces;
        let pull =
            workspaces.iter().find(|w| w.id == id).and_then(|w| w.pull_request.as_ref()).expect("a pull request");
        let stack = pull.stack.as_ref().expect("a stack");
        assert_eq!((stack.number, stack.base.as_str(), stack.position, stack.size), (484, "main", 2, 3));
        assert_eq!(stack.layers.iter().map(|l| l.number).collect::<Vec<_>>(), [482, 483, 485]);
        let first = &stack.layers[0];
        assert_eq!(first.checks.as_deref(), Some("failing"));
        assert_eq!(first.review.as_deref(), Some("review_required"));
        assert_eq!((stack.layers[2].state.as_str(), first.conflicts), ("draft", false));
    }

    #[test]
    fn a_layer_changing_changes_what_watchers_see() {
        let mut repo = stacked();
        let before = repo.app.refreshed();
        set(&repo, "stack", &STACK.replace("FAILURE", "SUCCESS"));

        ask_in(&mut repo, 2);

        assert_ne!(repo.app.refreshed(), before);
    }
}
