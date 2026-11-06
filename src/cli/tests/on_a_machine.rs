use rstest::rstest;

use super::parse;
use crate::cli::args::{Command, Control, FlowAction, StartArgs};
use crate::cli::unwaited;
use crate::error::Error;
use crate::machines::send::UNWAITED;

fn machine_of(args: &[&str]) -> Option<String> {
    let on = match parse(args).expect("parse").command {
        Some(Command::Control(Control::Start(start))) => start.on,
        Some(Command::Control(Control::NewWorkspace(new))) => new.on,
        Some(Command::Control(Control::Flow(flow))) => {
            let FlowAction::Run(run) = flow.action else { panic!("not a flow run") };
            run.on
        }
        _ => panic!("not a start"),
    };
    on.machine
}

pub(super) fn start(args: &[&str]) -> StartArgs {
    let Some(Command::Control(Control::Start(start))) = parse(args).expect("parse").command else {
        panic!("not a start")
    };
    *start
}

#[rstest]
#[case::start(&["start", "claude", "--project", "2"])]
#[case::new_workspace(&["new-workspace", "web", "--project", "2"])]
#[case::flow_run(&["flow", "run", "Quick", "--prompt", "fix it"])]
fn a_start_takes_the_machine_it_runs_on(#[case] args: &[&str]) {
    let on = [args, &["--machine", "vps"]].concat();

    assert_eq!((machine_of(&on), machine_of(args)), (Some("vps".into()), None));
}

#[rstest]
#[case::start(&["start", "claude", "--machine", ""])]
#[case::new_workspace(&["new-workspace", "web", "--machine", ""])]
#[case::flow_run(&["flow", "run", "Quick", "--prompt", "fix it", "--machine", ""])]
fn an_empty_machine_exits_with_2(#[case] args: &[&str]) {
    assert_eq!(parse(args).expect_err("wrong usage").exit_code(), 2);
}

#[rstest]
#[case::wait(&["--wait"])]
#[case::until(&["--wait", "--until", "idle"])]
#[case::timeout(&["--timeout", "30"])]
fn a_start_there_refuses_to_wait_as_the_server_does(#[case] waits: &[&str]) {
    let args = [&["start", "claude", "--machine", "vps"], waits].concat();

    let refused = unwaited(&start(&args)).expect_err("a refusal");

    assert!(matches!(&refused, Error::WrongUsage(why) if why == UNWAITED), "{refused:?}");
}

#[rstest]
#[case::waiting_here(&["start", "claude", "--wait", "--timeout", "30"])]
#[case::not_waiting_there(&["start", "claude", "--machine", "vps"])]
fn a_start_here_or_without_waiting_goes_on(#[case] args: &[&str]) {
    assert!(unwaited(&start(args)).is_ok());
}
