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
