use super::*;

#[test]
fn close_stops_the_shells_of_a_tab() {
    let (mut app, rx) = app();
    let made = done(now(&mut app, None, new_tab(None)));
    let tab = made.ids.tab.expect("the new tab");

    done(now(
        &mut app,
        None,
        Command::Close(wire::Close { item: Item::Tab(tab), remove_worktree: false, force: false }),
    ));

    pump_until(&mut app, &rx, "the tab closes", |a| a.projects[0].workspaces[0].tabs.len() == 1);
}

#[test]
fn a_project_closes_without_asking() {
    let (mut app, rx, _dirs) = app_with(2);
    let id = app.projects[0].id;

    done(now(
        &mut app,
        None,
        Command::Close(wire::Close { item: Item::Project(id), remove_worktree: false, force: false }),
    ));

    pump_until(&mut app, &rx, "the project closes", |a| a.projects.len() == 1);
    assert!(app.overlay.is_none());
}

#[test]
fn rename_without_a_target_names_the_callers_tab() {
    let (mut app, _rx, _dirs) = app_with(2);
    let caller = term(&app, 0).id;
    let rename = |name: &str| Command::Rename(wire::Rename { item: None, name: name.into() });

    done(now(&mut app, Some(caller), rename("mine")));
    let named = app.projects[0].workspaces[0].tabs[0].name.clone();
    done(now(&mut app, Some(caller), rename("")));

    assert_eq!(
        (named.as_deref(), app.projects[0].workspaces[0].tabs[0].name.as_deref()),
        (Some("mine"), None)
    );
}

#[test]
fn a_group_keeps_a_name() {
    let (mut app, _rx) = app();
    let group = app.add_group("work".into());

    let message = error(now(
        &mut app,
        None,
        Command::Rename(wire::Rename { item: Some(Item::Group(group)), name: " ".into() }),
    ));

    assert_eq!((message.as_str(), app.groups[0].entry.name.as_str()), ("a group needs a name", "work"));
}

#[test]
fn notify_shows_a_toast_and_asks_for_a_desktop_notification() {
    let (mut app, _rx) = app();

    done(now(&mut app, None, Command::Notify(wire::Notify { text: "the build\u{7} is ready".into() })));

    let sent: Vec<String> = app.take_notifications().into_iter().map(|n| n.text).collect();
    assert_eq!((toast(&app), sent), (Some("the build is ready"), vec!["the build is ready".to_string()]));
}

#[test]
fn the_todo_list_follows_its_commands() {
    let (mut app, _rx) = app();
    let todo = |app: &mut App, todo| now(app, None, Command::Todo(todo));

    let milk = done(todo(&mut app, wire::Todo::Add("buy milk".into()))).todo.expect("an id");
    let docs = done(todo(&mut app, wire::Todo::Add("write docs".into()))).todo.expect("an id");
    done(todo(&mut app, wire::Todo::Done(milk)));
    done(todo(&mut app, wire::Todo::Rm(docs)));
    let Response::Ok(list) = todo(&mut app, wire::Todo::List) else { panic!("no list") };

    let list: wire::TodoList = serde_json::from_value(list).expect("a list");
    assert_eq!(list.todos, [wire::TodoItem { id: milk, text: "buy milk".into(), done: true }]);
}
