use super::*;

fn with_bot(via: Via, chat: Option<&str>) -> Settings {
    let mut s = settings();
    *s.messenger_mut(via) =
        Messenger { bot: Some("@cornercase_bot".into()), chat: chat.map(String::from), ..Messenger::default() };
    s
}

#[test]
fn the_phone_row_shows_once_a_bot_is_set() {
    let mut s = settings();
    s.open_page(Page::Remote);
    let alone = s.rows().contains(&Row::Chat(Via::Telegram));
    let mut s = with_bot(Via::Telegram, None);
    s.open_page(Page::Remote);
    let (telegram, discord) = (Via::Telegram, Via::Discord);
    assert_eq!(s.rows(), [Row::Remote, Row::Bot(telegram), Row::Chat(telegram), Row::Bot(discord)]);
    assert!(!alone);
}

#[rstest::rstest]
#[case::telegram(Via::Telegram, "enter shows a code to scan with your phone")]
#[case::discord(Via::Discord, "enter shows a link to add the app to your account")]
fn an_unpaired_phone_row_says_how_that_platform_pairs(#[case] via: Via, #[case] note: &str) {
    let s = with_bot(via, None);

    assert_eq!(s.row_text(&Row::Chat(via)).note, note);
}

#[test]
fn a_pasted_bot_token_is_hidden_and_checked() {
    let mut s = settings();
    go_to(&mut s, &Row::Bot(Via::Telegram));
    press(&mut s, KeyCode::Enter);
    s.paste("1:t\n");
    let shown = s.edit_field().map(|(_, value)| value);

    let action = press(&mut s, KeyCode::Enter);
    let busy = s.busy();
    s.bot_checked(Via::Telegram, Ok("cornercase_bot".into()), false);
    let check = Action::CheckBot(Via::Telegram, Secret("1:t".into()));
    assert_eq!((shown.as_deref(), action, busy), (Some("•••"), check, true));
    assert_eq!((s.edit.is_none(), s.busy()), (true, false));
    assert_eq!(s.notice.as_deref(), Some("bot @cornercase_bot is ready: pair your phone in the row below"));
}

#[test]
fn a_refused_bot_token_stays_in_the_field() {
    let mut s = settings();
    go_to(&mut s, &Row::Bot(Via::Telegram));
    press(&mut s, KeyCode::Enter);
    s.paste("bad");
    press(&mut s, KeyCode::Enter);
    s.bot_checked(Via::Telegram, Err("Telegram refused the bot token".into()), false);
    assert_eq!(s.edit.and_then(|e| e.error).as_deref(), Some("Telegram refused the bot token"));
}

#[test]
fn enter_pairs_and_delete_removes_the_bot_or_forgets_the_phone() {
    let discord = Via::Discord;
    let mut s = with_bot(discord, Some("@ada"));
    go_to(&mut s, &Row::Chat(discord));
    let pair = press(&mut s, KeyCode::Enter);
    let forget = press(&mut s, KeyCode::Backspace);
    go_to(&mut s, &Row::Bot(discord));
    let remove = press(&mut s, KeyCode::Delete);
    let expected = (Action::Pair(discord), Action::ForgetChat(discord), Action::RemoveBot(discord));
    assert_eq!((pair, forget, remove), expected);
}

mod forum_mode {
    use serde_json::json;

    use super::*;

    fn paired() -> Settings {
        with_bot(Via::Discord, Some("@ada"))
    }

    fn switched_on(mut s: Settings) -> Settings {
        s.config.discord_forum = true;
        s
    }

    #[test]
    fn shows_under_the_phone_only_once_discord_has_a_chat() {
        let mut unpaired = with_bot(Via::Discord, None);
        unpaired.open_page(Page::Remote);
        let mut telegram = with_bot(Via::Telegram, Some("Ana"));
        telegram.open_page(Page::Remote);
        let mut s = paired();
        s.open_page(Page::Remote);

        let discord = Via::Discord;
        assert!(!unpaired.rows().contains(&Row::Forum(discord)));
        assert!(!telegram.rows().iter().any(|row| matches!(row, Row::Forum(_))));
        assert_eq!(&s.rows()[2..], [Row::Bot(discord), Row::Chat(discord), Row::Forum(discord)]);
    }

    #[rstest::rstest]
    #[case::off(false, "", "[ ] off", "a post per workspace in a forum channel of your server")]
    #[case::waiting(true, "", "[x] on", "enter sets it up")]
    #[case::a_step_left(true, "add the app to your server", "[x] on", "add the app to your server")]
    #[case::ready(true, "Ada's server › #cornercase", "[x] on", "Ada's server › #cornercase")]
    fn says_what_it_waits_for(#[case] on: bool, #[case] forum: &str, #[case] value: &str, #[case] note: &str) {
        let mut s = paired();
        s.config.discord_forum = on;
        s.messenger_mut(Via::Discord).forum = forum.into();

        let text = s.row_text(&Row::Forum(Via::Discord));
        assert_eq!((text.label.as_str(), text.value.as_str(), text.note.as_str()), ("forum mode", value, note));
    }

    #[test]
    fn turning_it_on_saves_and_opens_the_setup() {
        let mut s = paired();
        go_to(&mut s, &Row::Forum(Via::Discord));

        let action = press(&mut s, KeyCode::Enter);
        let saved = Config { discord_forum: true, ..Config::default() };
        assert_eq!(action, Action::Forum(Via::Discord, Some(Box::new(saved))));
        assert!(s.config.discord_forum);
    }

    #[test]
    fn enter_on_a_forum_not_ready_opens_the_setup_again() {
        let mut s = switched_on(paired());
        go_to(&mut s, &Row::Forum(Via::Discord));

        assert_eq!(press(&mut s, KeyCode::Enter), Action::Forum(Via::Discord, None));
        assert!(s.config.discord_forum);
    }

    #[test]
    fn enter_on_a_ready_forum_turns_it_off() {
        let mut s = switched_on(paired());
        s.messenger_mut(Via::Discord).forum_ready = true;
        go_to(&mut s, &Row::Forum(Via::Discord));

        let config = saved(press(&mut s, KeyCode::Enter));
        assert!(!config.discord_forum);
    }

    #[rstest::rstest]
    #[case::on(false, true)]
    #[case::off_before_it_is_ready(true, false)]
    fn the_config_request_switches_it_without_the_setup(#[case] from: bool, #[case] to: bool) {
        let mut s = paired();
        s.config.discord_forum = from;

        let notice = s.apply("discord_forum", &json!(to)).expect("applied");
        assert_eq!(s.config.discord_forum, to);
        assert!(notice.is_some_and(|n| n.starts_with("forum mode is")));
    }

    #[test]
    fn is_described_as_a_switch_on_the_remote_page() {
        let setting = paired().describe().into_iter().find(|s| s.key == "discord_forum").expect("discord_forum");
        let place = (setting.page.as_str(), setting.section.as_str(), setting.label.as_str());
        assert_eq!((setting.kind, place), (crate::control::config::Kind::Switch, ("Remote", "Discord", "forum mode")));
    }
}

mod closed_workspaces {
    use serde_json::json;

    use super::*;
    use crate::control::config::Kind;

    const ROW: Row = Row::ForumClose(Via::Discord);

    fn forum_on() -> Settings {
        let mut s = with_bot(Via::Discord, Some("@ada"));
        s.config.discord_forum = true;
        s
    }

    #[test]
    fn shows_under_the_forum_mode_only_while_it_is_on() {
        let mut off = with_bot(Via::Discord, Some("@ada"));
        off.open_page(Page::Remote);
        let mut s = forum_on();
        s.open_page(Page::Remote);

        assert!(!off.rows().contains(&ROW));
        assert_eq!(&s.rows()[4..], [Row::Forum(Via::Discord), ROW]);
    }

    #[rstest::rstest]
    #[case::archived_by_default("archive", "archive")]
    #[case::deleted("delete", "delete")]
    #[case::unknown_is_archived("burn", "archive")]
    fn shows_what_happens_to_the_thread(#[case] setting: &str, #[case] shown: &str) {
        let mut s = forum_on();
        s.config.discord_forum_close = setting.into();

        let text = s.row_text(&ROW);
        assert_eq!((text.label.as_str(), text.value.as_str()), ("closed workspaces", shown));
    }

    #[test]
    fn enter_picks_archive_or_delete() {
        let mut s = forum_on();
        go_to(&mut s, &ROW);
        press(&mut s, KeyCode::Enter);
        let values: Vec<String> = s.pick_choices().iter().map(|item| item.value.clone()).collect();
        let selected = s.pick_selected();

        press(&mut s, KeyCode::Down);
        let config = saved(press(&mut s, KeyCode::Enter));
        assert_eq!((values, selected), (vec!["archive".to_string(), "delete".to_string()], Some(0)));
        assert_eq!(config.discord_forum_close, "delete");
    }

    #[test]
    fn the_config_request_sets_it_and_refuses_anything_else() {
        let mut s = forum_on();

        let notice = s.apply("discord_forum_close", &json!("delete")).expect("applied");
        let refused = s.apply("discord_forum_close", &json!("burn"));
        assert_eq!(
            (s.config.discord_forum_close.as_str(), notice.as_deref()),
            ("delete", Some("closed workspaces: delete"))
        );
        assert!(matches!(refused, Err(Refused::Invalid(_))));
    }

    #[test]
    fn is_described_as_a_choice_on_the_remote_page_while_the_forum_is_on() {
        let off = with_bot(Via::Discord, Some("@ada")).describe();
        let setting = forum_on().describe().into_iter().find(|s| s.key == "discord_forum_close").expect("described");

        let place = (setting.page.as_str(), setting.section.as_str(), setting.label.as_str());
        assert_eq!((setting.kind, place), (Kind::Choice, ("Remote", "Discord", "closed workspaces")));
        assert_eq!((setting.value, setting.default), (json!("archive"), json!("archive")));
        assert!(!off.iter().any(|s| s.key == "discord_forum_close"));
    }
}
