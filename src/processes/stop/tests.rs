use std::process::{Child, Command, Stdio};

use super::*;
use crate::process::escalation::Outcome;
#[cfg(unix)]
use crate::test_util::Family;
use crate::test_util::wait_until;

fn pid_of(child: &Child) -> i32 {
    i32::try_from(child.id()).expect("a pid")
}

#[cfg(unix)]
fn spawn(script: &str) -> Child {
    Command::new("/bin/sh").args(["-c", script]).stdin(Stdio::null()).spawn().expect("spawn sh")
}

fn gone(pid: i32) -> bool {
    !process::running(pid, None)
}

#[cfg(unix)]
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

#[cfg(unix)]
mod reused_pids {
    use super::*;

    #[test]
    fn a_member_without_a_start_is_never_signalled() {
        let mut root = spawn("sleep 30");
        let mut bystander = spawn("sleep 30");
        let (pid, other) = (pid_of(&root), pid_of(&bystander));
        wait_until("both run", || process::start_stamp(pid).is_some() && process::start_stamp(other).is_some());
        let blind = Member { pid: other, started: None, name: "sleep".into() };
        let group = Group { members: vec![blind], ..Group::live(pid) };
        let stopped = stop(group);
        assert!(matches!(&stopped[..], [(_, Ok(_))]), "{stopped:?}");
        assert!(bystander.try_wait().expect("check").is_none());
        let _ = (root.wait(), bystander.kill(), bystander.wait());
    }

    #[test]
    fn a_group_built_with_the_requests_start_keeps_it_and_is_refused_when_it_differs() {
        let mut child = spawn("sleep 30");
        let pid = pid_of(&child);
        wait_until("it runs", || process::start_stamp(pid).is_some());
        let group = Group::live_with(pid, process::start_stamp(pid).map(|stamp| stamp + 1));
        assert_eq!(group.root.started, process::start_stamp(pid).map(|stamp| stamp + 1));
        let stopped = stop(group);
        assert!(matches!(&stopped[..], [(_, Err(reason))] if reason.contains("no longer the program")), "{stopped:?}");
        assert!(child.try_wait().expect("check").is_none());
        let _ = (child.kill(), child.wait());
    }

    #[test]
    fn a_root_that_cannot_be_signalled_does_not_hide_the_members_that_were() {
        if rustix::process::getuid().is_root() {
            return;
        }
        let mut child = spawn("sleep 30");
        let pid = pid_of(&child);
        wait_until("it runs", || process::start_stamp(pid).is_some());
        let group = Group { root: Member::read(1), members: vec![Member::read(pid)] };
        assert_eq!(group.signal(Signal::TERM), Ok(()));
        wait_until("the member ends", || child.try_wait().expect("check").is_some());
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
        assert_eq!(said(Err("it is gone".into()), 0), "did not stop node (pid 48211): it is gone");
    }

    #[cfg(unix)]
    #[test]
    fn the_hard_step_is_named_by_its_signals() {
        let said = |outcome: Result<Outcome, String>| summary(&[(group(0), outcome)]);
        assert_eq!(said(Ok(Outcome::Killed)), "killed node (pid 48211): it ignored SIGTERM");
        assert_eq!(said(Ok(Outcome::StillRunning)), "node (pid 48211) is still running after SIGKILL");
    }

    #[cfg(windows)]
    #[test]
    fn the_hard_step_names_no_signal() {
        let said = |outcome: Result<Outcome, String>| summary(&[(group(0), outcome)]);
        assert_eq!(
            said(Ok(Outcome::Killed)),
            "killed node (pid 48211): it had no window to close, or did not close within 3 s"
        );
        assert_eq!(said(Ok(Outcome::StillRunning)), "node (pid 48211) is still running after being terminated");
    }
}

#[cfg(unix)]
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

#[cfg(windows)]
mod on_windows {
    use std::time::{Duration, Instant};

    use super::*;
    use crate::test_util::{Running, wait_until_within};

    const PING: &str = r"%SystemRoot%\System32\ping.exe -n 30 127.0.0.1";

    const STARTS_WITHIN: Duration = Duration::from_secs(30);

    const FORM: &str = "Add-Type -AssemblyName System.Windows.Forms; \
                        $form = New-Object System.Windows.Forms.Form; $form.StartPosition = 'Manual'; \
                        $form.Location = New-Object System.Drawing.Point(-30000, -30000); ";

    const KEEP_OPEN: &str = "$form.Add_FormClosing({ $_.Cancel = $true }); ";

    const SHOW: &str = "[System.Windows.Forms.Application]::Run($form)";

    fn quiet(command: &mut Command) -> Child {
        command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().expect("spawn")
    }

    fn pings(pid: i32) -> usize {
        let named = |pid: &i32| process::name(*pid).is_some_and(|name| name.eq_ignore_ascii_case("ping"));
        process::descendants(pid).into_iter().filter(named).count()
    }

    fn window(script: &str) -> Running {
        let mut shell = Command::new("powershell.exe");
        shell.args(["-NoProfile", "-NonInteractive", "-Command", script]);
        let form = Running::spawn(shell.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()));
        wait_until_within("the window shows", Duration::from_secs(60), || askable(form.pid()));
        form
    }

    #[test]
    fn a_tree_without_a_window_is_ended_at_once_with_everything_it_started() {
        let mut root = quiet(Command::new("cmd.exe").args(["/d", "/c", &format!("start /b {PING} & {PING}")]));
        let pid = pid_of(&root);
        wait_until_within("cmd starts both pings", STARTS_WITHIN, || pings(pid) == 2);
        let tree: Vec<i32> = std::iter::once(pid).chain(process::descendants(pid)).collect();
        let started = Instant::now();

        let stopped = stop(Group::live(pid));

        assert!(matches!(&stopped[..], [(group, Ok(Outcome::Killed))] if group.size() == tree.len()), "{stopped:?}");
        assert!(started.elapsed() < Duration::from_secs(3), "{:?}", started.elapsed());
        wait_until("the whole tree is gone", || tree.iter().all(|pid| gone(*pid)));
        root.wait().expect("reap");
    }

    #[test]
    fn a_program_whose_window_closes_when_asked_is_stopped() {
        let form = window(&format!("{FORM}{SHOW}"));

        let stopped = stop(Group::live(form.pid()));

        assert!(matches!(&stopped[..], [(_, Ok(Outcome::Stopped))]), "{stopped:?}");
        assert!(gone(form.pid()));
    }

    #[test]
    fn a_program_that_keeps_its_window_open_is_ended_after_the_wait() {
        let form = window(&format!("{FORM}{KEEP_OPEN}{SHOW}"));
        let started = Instant::now();

        let stopped = stop(Group::live(form.pid()));

        assert!(matches!(&stopped[..], [(_, Ok(Outcome::Killed))]), "{stopped:?}");
        assert!(started.elapsed() >= Duration::from_secs(3), "{:?}", started.elapsed());
        assert!(gone(form.pid()));
    }
}
