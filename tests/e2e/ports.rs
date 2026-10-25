use serde_json::Value;

use super::*;

const LISTEN: &str =
    "python3 -c 'import socket, time; s = socket.socket(); s.bind((\"127.0.0.1\", 0)); s.listen(); time.sleep(60)'";

fn ports(session: &Session, args: &[&str]) -> Vec<Value> {
    let listing: Value = serde_json::from_str(&session.says(&[&["ports", "--json"], args].concat())).expect("json");
    listing["ports"].as_array().expect("a list").clone()
}

#[test]
fn a_server_started_in_a_tab_is_listed_with_its_pane_and_url() {
    let app = Harness::start();
    let pane = app.session.says(&["new-tab", "--name", "server", "--", LISTEN]);
    let id: u64 = pane.parse().expect("a pane id");
    let ours = |ports: &[Value]| ports.iter().find(|p| p["pane"] == id).cloned();

    let mut found = None;
    wait_until("the server listens", || {
        found = ours(&ports(&app.session, &[]));
        found.is_some()
    });

    let found = found.expect("the port");
    let port = found["port"].as_u64().expect("a port");
    assert_eq!(found["url"], format!("http://localhost:{port}"));
    assert_eq!((found["via"].as_str(), found["addresses"][0].as_str()), (Some("pane"), Some("127.0.0.1")));
    assert!(ours(&ports(&app.session, &["--all"])).is_some());
    let text = app.session.says(&["ports"]);
    assert!(
        text.contains(&format!("http://localhost:{port}  127.0.0.1")) && text.contains(&format!("pane {pane}")),
        "{text}"
    );
}
