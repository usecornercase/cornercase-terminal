use std::thread;

use super::*;

fn old_server(answer: ServerMessage) -> Result<Value> {
    let (ours, mut theirs) = UnixStream::pair().expect("a socket pair");
    let server = thread::spawn(move || {
        let asked = protocol::recv::<ClientMessage>(&mut theirs).expect("a request");
        protocol::send(&mut theirs, &answer).expect("an answer");
        asked
    });

    let answered = ask_on(ours, "contract", control::Command::Contract(control::Contract {}));

    let Some(ClientMessage::Request(text)) = server.join().expect("the fake server") else { panic!("not a request") };
    assert!(text.contains("\"contract\""), "{text}");
    answered
}

#[test]
fn a_server_without_contract_says_it_has_no_such_command() {
    let refusal = serde_json::to_string(&Response::Error(control::unknown_command("contract"))).expect("json");

    let error = old_server(ServerMessage::Response(refusal)).expect_err("refused");

    assert!(matches!(&error, Error::Control(_)), "{error:?}");
    assert!(error.to_string().contains("has no `contract` command"), "{error}");
}

#[test]
fn a_server_older_than_requests_is_too_old() {
    let error = old_server(ServerMessage::Rejected("another build".into())).expect_err("rejected");

    assert!(matches!(error, Error::OldServer("contract")), "{error:?}");
}

mod features {
    use super::*;

    fn offering(features: &[&str]) -> (Result<()>, Vec<String>) {
        asking(features, "accounts", "has no account profiles")
    }

    fn asking(features: &[&str], feature: &str, lacks: &str) -> (Result<()>, Vec<String>) {
        let (ours, mut theirs) = UnixStream::pair().expect("a socket pair");
        let mut descriptor = control::contract::describe("boot", &[]);
        descriptor.features.retain(|feature| features.contains(&feature.as_str()));
        let server = thread::spawn(move || {
            let mut asked = Vec::new();
            while let Ok(Some(ClientMessage::Request(text))) = protocol::recv::<ClientMessage>(&mut theirs) {
                asked.push(text);
                let answer = serde_json::to_string(&Response::Ok(json(&descriptor))).expect("json");
                protocol::send(&mut theirs, &ServerMessage::Response(answer)).expect("an answer");
            }
            asked
        });

        let checked = require_on(Ok(ours), feature, lacks);

        (checked, server.join().expect("the fake server"))
    }

    fn json(descriptor: &control::contract::Descriptor) -> Value {
        serde_json::to_value(descriptor).expect("a descriptor")
    }

    #[test]
    fn a_server_without_accounts_is_refused_after_asking_only_for_its_contract() {
        let (checked, asked) = offering(&["views"]);

        let error = checked.expect_err("refused");
        assert!(error.to_string().contains("has no account profiles"), "{error}");
        assert_eq!(asked.len(), 1, "{asked:?}");
        assert!(asked[0].contains("\"contract\""), "{}", asked[0]);
    }

    #[test]
    fn a_server_with_accounts_lets_the_request_go() {
        let (checked, asked) = offering(&["accounts"]);

        checked.expect("accepted");
        assert_eq!(asked.len(), 1);
    }

    #[test]
    fn a_server_without_login_checks_is_refused_before_the_list_is_asked_for() {
        let (checked, asked) = asking(&["accounts"], "account-login", "cannot check the accounts' logins");

        let error = checked.expect_err("refused");
        assert!(error.to_string().contains("cannot check the accounts' logins"), "{error}");
        assert_eq!(asked.len(), 1, "{asked:?}");
    }

    #[test]
    fn a_server_without_account_switching_is_refused_before_the_change_is_sent() {
        let (checked, asked) = asking(&["accounts"], "account-switch", "cannot rename or switch accounts");

        let error = checked.expect_err("refused");
        assert!(error.to_string().contains("cannot rename or switch accounts"), "{error}");
        assert_eq!(asked.len(), 1, "{asked:?}");
    }
}
