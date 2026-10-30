use cornercase::version;
use serde_json::{Value, json};

use super::remote::{boot_of, cornercase_in, fake_ssh_to, gone, ssh_pid_to};
use super::*;

const VPS: &str = "vps";

fn reach(machine: &Session) -> String {
    format!("env {}", machine.socket_var())
}

fn folder(session: &Session, name: &str) -> PathBuf {
    let dir = session.dir.join(name);
    std::fs::create_dir_all(&dir).expect("create a folder");
    dir
}

fn here_with(listed: &[&str], reached: &[(&str, &str)]) -> Arc<Session> {
    let named: Vec<(&str, &str)> = listed.iter().map(|name| (*name, *name)).collect();
    here_listing(&named, reached)
}

fn here_listing(listed: &[(&str, &str)], reached: &[(&str, &str)]) -> Arc<Session> {
    let here = Session::new();
    let machines: Vec<Value> = listed
        .iter()
        .map(|(name, destination)| json!({"name": name, "icon": "☁", "destination": destination, "folder": "~/code"}))
        .collect();
    std::fs::write(here.dir.join("config.json"), json!({"machines": machines}).to_string()).expect("write the config");
    let path = fake_ssh_to(&here.dir, reached);
    let started = here.cold_command().env("PATH", path).arg("start-server").output().expect("run cornercase");
    assert!(started.status.success(), "{started:?}");
    here
}

fn linked_to_one() -> (Arc<Session>, Arc<Session>) {
    let machine = Session::new();
    let here = here_with(&[VPS], &[(VPS, &reach(&machine))]);
    wait_for_machine(&here, VPS, "the machine is linked", in_state("linked"));
    (machine, here)
}

fn machines(here: &Session) -> Vec<Value> {
    let list: Value = serde_json::from_str(&here.says(&["machines", "--json"])).expect("a machine list");
    list["machines"].as_array().cloned().unwrap_or_default()
}

fn wait_for_machine(here: &Session, name: &str, what: &str, cond: impl Fn(&Value) -> bool) -> Value {
    let mut machine = Value::Null;
    wait_until(what, || {
        machine = machines(here).into_iter().find(|m| m["name"] == name).unwrap_or_default();
        cond(&machine)
    });
    machine
}

fn in_state(state: &'static str) -> impl Fn(&Value) -> bool {
    move |machine| machine["state"] == state
}

fn with_state<'a>(list: &'a [Value], state: &str) -> Option<&'a Value> {
    list.iter().find(|machine| machine["state"] == state)
}

fn name_of(machine: &Value) -> &str {
    machine["name"].as_str().expect("a name")
}

fn boot(session: &Session) -> Value {
    boot_of(&session.says(&["status", "--json"]))
}

#[rstest]
#[case::with_no_server_there(false)]
#[case::with_its_server_running(true)]
fn a_machine_is_linked_to_its_one_server(#[case] running: bool) {
    let machine = Session::new();
    let before = running.then(|| {
        assert!(machine.cold(&["start-server"]).status.success(), "the server there starts");
        boot(&machine)
    });
    let here = here_with(&[VPS], &[(VPS, &reach(&machine))]);

    let vps = wait_for_machine(&here, VPS, "the machine is linked", in_state("linked"));

    assert_eq!(
        (vps["version"].as_str(), vps["server_version"].as_str()),
        (Some(version::CURRENT), Some(version::CURRENT))
    );
    assert_eq!(machine.servers(), 1);
    assert_eq!(before.unwrap_or_else(|| boot(&machine)), boot(&machine));
}

#[test]
fn a_dropped_link_comes_back_to_the_same_server() {
    let (machine, here) = linked_to_one();
    let (kept, first) = (boot(&machine), ssh_pid_to(&here, VPS));

    let killed = std::process::Command::new("kill").args(["-9", &first]).status().expect("run kill");

    assert!(killed.success(), "kill failed");
    wait_until("a new ssh", || ssh_pid_to(&here, VPS) != first);
    wait_for_machine(&here, VPS, "the machine is linked again", in_state("linked"));
    here.wait_for_file("server.log", "the drop is logged", |log| {
        log.contains("machine unreachable name=vps failed=1 ")
    });
    assert_eq!((boot(&machine), machine.servers()), (kept, 1));
}

#[test]
fn a_machine_without_cornercase_says_so_and_links_on_retry_once_it_is_there() {
    let machine = Session::new();
    let there = folder(&machine, "there");
    let home = folder(&machine, "home");
    let prefix = format!("{} HOME={} PATH={}:/usr/bin:/bin", reach(&machine), home.display(), there.display());
    let here = here_with(&[VPS], &[(VPS, &prefix)]);
    let missing = wait_for_machine(&here, VPS, "cornercase is not found there", in_state("unreachable"));
    assert_eq!(missing["error"], "cornercase was not found on vps: install it there");

    cornercase_in(&there);
    here.says(&["machines", "retry", VPS]);

    wait_for_machine(&here, VPS, "the machine is linked well before its next try", in_state("linked"));
    assert_eq!(machine.servers(), 1);
}

#[test]
fn a_machine_that_is_down_does_not_hold_up_another() {
    let machine = Session::new();
    let here = here_with(&["down", VPS], &[(VPS, &reach(&machine))]);

    let down = wait_for_machine(&here, "down", "ssh fails", in_state("unreachable"));

    assert_eq!(down["error"], "ssh: connect to host down port 22: Connection refused");
    wait_for_machine(&here, VPS, "the other machine is linked", in_state("linked"));
}

#[rstest]
#[case::removing_the_machine(&["machines", "remove", "vps"])]
#[case::changing_its_destination(&["machines", "edit", "vps", "--destination", "pi"])]
#[case::stopping_the_server_here(&["kill-server"])]
fn ending_a_link_leaves_the_server_there_running(#[case] ending: &[&str]) {
    let (machine, here) = linked_to_one();
    let (kept, ssh) = (boot(&machine), ssh_pid_to(&here, VPS));

    here.says(ending);

    wait_until("the link's ssh ends", || gone(&ssh));
    assert_eq!((boot(&machine), machine.servers()), (kept, 1));
}

#[test]
fn editing_a_machine_but_its_destination_keeps_its_link() {
    let (_machine, here) = linked_to_one();
    let ssh = ssh_pid_to(&here, VPS);

    here.says(&["machines", "edit", VPS, "--name", "box", "--icon", "laptop", "--folder", "/srv"]);
    here.says(&["machines", "add", "pi", "pi"]);

    wait_until("the machine added next gets its link", || !ssh_pid_to(&here, "pi").is_empty());
    let kept = wait_for_machine(&here, "box", "the machine is renamed", in_state("linked"));
    assert_eq!((ssh_pid_to(&here, VPS), kept["icon"].as_str(), gone(&ssh)), (ssh, Some("▭"), false));
}

#[test]
fn a_machine_that_is_this_computer_is_not_linked() {
    let here = here_with(&["self"], &[("self", "")]);

    let itself = wait_for_machine(&here, "self", "the link finds this server", in_state("unreachable"));

    assert_eq!(itself["error"], "that is this computer");
    wait_until("its ssh ends", || gone(&ssh_pid_to(&here, "self")));
}

#[test]
fn two_machines_on_one_server_are_linked_once() {
    let machine = Session::new();
    let there = reach(&machine);
    let here = here_with(&[VPS, "box"], &[(VPS, &there), ("box", &there)]);

    let mut list = Vec::new();
    wait_until("one of them is refused", || {
        list = machines(&here);
        with_state(&list, "linked").is_some() && with_state(&list, "unreachable").is_some()
    });

    let (linked, refused) =
        (with_state(&list, "linked").expect("linked"), with_state(&list, "unreachable").expect("refused"));
    assert_eq!(refused["error"], format!("the same server as {}", name_of(linked)));
    wait_until("the refused link's ssh ends", || gone(&ssh_pid_to(&here, name_of(refused))));
    assert_eq!(machine.servers(), 1);
}

#[test]
fn an_older_machine_is_updated_there_and_then_linked() {
    let machine = Session::new();
    let old = folder(&machine, "old");
    let script = format!(
        "#!/bin/sh\necho \"$*\" >> '{old}/args'\nif [ -e '{old}/updated' ]; then exec '{bin}' \"$@\"; fi\n\
         if [ \"$1\" = update ]; then : > '{old}/updated'; exit 0; fi\necho 'cornercase-proxy 1 0.0.1'\nexec cat > /dev/null\n",
        old = old.display(),
        bin = bin()
    );
    write_executable(&old.join("cornercase"), &script);
    let here = here_with(&[VPS], &[(VPS, &format!("{} PATH={}:/usr/bin:/bin", reach(&machine), old.display()))]);
    let older = wait_for_machine(&here, VPS, "the machine is older", in_state("older"));
    assert_eq!(older["version"], "0.0.1");

    here.says(&["machines", "update", VPS]);

    wait_for_machine(&here, VPS, "the machine is linked at once", in_state("linked"));
    let ran = std::fs::read_to_string(old.join("args")).expect("the arguments it got");
    assert_eq!(ran.lines().filter(|args| *args != "proxy").collect::<Vec<_>>(), ["update"], "{ran}");
}

#[test]
fn an_update_that_fails_there_is_logged_by_the_machine_s_name_only() {
    let destination = "ana@vps.example.com";
    let machine = Session::new();
    let old = folder(&machine, "old");
    let script = "#!/bin/sh\nif [ \"$1\" = update ]; then echo 'cannot reach vps.example.com port 443' >&2; exit 1; fi\n\
                  echo 'cornercase-proxy 1 0.0.1'\nexec cat > /dev/null\n";
    write_executable(&old.join("cornercase"), script);
    let prefix = format!("{} PATH={}:/usr/bin:/bin", reach(&machine), old.display());
    let here = here_listing(&[(VPS, destination)], &[(destination, &prefix)]);
    wait_for_machine(&here, VPS, "the machine is older", in_state("older"));

    let failed = here.cli(&["machines", "update", VPS]);

    assert!(!failed.status.success(), "{failed:?}");
    here.wait_for_file("server.log", "the failed update is logged", |log| log.contains("update failed"));
    let log = std::fs::read_to_string(here.dir.join("server.log")).expect("read the log");
    assert!(log.contains("cannot reach vps port 443") && !log.contains("vps.example.com"), "{log}");
}
