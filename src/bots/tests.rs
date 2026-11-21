use rstest::rstest;

use super::*;

fn bot(id: &str, name: &str) -> Bot {
    sample(id, name)
}

mod names {
    use super::*;

    #[rstest]
    #[case::empty("")]
    #[case::forty_one(&"a".repeat(41))]
    #[case::control("a\u{7}b")]
    #[case::newline("a\nb")]
    #[case::duplicate_in_another_case("PRS")]
    fn are_refused(#[case] name: &str) {
        let taken = bot("prs", "prs");
        assert!(name_refusal(name, &[&taken]).is_some(), "{name:?}");
    }

    #[rstest]
    #[case::forty(&"a".repeat(40))]
    #[case::forty_accents(&"é".repeat(40))]
    #[case::spaces("Review my PRs")]
    fn are_taken(#[case] name: &str) {
        assert_eq!(name_refusal(name, &[]), None);
    }
}

mod ids {
    use super::*;

    #[rstest]
    #[case::words("PRs review", "prs-review")]
    #[case::nothing_to_keep("!!!", "bot")]
    fn are_a_slug_of_the_name(#[case] name: &str, #[case] id: &str) {
        assert_eq!(new_id(name, &[], &|_| false), id);
    }

    #[test]
    fn a_folder_already_on_disk_counts_as_taken() {
        assert_eq!(new_id("prs", &[], &|id| id == "prs"), "prs-2");
    }

    #[test]
    fn a_taken_one_gets_a_number() {
        let (first, second) = (bot("prs", "PRs"), bot("prs-2", "Other"));
        assert_eq!(new_id("prs", &[&first], &|_| false), "prs-2");
        assert_eq!(new_id("prs", &[&first, &second], &|_| false), "prs-3");
    }
}

#[test]
fn a_bot_is_found_by_its_id_and_by_its_name_in_any_case() {
    let bot = bot("prs-review", "PRs Review");
    let all = [&bot];
    assert!(find(&all, "prs-review").is_some());
    assert!(find(&all, "prs review").is_some());
    assert!(find(&all, "other").is_none());
}

#[test]
fn there_can_be_fifty_bots() {
    assert_eq!(MAX_BOTS, 50);
}

#[test]
fn the_server_stops_name_the_commands_and_what_they_ask() {
    assert_eq!(stop_commands(), ["kill-server", "restart", "update"]);
    for asked in ["kill-server", "restart", "restart-when-idle", "update-install"] {
        assert!(stops_server(asked), "{asked}");
    }
    for asked in ["update-check", "status", "account-restart", "start-server"] {
        assert!(!stops_server(asked), "{asked}");
    }
}

#[test]
fn a_bot_without_follow_reads_as_following_and_is_written_as_before() {
    let text = serde_json::to_string(&sample("prs", "PRs")).expect("json");
    assert!(!text.contains("follow"), "{text}");
    let old = text.replace(",\"conversation\":null", "");
    let read: Bot = serde_json::from_str(&old).expect("an old bot.json");
    assert!(read.follow);
    let off = serde_json::to_string(&Bot { follow: false, ..sample("prs", "PRs") }).expect("json");
    assert!(off.contains("\"follow\":false"), "{off}");
    let back: Bot = serde_json::from_str(&off).expect("read back");
    assert!(!back.follow);
}

#[test]
fn a_bot_without_chat_apps_reads_as_away_and_is_written_back_unchanged() {
    let text = serde_json::to_string(&sample("prs", "PRs")).expect("json");
    assert!(!text.contains("chat_apps"), "{text}");
    let read: Bot = serde_json::from_str(&text).expect("an old bot.json");
    assert_eq!(read.chat_apps, ChatApps::Away);
    assert_eq!(serde_json::to_string(&read).expect("json"), text);
    let always = serde_json::to_string(&Bot { chat_apps: ChatApps::Always, ..sample("prs", "PRs") }).expect("json");
    assert!(always.contains("\"chat_apps\":\"always\""), "{always}");
}

#[rstest]
#[case::away("away", Some(ChatApps::Away))]
#[case::always(" always ", Some(ChatApps::Always))]
#[case::off("off", Some(ChatApps::Off))]
#[case::other("sometimes", None)]
fn the_chat_apps_option_is_read_by_its_name(#[case] text: &str, #[case] wanted: Option<ChatApps>) {
    assert_eq!(ChatApps::parse(text), wanted);
    assert_eq!(wanted.map(ChatApps::name), wanted.map(|_| text.trim()));
}
