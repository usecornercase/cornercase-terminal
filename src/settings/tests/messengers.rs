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
