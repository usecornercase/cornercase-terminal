use std::time::{Duration, Instant};

use super::*;
use crate::process;
use crate::test_util::{Family, Running, wait_until};

fn times(at: Instant, used: &[((i32, Option<u64>), u64)]) -> Times {
    Times { at: Some(at), cpu: used.iter().map(|(key, millis)| (*key, Duration::from_millis(*millis))).collect() }
}

mod cpu {
    use super::*;

    #[test]
    fn one_second_of_cpu_over_two_seconds_is_fifty_percent() {
        let start = Instant::now();
        let before = times(start, &[((7, Some(1)), 1000)]);
        let now = times(start + Duration::from_secs(2), &[((7, Some(1)), 2000)]);
        assert_eq!(cpu_percent(&before, &now, (7, Some(1))), Some(50.0));
    }

    #[test]
    fn a_process_born_in_the_interval_counts_all_its_time() {
        let start = Instant::now();
        let before = times(start, &[]);
        let now = times(start + Duration::from_secs(1), &[((9, Some(5)), 500)]);
        assert_eq!(cpu_percent(&before, &now, (9, Some(5))), Some(50.0));
    }

    #[test]
    fn a_reused_pid_with_another_start_is_a_new_process() {
        let start = Instant::now();
        let before = times(start, &[((9, Some(5)), 9000)]);
        let now = times(start + Duration::from_secs(1), &[((9, Some(6)), 100)]);
        assert_eq!(cpu_percent(&before, &now, (9, Some(6))), Some(10.0));
    }

    #[test]
    fn several_cores_go_over_a_hundred_percent() {
        let start = Instant::now();
        let before = times(start, &[((9, None), 0)]);
        let now = times(start + Duration::from_secs(1), &[((9, None), 3000)]);
        assert_eq!(cpu_percent(&before, &now, (9, None)), Some(300.0));
    }

    #[test]
    fn nothing_is_known_without_an_earlier_sample_or_a_reading() {
        let now = times(Instant::now(), &[]);
        assert_eq!(cpu_percent(&Times::default(), &now, (1, None)), None);
        assert_eq!(cpu_percent(&now, &now, (1, None)), None);
    }
}

mod sums {
    use super::*;

    fn proc(cpu: Option<f64>, memory: Option<u64>) -> Proc {
        Proc { pid: 1, parent: None, depth: 0, name: "x".into(), args: vec![], started: None, cpu, memory }
    }

    #[test]
    fn add_what_is_known_and_skip_what_is_not() {
        let procs = [proc(Some(1.5), Some(10)), proc(None, None), proc(Some(2.0), Some(5))];
        assert_eq!((sum_cpu(&procs), sum_memory(&procs)), (3.5, 15));
    }
}

mod sampling {
    use super::*;

    fn me() -> i32 {
        crate::test_util::this_pid()
    }

    #[test]
    fn lists_a_busy_grandchild_under_its_panes_shell_with_cpu() {
        let busy = Running::spawn(std::process::Command::new("/bin/sh").args(["-c", "while :; do :; done"]));
        let pane = Pane { id: 1, shell: busy.pid(), foreground: None, agent: None };
        let places = Places { panes: vec![pane], server: me(), own: false };
        let sample = sample(&places, None, Duration::from_millis(300));
        let [tree] = &sample.panes[..] else { panic!("one pane: {:?}", sample.panes) };
        let shell = &tree.procs[0];
        assert_eq!((shell.pid, shell.parent, shell.depth), (busy.pid(), None, 0));
        assert!(shell.cpu.is_some_and(|cpu| cpu > 10.0), "{:?}", shell.cpu);
        assert!(shell.memory.is_some());
        assert_eq!(sample.cornercase, Vec::<Proc>::new());
    }

    #[test]
    fn follows_the_children_with_their_parents_and_depths() {
        let family = Family::new();
        let (child, grandchild) = (family.child().expect("child"), family.grandchild().expect("grandchild"));
        let pane = Pane { id: 1, shell: family.pid(), foreground: None, agent: None };
        let places = Places { panes: vec![pane], server: me(), own: false };
        let sample = sample(&places, None, Duration::from_millis(50));
        let shape: Vec<_> = sample.panes[0].procs.iter().map(|p| (p.pid, p.parent, p.depth)).collect();
        assert_eq!(shape, [(family.pid(), None, 0), (child, Some(family.pid()), 1), (grandchild, Some(child), 2)]);
    }

    #[test]
    fn puts_the_servers_own_children_that_are_not_shells_in_the_cornercase_group() {
        let pane_shell = Running::spawn(std::process::Command::new("/bin/sleep").arg("30"));
        let own = Running::spawn(std::process::Command::new("/bin/sleep").arg("30"));
        let pane = Pane { id: 1, shell: pane_shell.pid(), foreground: None, agent: None };
        let places = Places { panes: vec![pane], server: me(), own: true };
        let sample = sample(&places, None, Duration::from_millis(50));
        let pids: Vec<i32> = sample.cornercase.iter().map(|proc| proc.pid).collect();
        assert_eq!(pids.first(), Some(&me()));
        assert!(pids.contains(&own.pid()), "{pids:?}");
        assert!(!pids.contains(&pane_shell.pid()), "{pids:?}");
    }

    #[test]
    fn skips_zombies() {
        let mut child = std::process::Command::new("/bin/sh").args(["-c", "exit 0"]).spawn().expect("spawn");
        let pid = i32::try_from(child.id()).expect("pid");
        wait_until("the child is a zombie", || process::alive(pid) && !process::running(pid, None));
        let pane = Pane { id: 1, shell: pid, foreground: None, agent: None };
        let sample = sample(&Places { panes: vec![pane], server: me(), own: false }, None, Duration::from_millis(10));
        assert_eq!(sample.panes[0].procs, Vec::<Proc>::new());
        child.wait().expect("reap");
    }
}
