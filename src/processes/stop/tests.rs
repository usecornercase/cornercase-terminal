use std::process::{Child, Command, Stdio};

use super::*;
use crate::process::escalation::Outcome;
use crate::test_util::{Family, wait_until};

fn pid_of(child: &Child) -> i32 {
    i32::try_from(child.id()).expect("a pid")
}

fn spawn(script: &str) -> Child {
    Command::new("/bin/sh").args(["-c", script]).stdin(Stdio::null()).spawn().expect("spawn sh")
}

fn gone(pid: i32) -> bool {
    !process::running(pid, None)
}

mod stopping {
    use super::*;

    #[test]
    fn a_program_ends_on_sigterm_with_everything_it_started() {
        let family = Family::new();
        let (child, grandchild) = (family.child().expect("child"), family.grandchild().expect("grandchild"));
        let stopped = stop(Group::live(child));
        assert!(matches!(&stopped[..], [(_, Ok(Outcome::Stopped))]), "{stopped:?}");
        wait_until("both end", || gone(child) && gone(grandchild));
    }

    #[test]
    fn a_program_that_ignores_sigterm_is_killed() {
        let mut child = spawn("trap '' TERM; while :; do sleep 1; done");
        let pid = pid_of(&child);
        wait_until("the trap is set", || process::name(pid).is_some() && !process::children(pid).is_empty());
        let stopped = stop(Group::live(pid));
        assert!(matches!(&stopped[..], [(_, Ok(Outcome::Killed))]), "{stopped:?}");
        child.wait().expect("reap");
    }

    #[test]
    fn a_changed_start_is_left_alone() {
        let mut child = spawn("sleep 30");
        let pid = pid_of(&child);
        wait_until("it runs", || process::start_stamp(pid).is_some());
        let mut group = Group::live(pid);
        group.root.started = group.root.started.map(|stamp| stamp + 1);
        let stopped = stop(group);
        assert!(matches!(&stopped[..], [(_, Err(reason))] if reason.contains("no longer the program")), "{stopped:?}");
        assert!(child.try_wait().expect("check").is_none());
        child.kill().expect("kill");
        child.wait().expect("reap");
    }

    #[test]
    fn a_pid_that_is_gone_is_not_running() {
        let mut child = spawn("exit 0");
        let pid = pid_of(&child);
        child.wait().expect("reap");
        let stopped = stop(Group { root: Member { pid, started: None, name: "sh".into() }, members: vec![] });
        assert!(matches!(&stopped[..], [(_, Err(reason))] if reason.contains("no longer running")), "{stopped:?}");
    }

    #[test]
    fn a_child_started_after_the_confirmation_is_stopped_too() {
        let mut shell = spawn("sleep 30 & wait");
        let pid = pid_of(&shell);
        wait_until("the first child", || !process::children(pid).is_empty());
        let group = Group { root: Member::read(pid), members: vec![] };
        let late = process::children(pid)[0];
        let stopped = stop(group);
        assert!(matches!(&stopped[..], [(_, Ok(_))]), "{stopped:?}");
        wait_until("the late child ends", || gone(late));
        shell.wait().expect("reap");
    }
}

mod saying {
    use super::*;

    fn group(members: usize) -> Group {
        let member = |pid| Member { pid, started: None, name: "esbuild".into() };
        Group {
            root: Member { pid: 48211, started: None, name: "node".into() },
            members: (0..members).map(|i| member(i32::try_from(i).unwrap_or(0) + 1)).collect(),
        }
    }

    #[test]
    fn each_outcome_names_the_program_and_what_it_started() {
        let said = |outcome: Result<Outcome, String>, members| summary(&[(group(members), outcome)]);
        assert_eq!(said(Ok(Outcome::Stopped), 2), "stopped node (pid 48211) and 2 programs it started");
        assert_eq!(said(Ok(Outcome::Stopped), 1), "stopped node (pid 48211) and 1 program it started");
        assert_eq!(said(Ok(Outcome::Stopped), 0), "stopped node (pid 48211)");
        assert_eq!(said(Ok(Outcome::Killed), 0), "killed node (pid 48211): it ignored SIGTERM");
        assert_eq!(said(Ok(Outcome::StillRunning), 0), "node (pid 48211) is still running after SIGKILL");
        assert_eq!(said(Err("it is gone".into()), 0), "did not stop node (pid 48211): it is gone");
    }
}

mod inside {
    use super::*;

    #[test]
    fn follows_the_parents_up_to_the_root() {
        let family = Family::new();
        let grandchild = family.grandchild().expect("grandchild");
        assert!(inside(grandchild, family.pid()));
        assert!(!inside(family.pid(), grandchild));
    }
}
