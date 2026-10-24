use super::*;

const LINK: &str = "https://discord.com/oauth2/authorize?client_id=123456789&integration_type=0\
                    &scope=bot+applications.commands&permissions=292057844816";

fn with(setup: ForumSetup) -> View<'static> {
    View { overlay: Some(Overlay::Forum(setup)), ..view(&["cornercase"]) }
}

fn steps(done: usize) -> Vec<(bool, String)> {
    let content = if done > 1 {
        "Message Content Intent is on"
    } else {
        "turn on Message Content Intent: Developer Portal › Bot"
    };
    let server = if done > 2 { "the app is in home" } else { "add the app to your server" };
    ["Guild Install is on", content, server, "a private forum channel, #cornercase, is made for you"]
        .iter()
        .enumerate()
        .map(|(i, step)| (i < done, (*step).to_string()))
        .collect()
}

fn halfway() -> ForumSetup {
    ForumSetup {
        steps: steps(2),
        servers: Vec::new(),
        url: Some(LINK.into()),
        code: remote_access::code(LINK),
        notes: vec!["open the link and add the app to your own server.".into(), "no server yet?".into()],
        problem: None,
    }
}

fn ready() -> ForumSetup {
    ForumSetup { steps: steps(4), servers: Vec::new(), url: None, code: Vec::new(), notes: Vec::new(), problem: None }
}

fn two_servers() -> ForumSetup {
    ForumSetup { servers: vec!["home".into(), "work".into()], ..ready() }
}

fn cell_at(view: &View, hit: ForumHit) -> (Position, String) {
    let Some(Overlay::Forum(setup)) = &view.overlay else { panic!("no forum dialog") };
    let pos = (0..20)
        .flat_map(|y| (0..W).map(move |x| Position::new(x, y)))
        .find(|pos| forum_hit(AREA, setup, *pos) == Some(hit))
        .unwrap_or_else(|| panic!("no {hit:?}"));
    let t = render(view);
    let text: String = (pos.x..pos.x + 10).map(|x| t.backend().buffer()[(x, pos.y)].symbol().to_string()).collect();
    (pos, text)
}

#[test]
fn renders_the_forum_setup_halfway_with_the_server_link() {
    insta::assert_snapshot!(render_sized(&with(halfway()), W, 50).backend());
}

#[test]
fn renders_the_forum_setup_with_every_step_done() {
    insta::assert_snapshot!(render(&with(ready())).backend());
}

#[test]
fn the_buttons_and_the_servers_are_hit_where_they_are_drawn() {
    let view = with(two_servers());

    assert!(cell_at(&view, ForumHit::Done).1.starts_with(" done "));
    assert!(cell_at(&view, ForumHit::TurnOff).1.starts_with(" turn off "));
    assert!(cell_at(&view, ForumHit::Server(1)).1.starts_with("  › work"));
}
