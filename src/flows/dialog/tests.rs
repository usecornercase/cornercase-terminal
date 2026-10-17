use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use rstest::rstest;

use super::edit::{DEFAULT_MODEL, OTHER_MODEL};
use super::*;
use crate::flows::store::{Listing, Origin, Saved};
use crate::flows::{Back, End, Stage};

pub(super) const ROOM: Room = Room { pick: 10, width: 40, height: 5, wide: true };

fn user_flow() -> Flow {
    let mut flow = builtin::flows(&[Known::Claude, Known::Codex], Some(Known::Claude)).remove(0);
    flow.name = "Mine".into();
    flow
}

pub(super) fn dialog() -> Dialog {
    let mut builtins = builtin::flows(&[Known::Claude, Known::Codex], Some(Known::Claude));
    builtins.retain(|flow| flow.name != "Arena");
    let mut flows: Vec<Saved> = builtins.into_iter().map(|flow| Saved { flow, origin: Origin::BuiltIn }).collect();
    flows.push(Saved { flow: user_flow(), origin: Origin::User(PathBuf::from("/flows/mine.toml")) });
    let broken = vec![(PathBuf::from("/flows/bad.toml"), "expected `=`".to_string())];
    let builtins = flows.iter().take(4).map(|saved| saved.flow.name.clone()).collect();
    Dialog::new(
        Listing { flows, broken, builtins },
        Agents::new(vec![Known::Claude, Known::Codex], Some(Known::Claude), true),
    )
}

fn mine() -> usize {
    4
}

pub(super) fn editing() -> Dialog {
    let mut d = dialog();
    d.open(mine());
    d
}

pub(super) fn edit(d: &Dialog) -> &Editing {
    d.edit.as_deref().expect("the edit page is open")
}

fn edit_mut(d: &mut Dialog) -> &mut Editing {
    d.edit.as_deref_mut().expect("the edit page is open")
}

fn key(d: &mut Dialog, code: KeyCode) -> Action {
    d.key(KeyEvent::new(code, KeyModifiers::NONE), ROOM)
}

fn type_text(d: &mut Dialog, text: &str) {
    for c in text.chars() {
        key(d, KeyCode::Char(c));
    }
}

fn click_row(d: &mut Dialog, row: Row) -> Action {
    let i = edit(d).rows().iter().position(|r| *r == row).expect("the row is shown");
    d.hit(Hit::Row(i), ROOM)
}

fn stages(d: &Dialog) -> Vec<(Vec<String>, Option<usize>)> {
    edit(d).flow.stages.iter().map(|s| (s.roles.clone(), s.back.as_ref().map(|b| b.to))).collect()
}

fn roles(names: &[&str]) -> Vec<String> {
    names.iter().map(|n| (*n).to_string()).collect()
}

mod the_list {
    use super::*;

    #[test]
    fn one_list_without_sections_tags_this_projects_flows_and_built_ins_you_edited() {
        let mut d = dialog();
        d.flows[mine()].flow.name = "Quick".into();
        d.flows.remove(2);
        d.flows.push(Saved { flow: user_flow(), origin: Origin::Project(PathBuf::from("/repo/theirs.toml")) });
        d.flows[4].flow.name = "Theirs".into();
        let rows = d.list_rows();
        assert!(rows.iter().all(|r| r.section.is_empty()));
        let notes: Vec<&str> = rows.iter().map(|r| r.note.as_str()).collect();
        assert!(notes[3].starts_with("edited · "), "{notes:?}");
        assert!(notes[4].starts_with("this project · "), "{notes:?}");
        assert!(!notes[0].contains(" · "), "{notes:?}");
    }

    #[test]
    fn a_flow_shows_its_agents_and_its_plan() {
        let rows = dialog().list_rows();
        assert_eq!(
            (rows[0].label.as_str(), rows[0].value.as_str(), rows[0].note.as_str(), rows[0].red),
            ("Full issue", "5 agents", "coder → reviewer + security → tester", false),
        );
    }

    #[test]
    fn a_flow_with_problems_shows_them_in_red() {
        let mut d = dialog();
        d.flows[mine()].flow.stages.clear();
        let row = &d.list_rows()[mine()];
        assert!(row.red);
        assert_eq!(row.value, "`coder` is in no stage (+4 more)");
    }

    #[test]
    fn a_file_that_cannot_be_read_says_why() {
        let row = dialog().list_rows().pop().expect("a row");
        assert_eq!((row.label.as_str(), row.value.as_str(), row.red), ("bad.toml", "expected `=`", true));
    }

    #[test]
    fn the_summary_counts_built_in_flows_and_yours() {
        assert_eq!(dialog().summary(), "4 built in, 1 yours");
    }

    #[rstest]
    #[case::built_in(0, vec![EDIT, DUPLICATE, EXPORT, IMPORT, NEW, DONE])]
    #[case::yours(4, vec![EDIT, DUPLICATE, EXPORT, DELETE, IMPORT, NEW, DONE])]
    #[case::unreadable(5, vec![DELETE, IMPORT, NEW, DONE])]
    fn the_buttons_follow_the_selected_flow(#[case] selected: usize, #[case] expected: Vec<&str>) {
        let mut d = dialog();
        d.select(selected);
        assert_eq!(d.buttons(Room { width: 80, ..ROOM }), expected);
    }

    #[test]
    fn a_built_in_flow_opens_and_saves_as_yours_under_its_name() {
        let mut d = dialog();
        d.open(0);
        assert!(!d.dirty());
        edit_mut(&mut d).flow.description = "mine now".into();
        let Action::Save { flow, place, previous } = d.save() else { panic!("not saved") };
        assert_eq!((flow.name.as_str(), place, previous), ("Full issue", Place::Yours, None));
    }

    #[test]
    fn a_built_in_you_edited_offers_restore_default_which_deletes_your_file() {
        let mut d = dialog();
        d.flows[mine()].flow.name = "Full issue".into();
        d.select(mine());
        assert_eq!(d.buttons(ROOM), [EDIT, DUPLICATE, EXPORT, RESTORE, IMPORT, NEW, DONE]);
        d.press(RESTORE);
        assert_eq!(d.confirm, Some(Confirm::Restore(mine())));
        assert_eq!(d.confirm_text().map(|(title, ..)| title), Some("Restore default"));
        assert_eq!(d.confirmed(), Action::Delete(PathBuf::from("/flows/mine.toml")));
    }

    #[test]
    fn a_click_selects_and_a_second_click_edits() {
        let mut d = dialog();
        d.hit(Hit::Row(mine()), ROOM);
        assert!(d.edit.is_none());
        d.hit(Hit::Row(mine()), ROOM);
        assert_eq!(edit(&d).flow.name, "Mine");
    }
}

mod list_preview {
    use super::*;

    fn flow_named(d: &Dialog) -> Option<String> {
        match d.preview() {
            Some(Preview::Flow(flow)) => Some(flow.name.clone()),
            _ => None,
        }
    }

    #[test]
    fn the_selected_flow_is_previewed_as_soon_as_the_list_opens() {
        assert_eq!(flow_named(&dialog()).as_deref(), Some("Full issue"));
    }

    #[test]
    fn a_click_selects_and_previews_without_editing_and_a_second_click_edits() {
        let mut d = dialog();
        d.hit(Hit::Row(mine()), ROOM);
        assert!(d.edit.is_none());
        assert_eq!(flow_named(&d).as_deref(), Some("Mine"));
        d.hit(Hit::Row(mine()), ROOM);
        assert!(d.edit.is_some());
    }

    #[test]
    fn changing_the_selection_scrolls_the_preview_back_to_the_top() {
        let mut d = dialog();
        d.scroll_preview(5, 20);
        assert_eq!(d.preview_scroll(), 5);
        d.hit(Hit::Row(1), ROOM);
        assert_eq!(d.preview_scroll(), 0);
        d.scroll_preview(5, 20);
        key(&mut d, KeyCode::Down);
        assert_eq!(d.preview_scroll(), 0);
        d.scroll_preview(5, 20);
        key(&mut d, KeyCode::Up);
        assert_eq!(d.preview_scroll(), 0);
        d.scroll_preview(5, 20);
        d.wheel(1, ROOM);
        assert_eq!(d.preview_scroll(), 0);
        d.scroll_preview(5, 20);
        d.hit(Hit::Row(d.cursor), ROOM);
        assert!(d.edit.is_some());
    }

    #[test]
    fn scrolling_stops_at_its_edges() {
        let mut d = dialog();
        d.scroll_preview(100, 7);
        assert_eq!(d.preview_scroll(), 7);
        d.scroll_preview(-100, 7);
        assert_eq!(d.preview_scroll(), 0);
    }

    #[test]
    fn the_list_scroll_is_not_the_editors() {
        let mut d = dialog();
        d.scroll_preview(3, 20);
        d.open(d.cursor);
        assert_eq!(d.preview_scroll(), 0);
        d.scroll_preview(2, 20);
        d.back();
        assert_eq!(d.preview_scroll(), 3);
    }

    #[test]
    fn an_unreadable_file_says_why_it_has_no_diagram() {
        let mut d = dialog();
        d.select(5);
        assert_eq!(
            d.preview(),
            Some(Preview::Note("bad.toml cannot be read as a flow, so it has no diagram: expected `=`".into()))
        );
    }

    #[test]
    fn an_empty_list_says_how_to_get_a_flow() {
        let mut d = dialog();
        d.set_listing(Listing::default());
        assert_eq!(d.preview(), Some(Preview::Note("no flows yet: new makes one, import brings one in".into())));
    }

    #[test]
    fn importing_and_a_text_field_have_no_preview() {
        let mut d = dialog();
        d.press(IMPORT);
        assert_eq!(d.preview(), None);
        let mut d = editing();
        assert!(d.preview().is_some());
        d.hit(Hit::Tab(2), ROOM);
        click_row(&mut d, Row::Prompt);
        assert_eq!(d.preview(), None);
    }

    #[test]
    fn a_wide_list_keeps_its_buttons_whatever_width_the_preview_leaves() {
        let mut d = dialog();
        d.flows[mine()].flow.name = "Full issue".into();
        d.select(mine());
        let room = Room { width: 62, ..ROOM };
        assert_eq!(d.buttons(room), [EDIT, DUPLICATE, EXPORT, RESTORE, IMPORT, NEW, DONE]);
    }

    #[test]
    fn a_narrow_list_swaps_between_the_flows_and_the_diagram() {
        let narrow = Room { wide: false, width: 0, ..ROOM };
        let mut d = dialog();
        assert_eq!(d.buttons(narrow), [EDIT, DUPLICATE, EXPORT, DIAGRAM, IMPORT, NEW, DONE]);
        d.press(DIAGRAM);
        assert!(d.diagram());
        assert_eq!(d.buttons(narrow), [EDIT, DUPLICATE, EXPORT, LIST, IMPORT, NEW, DONE]);
        d.press(LIST);
        assert!(!d.diagram());
        assert_eq!(d.buttons(Room { width: 0, ..ROOM }), [EDIT, DUPLICATE, EXPORT, IMPORT, NEW, DONE]);
    }
}

mod duplicating {
    use super::*;

    #[test]
    fn a_copy_takes_the_name_with_copy() {
        let d = dialog();
        let Action::Duplicate(flow) = d.duplicate() else { panic!("no copy") };
        assert_eq!(flow.name, "Full issue copy");
        assert_eq!(flow.stages, d.flows[0].flow.stages);
    }

    #[test]
    fn a_second_copy_gets_a_number() {
        let mut d = dialog();
        d.flows[mine()].flow.name = "Full issue copy".into();
        let Action::Duplicate(flow) = d.duplicate() else { panic!("no copy") };
        assert_eq!(flow.name, "Full issue copy 2");
    }
}

mod deleting {
    use super::*;

    #[test]
    fn deleting_asks_first() {
        let mut d = dialog();
        d.select(mine());
        assert_eq!(d.press(DELETE), Action::None);
        assert_eq!(d.confirm, Some(Confirm::Delete(mine())));
        assert_eq!(d.confirmed(), Action::Delete(PathBuf::from("/flows/mine.toml")));
    }

    #[test]
    fn an_unreadable_file_can_be_deleted() {
        let mut d = dialog();
        d.select(5);
        d.ask_delete();
        assert_eq!(d.confirmed(), Action::Delete(PathBuf::from("/flows/bad.toml")));
    }

    #[test]
    fn a_built_in_flow_cannot_be_deleted() {
        let mut d = dialog();
        d.ask_delete();
        assert_eq!(d.confirm, None);
    }

    #[test]
    fn esc_keeps_the_flow() {
        let mut d = dialog();
        d.select(mine());
        d.ask_delete();
        key(&mut d, KeyCode::Esc);
        assert_eq!((d.confirm, d.entries()), (None, 6));
    }
}

mod new_flows {
    use super::*;

    #[test]
    fn a_new_flow_has_a_coder_and_a_reviewer_and_no_problems() {
        let mut d = dialog();
        d.press(NEW);
        let flow = &edit(&d).flow;
        assert_eq!(flow.roles.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), ["coder", "reviewer"]);
        assert_eq!(flow.problems(), Vec::<String>::new());
        assert_eq!((edit(&d).place, edit(&d).path.clone()), (Place::Yours, None));
    }

    #[test]
    fn saving_a_new_flow_writes_a_new_file() {
        let mut d = dialog();
        d.press(NEW);
        let Action::Save { flow, place, previous } = d.press(SAVE) else { panic!("not saved") };
        assert_eq!((flow.name.as_str(), place, previous), ("New flow", Place::Yours, None));
    }
}

mod saving {
    use super::*;

    #[test]
    fn a_saved_flow_names_the_file_it_replaces() {
        let mut d = editing();
        let Action::Save { previous, .. } = d.save() else { panic!("not saved") };
        assert_eq!(previous, Some(PathBuf::from("/flows/mine.toml")));
    }

    #[test]
    fn a_flow_with_problems_is_not_saved() {
        let mut d = editing();
        edit_mut(&mut d).flow.stages.clear();
        assert_eq!(d.save(), Action::None);
        assert_eq!(d.hint(), "fix the problems in red to save the flow");
    }

    #[test]
    fn the_problems_are_always_shown_and_nothing_else() {
        let mut d = editing();
        assert_eq!(d.warnings(), Vec::<String>::new());
        edit_mut(&mut d).flow.name.clear();
        assert_eq!(d.warnings(), ["the flow has no name"]);
    }

    #[test]
    fn a_project_flow_says_in_red_what_env_and_args_it_sets() {
        let mut d = dialog();
        d.flows[mine()].origin = Origin::Project(PathBuf::from("/repo/.cornercase/flows/mine.toml"));
        d.flows[mine()].flow.roles[3].env.insert("PATH".into(), "/tmp/evil".into());
        d.open(mine());
        let warnings = d.warnings();
        assert_eq!(warnings[0], "this project's flow sets PATH for `tester`; they run without asking, so check them");
        let mut yours = editing();
        edit_mut(&mut yours).flow.roles[3].env.insert("PATH".into(), "/tmp/evil".into());
        assert_eq!(yours.warnings().len(), 0);
    }

    #[test]
    fn once_saved_the_flow_is_clean() {
        let mut d = editing();
        edit_mut(&mut d).flow.description = "changed".into();
        d.saved(PathBuf::from("/flows/mine-2.toml"));
        assert!(!d.dirty());
        assert_eq!(edit(&d).path, Some(PathBuf::from("/flows/mine-2.toml")));
    }

    #[test]
    fn the_flow_can_move_to_the_project() {
        let mut d = editing();
        click_row(&mut d, Row::Place);
        d.hit(Hit::Pick(1), ROOM);
        assert_eq!(edit(&d).place, Place::Project);
        assert!(d.dirty());
    }
}

mod leaving {
    use super::*;

    #[test]
    fn done_without_changes_goes_back_to_the_list() {
        let mut d = editing();
        assert_eq!(d.press(DONE), Action::None);
        assert!(d.edit.is_none());
    }

    #[test]
    fn done_with_changes_asks_first() {
        let mut d = editing();
        edit_mut(&mut d).flow.description = "changed".into();
        d.press(DONE);
        assert_eq!(d.confirm, Some(Confirm::Leave(Then::List)));
        assert_eq!(d.confirmed(), Action::None);
        assert!(d.edit.is_none());
    }

    #[test]
    fn a_press_outside_with_changes_asks_then_closes() {
        let mut d = editing();
        edit_mut(&mut d).flow.description = "changed".into();
        assert_eq!(d.close(), Action::None);
        assert_eq!(d.confirm, Some(Confirm::Leave(Then::Close)));
        assert_eq!(d.confirmed(), Action::Close);
    }

    #[test]
    fn a_press_outside_the_question_keeps_editing() {
        let mut d = editing();
        edit_mut(&mut d).flow.description = "changed".into();
        d.close();
        assert_eq!(d.close(), Action::None);
        assert_eq!((d.confirm, d.dirty()), (None, true));
    }

    #[test]
    fn an_edited_prompt_not_kept_yet_counts_as_a_change() {
        let mut d = editing();
        d.hit(Hit::Tab(2), ROOM);
        click_row(&mut d, Row::Prompt);
        assert!(!d.dirty());
        type_text(&mut d, "x");
        assert!(d.dirty());
    }
}

mod fields {
    use super::*;

    #[test]
    fn the_name_is_typed_and_kept_with_enter() {
        let mut d = editing();
        click_row(&mut d, Row::Name);
        key(&mut d, KeyCode::Backspace);
        type_text(&mut d, "e2");
        key(&mut d, KeyCode::Enter);
        assert_eq!((edit(&d).flow.name.as_str(), d.title()), ("Mine2", "Flows › Mine2".to_string()));
    }

    #[test]
    fn an_empty_name_is_refused() {
        let mut d = editing();
        click_row(&mut d, Row::Name);
        d.edit.as_deref_mut().and_then(|e| e.field.as_mut()).expect("field").input.clear();
        key(&mut d, KeyCode::Enter);
        assert_eq!(d.error().as_deref(), Some("a flow needs a name"));
    }

    #[rstest]
    #[case::a_number("5", Ok(5))]
    #[case::zero("0", Err(()))]
    #[case::words("lots", Err(()))]
    fn at_most_takes_a_number(#[case] typed: &str, #[case] expected: Result<u32, ()>) {
        let mut d = editing();
        click_row(&mut d, Row::Stage(1));
        click_row(&mut d, Row::AtMost(1));
        edit_mut(&mut d).field.as_mut().expect("field").input = typed.into();
        key(&mut d, KeyCode::Enter);
        let at_most = edit(&d).flow.stages[1].back.as_ref().map(|b| b.at_most);
        match expected {
            Ok(n) => assert_eq!(at_most, Some(n)),
            Err(()) => assert_eq!((at_most, d.error().is_some()), (Some(3), true)),
        }
    }

    #[test]
    fn env_takes_key_value_pairs() {
        let mut d = editing();
        d.hit(Hit::Tab(2), ROOM);
        click_row(&mut d, Row::Env);
        type_text(&mut d, "CLAUDE_CODE_USE_BEDROCK=1 'AWS_REGION=eu west'");
        key(&mut d, KeyCode::Enter);
        let env: Vec<(String, String)> =
            edit(&d).flow.roles[0].env.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        assert_eq!(env, [("AWS_REGION".into(), "eu west".into()), ("CLAUDE_CODE_USE_BEDROCK".into(), "1".into())]);
    }

    #[test]
    fn env_without_an_equals_sign_is_refused() {
        let mut d = editing();
        d.hit(Hit::Tab(2), ROOM);
        click_row(&mut d, Row::Env);
        type_text(&mut d, "REGION");
        key(&mut d, KeyCode::Enter);
        assert_eq!(d.error().as_deref(), Some("write KEY=value pairs; `REGION` has no ="));
    }

    #[test]
    fn args_are_split_like_the_agents_settings() {
        let mut d = editing();
        d.hit(Hit::Tab(2), ROOM);
        click_row(&mut d, Row::Args);
        type_text(&mut d, "--effort high --name 'a b'");
        d.press(SAVE);
        assert_eq!(edit(&d).flow.roles[0].args, roles(&["--effort", "high", "--name", "a b"]));
    }
}

mod roles_tabs {
    use super::*;

    #[test]
    fn the_tabs_are_the_flow_the_orchestrator_each_role_and_a_new_one() {
        assert_eq!(
            edit(&editing()).tabs(),
            ["Flow", "Orchestrator", "coder", "reviewer", "security", "tester", ADD_ROLE]
        );
    }

    #[test]
    fn renaming_a_role_renames_it_in_the_stages() {
        let mut d = editing();
        d.hit(Hit::Tab(3), ROOM);
        click_row(&mut d, Row::RoleName);
        edit_mut(&mut d).field.as_mut().expect("field").input = "critic".into();
        key(&mut d, KeyCode::Enter);
        assert_eq!(stages(&d)[1].0, roles(&["critic", "security"]));
        assert_eq!(edit(&d).flow.problems(), Vec::<String>::new());
    }

    #[rstest]
    #[case::taken("coder", "another role is named `coder`")]
    #[case::orchestrator("orchestrator", "`orchestrator` is taken by the orchestrator")]
    #[case::empty(" ", "a role needs a name")]
    fn a_role_name_must_be_free(#[case] name: &str, #[case] error: &str) {
        let mut d = editing();
        d.hit(Hit::Tab(3), ROOM);
        click_row(&mut d, Row::RoleName);
        edit_mut(&mut d).field.as_mut().expect("field").input = name.into();
        key(&mut d, KeyCode::Enter);
        assert_eq!((d.error().as_deref(), edit(&d).flow.roles[1].name.as_str()), (Some(error), "reviewer"));
    }

    #[test]
    fn the_orchestrator_tab_has_no_name_edits_or_delete() {
        let mut d = editing();
        d.hit(Hit::Tab(1), ROOM);
        assert_eq!(edit(&d).rows(), [Row::Agent, Row::Model, Row::Effort, Row::Args, Row::Env, Row::Prompt]);
        assert_eq!(d.buttons(ROOM), [SAVE, DONE]);
    }

    #[test]
    fn a_new_role_gets_a_stage_and_asks_for_its_name() {
        let mut d = editing();
        d.hit(Hit::Tab(6), ROOM);
        assert_eq!(edit(&d).tab, Tab::Role(4));
        assert_eq!(edit(&d).field.as_ref().map(|f| f.row), Some(Row::RoleName));
        type_text(&mut d, "docs");
        key(&mut d, KeyCode::Enter);
        assert_eq!(stages(&d).last(), Some(&(roles(&["docs"]), None)));
        assert_eq!(edit(&d).flow.problems(), Vec::<String>::new());
    }

    #[test]
    fn a_flow_takes_at_most_eight_roles() {
        let mut d = editing();
        for _ in 0..10 {
            d.hit(Hit::Tab(edit(&d).tabs().len() - 1), ROOM);
            key(&mut d, KeyCode::Esc);
        }
        assert_eq!(edit(&d).flow.roles.len(), crate::flows::MAX_ROLES);
        assert_eq!(d.hint(), "a flow takes at most 8 roles besides the orchestrator");
    }

    #[test]
    fn deleting_a_role_drops_the_stages_it_leaves_empty_and_their_loops() {
        let mut d = editing();
        d.hit(Hit::Tab(2), ROOM);
        d.press(DELETE_ROLE);
        assert_eq!(stages(&d), [(roles(&["reviewer", "security"]), None), (roles(&["tester"]), None)]);
        assert_eq!(edit(&d).tab, Tab::Flow);
    }

    #[test]
    fn a_role_runs_in_its_stage_with_its_agent_changing_resets_the_model() {
        let mut d = editing();
        d.hit(Hit::Tab(2), ROOM);
        edit_mut(&mut d).flow.roles[0].model = Some("opus".into());
        click_row(&mut d, Row::Agent);
        let codex = Known::ALL.iter().position(|k| *k == Known::Codex).expect("codex");
        d.hit(Hit::Pick(codex), ROOM);
        let role = &edit(&d).flow.roles[0];
        assert_eq!((role.agent.as_str(), role.model.clone()), ("codex", None));
    }

    #[test]
    fn edits_files_flips_on_a_click() {
        let mut d = editing();
        d.hit(Hit::Tab(3), ROOM);
        click_row(&mut d, Row::Edits);
        assert!(edit(&d).flow.roles[1].edits);
    }
}

mod models {
    use super::*;

    fn labels(d: &Dialog) -> Vec<String> {
        edit(d).pick_choices().iter().map(|c| c.label.clone()).collect()
    }

    #[test]
    fn claude_offers_its_aliases() {
        let mut d = editing();
        d.hit(Hit::Tab(2), ROOM);
        assert_eq!(click_row(&mut d, Row::Model), Action::None);
        assert_eq!(labels(&d), [DEFAULT_MODEL, "opus", "sonnet", "haiku", OTHER_MODEL]);
        d.hit(Hit::Pick(2), ROOM);
        assert_eq!(edit(&d).flow.roles[0].model.as_deref(), Some("sonnet"));
    }

    #[test]
    fn other_agents_offer_their_default_or_a_typed_model() {
        let mut d = editing();
        d.hit(Hit::Tab(3), ROOM);
        click_row(&mut d, Row::Model);
        assert_eq!(labels(&d), [DEFAULT_MODEL, OTHER_MODEL]);
        d.hit(Hit::Pick(1), ROOM);
        type_text(&mut d, "gpt-5.5");
        key(&mut d, KeyCode::Enter);
        assert_eq!(edit(&d).flow.roles[1].model.as_deref(), Some("gpt-5.5"));
    }

    #[test]
    fn default_clears_the_model() {
        let mut d = editing();
        d.hit(Hit::Tab(2), ROOM);
        edit_mut(&mut d).flow.roles[0].model = Some("opus".into());
        click_row(&mut d, Row::Model);
        d.hit(Hit::Pick(0), ROOM);
        assert_eq!(edit(&d).flow.roles[0].model, None);
    }

    #[test]
    fn an_effort_is_one_of_the_agents_levels_and_goes_with_its_agent() {
        let mut d = editing();
        d.hit(Hit::Tab(2), ROOM);
        click_row(&mut d, Row::Effort);
        assert_eq!(labels(&d), [DEFAULT_MODEL, "low", "medium", "high", "xhigh", "max"]);
        d.hit(Hit::Pick(3), ROOM);
        assert_eq!(edit(&d).flow.roles[0].effort.as_deref(), Some("high"));
        click_row(&mut d, Row::Effort);
        d.hit(Hit::Pick(0), ROOM);
        assert_eq!(edit(&d).flow.roles[0].effort, None);
        edit_mut(&mut d).flow.roles[0].effort = Some("max".into());
        click_row(&mut d, Row::Agent);
        d.hit(Hit::Pick(1), ROOM);
        assert_eq!((edit(&d).flow.roles[0].agent.as_str(), edit(&d).flow.roles[0].effort.as_deref()), ("codex", None));
    }

    #[test]
    fn an_agent_without_an_effort_switch_says_so_and_offers_nothing() {
        let mut d = editing();
        d.hit(Hit::Tab(2), ROOM);
        edit_mut(&mut d).flow.roles[0].agent = "gemini".into();
        click_row(&mut d, Row::Effort);
        assert!(edit(&d).pick.is_none());
        let rows = edit(&d).row_texts(&d.agents);
        let effort = rows.iter().find(|row| row.label == "effort").expect("the effort row");
        assert_eq!((effort.value.as_str(), effort.note.as_str()), ("none", "gemini has no effort switch"));
    }

    #[test]
    fn opencode_reads_its_models_once_and_fills_the_open_pick() {
        let mut d = editing();
        d.hit(Hit::Tab(3), ROOM);
        edit_mut(&mut d).flow.roles[1].agent = "opencode".into();
        assert_eq!(click_row(&mut d, Row::Model), Action::ReadModels(Known::Opencode));
        assert_eq!(click_row(&mut d, Row::Model), Action::None);
        d.models_read(Known::Opencode, vec!["deepseek/deepseek-v4-pro".into()]);
        assert_eq!(labels(&d), [DEFAULT_MODEL, "deepseek/deepseek-v4-pro", OTHER_MODEL]);
    }
}

mod stages_and_loops {
    use super::*;

    #[test]
    fn a_click_on_a_stage_opens_its_details() {
        let mut d = editing();
        click_row(&mut d, Row::Stage(1));
        let rows = edit(&d).rows();
        assert!(rows.contains(&Row::Roles(1)) && rows.contains(&Row::When(1)) && rows.contains(&Row::AtMost(1)));
        click_row(&mut d, Row::Stage(1));
        assert!(!edit(&d).rows().contains(&Row::Roles(1)));
    }

    #[test]
    fn roles_are_checked_in_the_order_of_the_tabs() {
        let mut d = editing();
        click_row(&mut d, Row::Stage(1));
        click_row(&mut d, Row::Roles(1));
        d.hit(Hit::Pick(1), ROOM);
        assert_eq!(stages(&d)[1].0, roles(&["security"]));
        d.hit(Hit::Pick(1), ROOM);
        assert_eq!(stages(&d)[1].0, roles(&["reviewer", "security"]));
        assert_eq!(edit(&d).pick_marks(), Some(vec![None, Some(true), Some(true), None]));
        d.press(DONE);
        assert!(edit(&d).pick.is_none());
    }

    #[test]
    fn a_new_stage_asks_for_its_roles() {
        let mut d = editing();
        edit_mut(&mut d).remove_stage(2);
        click_row(&mut d, Row::AddStage);
        assert_eq!(edit(&d).pick.as_ref().map(|p| p.row), Some(Row::Roles(2)));
        d.hit(Hit::Pick(3), ROOM);
        assert_eq!(stages(&d).last(), Some(&(roles(&["tester"]), None)));
    }

    #[test]
    fn a_role_in_another_stage_is_offered_with_where_it_is_and_cannot_be_checked() {
        let mut d = editing();
        click_row(&mut d, Row::AddStage);
        let notes: Vec<String> = edit(&d).pick_choices().iter().map(|c| c.note.clone()).collect();
        assert_eq!(notes[0], "in stage 1: a role works in one stage");
        d.hit(Hit::Pick(0), ROOM);
        assert_eq!(stages(&d).last(), Some(&(Vec::new(), None)));
        assert_eq!(d.hint(), "`coder` works in stage 1, and a role works in one stage");
        assert_eq!(edit(&d).pick_marks(), Some(vec![None, None, None, None]));
    }

    #[test]
    fn a_loop_goes_back_to_an_earlier_stage_only() {
        let mut d = editing();
        click_row(&mut d, Row::Stage(2));
        click_row(&mut d, Row::Back(2));
        let offered: Vec<String> = edit(&d).pick_choices().iter().map(|c| c.label.clone()).collect();
        assert_eq!(offered, ["never", "stage 1", "stage 2"]);
        d.hit(Hit::Pick(2), ROOM);
        assert_eq!(edit(&d).flow.stages[2].back, Some(Back { to: 2, when: "failures".into(), at_most: 2 }));
        click_row(&mut d, Row::Back(2));
        d.hit(Hit::Pick(0), ROOM);
        assert_eq!(edit(&d).flow.stages[2].back, None);
    }

    #[test]
    fn removing_a_stage_renumbers_the_loops_after_it() {
        let mut d = editing();
        edit_mut(&mut d).flow.stages.insert(0, Stage { roles: roles(&["tester"]), back: None });
        for stage in &mut edit_mut(&mut d).flow.stages[2..] {
            if let Some(back) = &mut stage.back {
                back.to = 2;
            }
        }
        edit_mut(&mut d).remove_stage(0);
        assert_eq!(stages(&d).iter().map(|s| s.1).collect::<Vec<_>>(), [None, Some(1), Some(1)]);
    }

    #[test]
    fn removing_the_stage_a_loop_goes_to_drops_the_loop() {
        let mut d = editing();
        d.hit(Hit::Remove(edit(&d).rows().iter().position(|r| *r == Row::Stage(0)).expect("stage")), ROOM);
        assert_eq!(stages(&d), [(roles(&["reviewer", "security"]), None), (roles(&["tester"]), None)]);
    }

    #[test]
    fn moving_a_stage_keeps_its_loops_pointing_at_the_same_stages() {
        let mut d = editing();
        let tester = edit(&d).rows().iter().position(|r| *r == Row::Stage(2)).expect("stage");
        d.hit(Hit::MoveUp(tester), ROOM);
        assert_eq!(
            stages(&d),
            [(roles(&["coder"]), None), (roles(&["tester"]), Some(1)), (roles(&["reviewer", "security"]), Some(1))],
        );
    }

    #[test]
    fn a_loop_that_would_go_forward_is_dropped() {
        let mut d = editing();
        let reviewers = edit(&d).rows().iter().position(|r| *r == Row::Stage(1)).expect("stage");
        d.hit(Hit::MoveUp(reviewers), ROOM);
        assert_eq!(stages(&d)[0], (roles(&["reviewer", "security"]), None));
        assert_eq!(edit(&d).flow.problems(), Vec::<String>::new());
    }

    fn whens(d: &Dialog) -> Vec<Option<String>> {
        edit(d).flow.stages.iter().map(|s| s.back.as_ref().map(|b| b.when.clone())).collect()
    }

    fn typing_when(stage: usize) -> Dialog {
        let mut d = editing();
        click_row(&mut d, Row::Stage(stage));
        click_row(&mut d, Row::When(stage));
        type_text(&mut d, "X");
        d
    }

    fn stage_row(d: &Dialog, stage: usize) -> usize {
        edit(d).rows().iter().position(|r| *r == Row::Stage(stage)).expect("the stage row")
    }

    #[test]
    fn moving_a_stage_while_its_when_is_typed_keeps_the_text_on_that_stage() {
        let mut d = typing_when(2);
        d.hit(Hit::MoveUp(stage_row(&d, 2)), ROOM);
        key(&mut d, KeyCode::Enter);
        assert_eq!(whens(&d), [None, Some("failuresX".into()), Some("findings".into())]);
    }

    #[test]
    fn removing_an_earlier_stage_while_a_when_is_typed_keeps_the_text_on_its_stage() {
        let mut d = typing_when(2);
        d.hit(Hit::Remove(stage_row(&d, 1)), ROOM);
        key(&mut d, KeyCode::Enter);
        assert_eq!(whens(&d), [None, Some("failuresX".into())]);
    }

    #[test]
    fn removing_the_stage_whose_when_is_typed_drops_the_text() {
        let mut d = typing_when(2);
        d.hit(Hit::Remove(stage_row(&d, 2)), ROOM);
        assert!(edit(&d).field.is_none());
        assert_eq!(whens(&d), [None, Some("findings".into())]);
    }

    #[test]
    fn the_new_stage_row_and_the_new_role_tab_say_add() {
        let d = editing();
        let label = edit(&d).row_texts(&d.agents).into_iter().find(|t| t.label.contains("stage")).map(|t| t.label);
        assert_eq!(label.as_deref(), Some("+ add stage"));
        assert_eq!(edit(&d).tabs().last().map(String::as_str), Some("+ add role"));
    }

    #[test]
    fn the_stage_being_changed_is_the_one_the_preview_follows_once() {
        let mut d = editing();
        assert_eq!(edit_mut(&mut d).newly_focused(), None);
        click_row(&mut d, Row::Stage(1));
        click_row(&mut d, Row::When(1));
        assert_eq!(edit(&d).preview_stage(), Some(1));
        assert_eq!(edit_mut(&mut d).newly_focused(), Some(1));
        assert_eq!(edit_mut(&mut d).newly_focused(), None);
        key(&mut d, KeyCode::Esc);
        d.hit(Hit::Tab(2), ROOM);
        assert_eq!(edit(&d).preview_stage(), None);
    }

    #[test]
    fn the_preview_scrolls_within_its_bounds() {
        let mut d = editing();
        edit_mut(&mut d).scroll_preview(5, 3);
        assert_eq!(edit(&d).preview, 3);
        edit_mut(&mut d).scroll_preview(-9, 3);
        assert_eq!(edit(&d).preview, 0);
    }

    #[test]
    fn the_end_is_picked() {
        let mut d = editing();
        click_row(&mut d, Row::End);
        d.hit(Hit::Pick(1), ROOM);
        assert_eq!(edit(&d).flow.end, End::Commits);
    }
}

mod long_texts {
    use super::*;

    #[test]
    fn notes_take_line_breaks_and_save_keeps_them() {
        let mut d = editing();
        click_row(&mut d, Row::Notes);
        type_text(&mut d, "one");
        key(&mut d, KeyCode::Enter);
        type_text(&mut d, "two");
        d.press(SAVE);
        assert_eq!(edit(&d).flow.notes, "one\ntwo");
    }

    #[test]
    fn esc_drops_what_was_typed() {
        let mut d = editing();
        d.hit(Hit::Tab(2), ROOM);
        let before = edit(&d).flow.roles[0].prompt.clone();
        click_row(&mut d, Row::Prompt);
        type_text(&mut d, "never mind");
        key(&mut d, KeyCode::Esc);
        assert_eq!((edit(&d).flow.roles[0].prompt.clone(), edit(&d).text.is_none()), (before, true));
    }

    #[test]
    fn a_paste_keeps_its_lines() {
        let mut d = editing();
        click_row(&mut d, Row::Notes);
        d.paste("a\nb", ROOM);
        d.press(SAVE);
        assert_eq!(edit(&d).flow.notes, "a\nb");
    }

    #[test]
    fn a_click_places_the_cursor_and_the_wheel_scrolls() {
        let mut d = editing();
        click_row(&mut d, Row::Notes);
        d.paste(&["line"; 12].join("\n"), ROOM);
        assert_eq!(edit(&d).text.as_ref().map(|t| t.editor.scroll()), Some(7));
        d.wheel(-3, ROOM);
        d.hit(Hit::Text { line: 0, col: 2 }, ROOM);
        type_text(&mut d, "X");
        d.press(SAVE);
        assert!(edit(&d).flow.notes.contains("liXne"));
    }

    #[test]
    fn the_prompt_names_its_role() {
        let mut d = editing();
        d.hit(Hit::Tab(3), ROOM);
        click_row(&mut d, Row::Prompt);
        assert_eq!(edit(&d).text_heading(), "reviewer › prompt");
        assert_eq!(d.buttons(ROOM), [SAVE, CANCEL]);
    }
}

mod narrow {
    use super::*;

    #[test]
    fn a_narrow_dialog_shows_the_diagram_behind_a_button() {
        let mut d = editing();
        let narrow = Room { wide: false, width: 0, ..ROOM };
        assert_eq!(d.buttons(narrow), [DIAGRAM, SAVE, DONE]);
        d.press(DIAGRAM);
        assert!(edit(&d).diagram);
        assert_eq!(d.buttons(narrow), [FORM, SAVE, DONE]);
    }
}
