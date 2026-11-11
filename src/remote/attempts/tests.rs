use std::cell::RefCell;
use std::collections::VecDeque;

use rstest::rstest;

use super::*;
use crate::remote::tests::{exited, with};

use RemoteShell::{Cmd, Posix, PowerShell};

#[derive(Default)]
struct Far {
    remembered: Option<RemoteShell>,
    answers: VecDeque<Option<RemoteShell>>,
    probes: usize,
    learned: Vec<RemoteShell>,
}

impl Far {
    fn remembering(shell: RemoteShell) -> Self {
        Self { remembered: Some(shell), ..Self::default() }
    }

    fn answering(mut self, answers: &[Option<RemoteShell>]) -> Self {
        self.answers = answers.iter().copied().collect();
        self
    }
}

impl Ask for Far {
    fn remembered(&mut self) -> Option<RemoteShell> {
        self.remembered
    }

    fn probe(&mut self) -> Option<RemoteShell> {
        self.probes += 1;
        self.answers.pop_front().flatten()
    }

    fn learn(&mut self, shell: RemoteShell) {
        self.learned.push(shell);
    }
}

fn failed(code: i32, lines: &[&str]) -> Failed {
    Failed { status: Some(exited(code)), stderr: with(lines) }
}

struct There {
    links: Vec<RemoteShell>,
    code: i32,
    tried: RefCell<Vec<RemoteShell>>,
}

impl There {
    fn linking(links: &[RemoteShell], code: i32) -> Self {
        Self { links: links.to_vec(), code, tried: RefCell::new(Vec::new()) }
    }

    fn attempt(&self, shell: RemoteShell) -> Attempt<RemoteShell> {
        self.tried.borrow_mut().push(shell);
        if self.links.contains(&shell) { Attempt::Linked(shell) } else { Attempt::Failed(failed(self.code, &[])) }
    }

    fn run(&self, shell: RemoteShell) -> ExitStatus {
        self.tried.borrow_mut().push(shell);
        exited(if self.links.contains(&shell) { 0 } else { self.code })
    }

    fn tried(&self) -> Vec<RemoteShell> {
        self.tried.borrow().clone()
    }
}

fn link(far: &mut Far, there: &There, probe_first: bool) -> Option<RemoteShell> {
    linked(far, probe_first, |shell| Ok(there.attempt(shell))).expect("no hard error").ok()
}

mod failures {
    use super::*;

    #[rstest]
    #[case::cmd_finds_no_sh(1, &["\"sh\" no se reconoce como un comando interno o externo,"], true)]
    #[case::powershell_finds_no_sh(1, &["sh : The term 'sh' is not recognized as the name of a cmdlet"], true)]
    #[case::cornercase_missing_under_sh(127, &["sh: 1: exec: cornercase: not found"], false)]
    #[case::a_missing_path_under_bash(126, &["sh: line 0: exec: /x: cannot execute: No such file or directory"], false)]
    #[case::ssh_itself(255, &["ssh: connect to host devbox port 22: Connection refused"], false)]
    #[case::an_old_cornercase(2, &["error: unrecognized subcommand 'proxy'"], false)]
    #[case::a_clean_end(0, &[], false)]
    fn asks_which_shell_only_when_sh_may_not_be_there(#[case] code: i32, #[case] lines: &[&str], #[case] asks: bool) {
        assert_eq!(failed(code, lines).maybe_another_shell(), asks);
    }

    #[test]
    fn a_signal_never_asks() {
        assert!(!Failed { status: None, stderr: Stderr::default() }.maybe_another_shell());
    }
}

mod connecting {
    use super::*;

    #[test]
    fn a_linux_machine_links_at_once_and_is_never_asked() {
        let (mut far, there) = (Far::default(), There::linking(&[Posix], 1));

        assert_eq!(link(&mut far, &there, false), Some(Posix));
        assert_eq!((far.probes, there.tried(), far.learned.len()), (0, vec![Posix], 0));
    }

    #[test]
    fn a_windows_machine_is_asked_once_and_remembered() {
        let (mut far, there) = (Far::default().answering(&[Some(Cmd)]), There::linking(&[Cmd], 1));

        assert_eq!(link(&mut far, &there, false), Some(Cmd));
        assert_eq!((far.probes, there.tried(), far.learned), (1, vec![Posix, Cmd], vec![Cmd]));
    }

    #[test]
    fn cornercase_missing_under_sh_is_not_asked_about() {
        let (mut far, there) = (Far::default(), There::linking(&[], NOT_FOUND));

        assert!(link(&mut far, &there, false).is_none());
        assert_eq!((far.probes, there.tried()), (0, vec![Posix]));
    }

    #[test]
    fn an_answer_of_sh_keeps_the_first_failure() {
        let (mut far, there) = (Far::default().answering(&[Some(Posix)]), There::linking(&[], 1));

        assert!(link(&mut far, &there, false).is_none());
        assert_eq!((far.probes, there.tried()), (1, vec![Posix]));
    }

    #[test]
    fn no_answer_keeps_the_first_failure() {
        let (mut far, there) = (Far::default().answering(&[None]), There::linking(&[], 1));

        assert!(link(&mut far, &there, false).is_none());
        assert_eq!((there.tried(), far.learned.len()), (vec![Posix], 0));
    }

    #[rstest]
    #[case::the_lines_for_windows_fail_under_sh(NOT_FOUND)]
    #[case::cornercase_refuses_a_stray_word(2)]
    #[case::a_parse_error(1)]
    fn a_remembered_shell_that_fails_is_asked_again(#[case] code: i32) {
        let (mut far, there) = (Far::remembering(Cmd).answering(&[Some(Posix)]), There::linking(&[Posix], code));

        assert_eq!(link(&mut far, &there, false), Some(Posix));
        assert_eq!((there.tried(), far.learned), (vec![Cmd, Posix], vec![Posix]));
    }

    #[test]
    fn a_remembered_shell_that_still_answers_keeps_its_failure() {
        let (mut far, there) = (Far::remembering(Cmd).answering(&[Some(Cmd)]), There::linking(&[], NOT_FOUND));

        assert!(link(&mut far, &there, false).is_none());
        assert_eq!(there.tried(), vec![Cmd]);
    }

    #[test]
    fn a_remembered_shell_is_not_asked_about_when_ssh_failed() {
        let (mut far, there) = (Far::remembering(Cmd), There::linking(&[], SSH_FAILED));

        assert!(link(&mut far, &there, false).is_none());
        assert_eq!(far.probes, 0);
    }

    #[test]
    fn a_windows_command_asks_first_and_never_tries_sh() {
        let (mut far, there) = (Far::default().answering(&[Some(PowerShell)]), There::linking(&[PowerShell], 1));

        assert_eq!(link(&mut far, &there, true), Some(PowerShell));
        assert_eq!((far.probes, there.tried()), (1, vec![PowerShell]));
    }

    #[test]
    fn a_shell_just_asked_is_not_asked_again() {
        let (mut far, there) = (Far::default().answering(&[Some(Cmd), Some(Posix)]), There::linking(&[], 1));

        assert!(link(&mut far, &there, true).is_none());
        assert_eq!((far.probes, there.tried()), (1, vec![Cmd]));
    }
}

mod running {
    use super::*;

    fn run(far: &mut Far, there: &There, args: &[&str]) -> Ran {
        ran(far, false, args, |shell| Ok(there.run(shell))).expect("no hard error")
    }

    #[test]
    fn a_success_is_never_asked_about() {
        let (mut far, there) = (Far::default(), There::linking(&[Posix], 1));

        let ran = run(&mut far, &there, &["kill-server"]);

        assert!(ran.status.success() && ran.learned.is_none() && far.probes == 0);
    }

    #[rstest]
    #[case::restart(&["restart", "--when-idle"])]
    #[case::plugin_enable(&["plugin", "enable", "--", "acme.notes"])]
    #[case::plugin_install(&["plugin", "install", "--yes", "--", "acme/notes"])]
    #[case::a_preview_that_fetches(&["plugin", "install", "--preview", "--json", "--", "acme/notes"])]
    fn a_command_that_may_have_changed_something_is_not_run_again(#[case] args: &[&str]) {
        let (mut far, there) = (Far::default().answering(&[Some(Cmd)]), There::linking(&[Cmd], 1));

        let ran = run(&mut far, &there, args);

        assert_eq!((ran.status.code(), ran.learned, ran.again), (Some(1), Some(Cmd), false));
        assert_eq!((there.tried(), far.learned), (vec![Posix], vec![Cmd]));
    }

    #[rstest]
    #[case::logs(&["logs", "--path"])]
    #[case::plugin_list(&["plugin", "list", "--json"])]
    #[case::plugin_logs(&["plugin", "logs", "--lines", "200", "--", "acme.notes"])]
    fn reading_is_run_again_through_the_right_shell(#[case] args: &[&str]) {
        let (mut far, there) = (Far::default().answering(&[Some(Cmd)]), There::linking(&[Cmd], 1));

        let ran = run(&mut far, &there, args);

        assert!(ran.status.success() && ran.again);
        assert_eq!(there.tried(), vec![Posix, Cmd]);
    }

    #[test]
    fn only_status_one_asks_about_a_shell_not_yet_known() {
        let (mut far, there) = (Far::default(), There::linking(&[], 2));

        run(&mut far, &there, &["logs"]);

        assert_eq!(far.probes, 0);
    }

    #[test]
    fn a_remembered_shell_that_fails_is_asked_again() {
        let (mut far, there) = (Far::remembering(PowerShell).answering(&[Some(Posix)]), There::linking(&[Posix], 127));

        let ran = run(&mut far, &there, &["logs"]);

        assert!(ran.status.success() && ran.learned == Some(Posix));
        assert_eq!((there.tried(), far.learned), (vec![PowerShell, Posix], vec![Posix]));
    }
}

mod windows {
    use super::*;

    #[test]
    fn a_remembered_shell_answers_without_asking() {
        let mut far = Far::remembering(Cmd);

        assert!(windows_there(&mut far));
        assert_eq!(far.probes, 0);
    }

    #[rstest]
    #[case::cmd(Some(Cmd), true)]
    #[case::powershell(Some(PowerShell), true)]
    #[case::sh(Some(Posix), false)]
    #[case::no_answer(None, false)]
    fn a_machine_not_yet_known_is_asked(#[case] answer: Option<RemoteShell>, #[case] windows: bool) {
        let mut far = Far::default().answering(&[answer]);

        assert_eq!(windows_there(&mut far), windows);
        assert_eq!((far.probes, far.learned), (1, answer.into_iter().collect::<Vec<_>>()));
    }
}
