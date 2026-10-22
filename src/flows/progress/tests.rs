use std::time::{Duration, Instant, SystemTime};

use super::*;
use crate::agents::Known;
use crate::flows::{ORCHESTRATOR, builtin};

fn full_issue() -> Flow {
    builtin::flows(&Known::ALL, Some(Known::Claude)).remove(0)
}

struct Script {
    flow: Flow,
    start: Instant,
    messages: Vec<Message>,
}

impl Script {
    fn new() -> Self {
        Self { flow: full_issue(), start: Instant::now(), messages: Vec::new() }
    }

    fn with(mut self, kind: Kind, from: &str, to: &str) -> Self {
        let at = self.start + Duration::from_secs(u64::try_from(self.messages.len()).expect("few messages"));
        let text = Some(format!("{from} to {to}"));
        let time = SystemTime::UNIX_EPOCH + Duration::from_secs(self.messages.len() as u64);
        self.messages.push(Message {
            from: from.into(),
            to: to.into(),
            kind,
            text,
            bytes: 0,
            at,
            time,
            seq: 0,
            edge: None,
        });
        self
    }

    fn compact(mut self, to: &str) -> Self {
        self = self.send(to);
        if let Some(last) = self.messages.last_mut() {
            last.text = Some("/compact keep: the branches".into());
        }
        self
    }

    fn send(self, to: &str) -> Self {
        self.with(Kind::Send, ORCHESTRATOR, to)
    }

    fn progress(&self) -> Progress {
        Progress::of(&self.flow, &self.messages)
    }

    fn edges(&self) -> Vec<Option<Edge>> {
        let mut progress = Progress::default();
        self.messages.iter().map(|message| progress.step(&self.flow, message)).collect()
    }
}

mod the_active_stage {
    use super::*;

    #[test]
    fn nothing_is_active_before_the_first_send() {
        assert_eq!(Script::new().progress(), Progress::default());
    }

    #[test]
    fn the_stage_of_the_role_that_last_got_a_send() {
        let p = Script::new().send("coder").send("reviewer").progress();
        assert_eq!((p.active, p.holder.as_deref()), (Some(1), Some("reviewer")));
    }

    #[test]
    fn a_member_writing_to_another_member_does_not_move_the_work() {
        let p = Script::new().send("coder").with(Kind::Send, "coder", "tester").progress();
        assert_eq!((p.active, p.holder.as_deref(), p.outside), (Some(0), Some("coder"), None));
    }

    #[test]
    fn reads_keys_and_waits_do_not_move_the_work() {
        let p = Script::new()
            .send("coder")
            .with(Kind::Read, ORCHESTRATOR, "reviewer")
            .with(Kind::Wait, ORCHESTRATOR, "tester")
            .with(Kind::Keys, ORCHESTRATOR, "security")
            .progress();
        assert_eq!((p.active, p.last), (Some(0), None));
    }

    #[test]
    fn a_send_to_the_orchestrator_does_not_move_the_work() {
        let p = Script::new().send("coder").with(Kind::Send, "coder", ORCHESTRATOR).progress();
        assert_eq!((p.active, p.holder.as_deref()), (Some(0), Some("coder")));
    }
}

mod arrows {
    use super::*;

    #[test]
    fn the_first_send_into_the_first_stage_moves_along_no_arrow() {
        assert_eq!(Script::new().send("coder").progress().last, None);
    }

    #[test]
    fn a_send_into_the_next_stage_moves_along_its_arrow() {
        let s = Script::new().send("coder").send("reviewer");
        let last = s.progress().last.expect("a step");
        assert_eq!((last.edge, last.from.as_deref(), last.to.as_str()), (Edge::Forward(0), Some("coder"), "reviewer"));
        assert_eq!(last.at, s.messages[1].at);
    }

    #[test]
    fn the_second_role_of_a_parallel_stage_takes_the_same_arrow() {
        let last = Script::new().send("coder").send("reviewer").send("security").progress().last.expect("a step");
        assert_eq!((last.edge, last.from.as_deref(), last.to.as_str()), (Edge::Forward(0), Some("coder"), "security"));
    }

    #[test]
    fn a_follow_up_to_the_same_role_moves_nothing() {
        let s = Script::new().send("coder").send("reviewer").send("reviewer");
        assert_eq!(s.progress().last.expect("a step").at, s.messages[1].at);
    }

    #[test]
    fn the_last_message_along_each_arrow_is_kept() {
        let s = Script::new().send("coder").send("reviewer").send("security").send("tester");
        let p = s.progress();
        let text = |edge, to| p.along(edge, to).and_then(|m| m.text.as_deref());
        assert_eq!(text(Edge::Forward(0), None), Some("orchestrator to security"));
        assert_eq!(text(Edge::Forward(1), None), Some("orchestrator to tester"));
        assert_eq!(p.along(Edge::Loop(1), None), None);
    }

    #[test]
    fn each_role_keeps_its_own_message_along_an_arrow() {
        let s = Script::new().send("coder").send("reviewer").send("security").send("reviewer");
        let p = s.progress();
        let text = |to| p.along(Edge::Forward(0), to).and_then(|m| m.text.as_deref());
        assert_eq!(text(Some("security")), Some("orchestrator to security"));
        assert_eq!(text(Some("reviewer")), Some("orchestrator to reviewer"));
        assert_eq!(p.along(Edge::Forward(0), Some("reviewer")).map(|m| m.at), Some(s.messages[3].at));
        assert_eq!(text(None), Some("orchestrator to reviewer"));
        assert_eq!(p.sent().count(), 2);
    }

    #[test]
    fn a_step_travels_for_a_second() {
        let s = Script::new().send("coder").send("reviewer");
        let p = s.progress();
        let at = s.messages[1].at;
        assert_eq!(p.travelling(at).map(|(_, done)| done), Some(0.0));
        assert_eq!(p.travelling(at + TRAVEL / 2).map(|(_, done)| done), Some(0.5));
        assert_eq!(p.travelling(at + TRAVEL), None);
        assert_eq!(p.travelling(s.start), None);
    }
}

mod loops {
    use super::*;

    #[test]
    fn a_send_back_to_the_loop_target_is_a_round() {
        let p = Script::new().send("coder").send("reviewer").send("coder").progress();
        assert_eq!((p.rounds(1), p.last.map(|l| l.edge)), (1, Some(Edge::Loop(1))));
    }

    #[test]
    fn rounds_add_up_for_each_loop() {
        let p = Script::new()
            .send("coder")
            .send("reviewer")
            .send("coder")
            .send("security")
            .send("coder")
            .send("reviewer")
            .send("tester")
            .send("coder")
            .progress();
        assert_eq!((p.rounds(1), p.rounds(2), p.outside), (2, 1, None));
    }

    #[test]
    fn rounds_past_the_limit_still_count() {
        let mut s = Script::new().send("coder");
        for _ in 0..4 {
            s = s.send("reviewer").send("coder");
        }
        assert_eq!(s.progress().rounds(1), 4);
    }
}

mod compacting {
    use super::*;

    #[test]
    fn a_compact_command_to_another_stage_moves_nothing_and_counts_no_round() {
        let script = Script::new().send("coder").send("reviewer").compact("coder");

        let p = script.progress();

        assert_eq!((p.active, p.rounds(1), p.outside), (Some(1), 0, None));
        assert_eq!(script.edges().last(), Some(&None));
    }

    #[test]
    fn a_real_send_after_it_still_loops() {
        let p = Script::new().send("coder").send("reviewer").compact("coder").send("coder").progress();

        assert_eq!((p.active, p.rounds(1), p.last.map(|l| l.edge)), (Some(0), 1, Some(Edge::Loop(1))));
    }

    #[test]
    fn a_compact_command_is_the_agents_own() {
        let mut script = Script::new().send("coder").send("reviewer");
        script.flow.roles[0].agent = "gemini".into();

        let p = script.compact("coder").progress();

        assert_eq!((p.active, p.rounds(1)), (Some(0), 1));
    }
}

mod outside_the_plan {
    use super::*;

    #[test]
    fn skipping_a_stage_is_outside_the_plan() {
        let p = Script::new().send("coder").send("tester").progress();
        let outside = Edge::Outside { from: Some(0), to: 2 };
        assert_eq!((p.outside, p.last.map(|l| l.edge), p.active), (Some(outside), Some(outside), Some(2)));
    }

    #[test]
    fn going_back_where_no_loop_goes_is_outside_the_plan() {
        let mut s = Script::new().send("reproducer");
        s.flow = builtin::flows(&Known::ALL, Some(Known::Claude)).remove(3);
        let p = s.send("coder").send("reproducer").progress();
        assert_eq!(p.outside, Some(Edge::Outside { from: Some(1), to: 0 }));
    }

    #[test]
    fn starting_anywhere_but_the_first_stage_is_outside_the_plan() {
        let p = Script::new().send("reviewer").progress();
        assert_eq!(p.outside, Some(Edge::Outside { from: None, to: 1 }));
    }

    #[test]
    fn the_last_step_outside_the_plan_stays_after_the_work_goes_on() {
        let p = Script::new().send("coder").send("tester").send("coder").progress();
        assert_eq!((p.outside, p.rounds(2)), (Some(Edge::Outside { from: Some(0), to: 2 }), 1));
    }
}

mod the_edge_a_step_returns {
    use super::*;

    #[test]
    fn a_step_returns_the_edge_it_moved_along() {
        let script = Script::new()
            .send("coder")
            .send("reviewer")
            .send("security")
            .send("security")
            .send("coder")
            .send("reviewer")
            .send("tester")
            .with(Kind::Send, "coder", "tester")
            .with(Kind::Read, ORCHESTRATOR, "coder");

        assert_eq!(
            script.edges(),
            [
                None,
                Some(Edge::Forward(0)),
                Some(Edge::Forward(0)),
                None,
                Some(Edge::Loop(1)),
                Some(Edge::Forward(0)),
                Some(Edge::Forward(1)),
                None,
                None,
            ]
        );
    }

    #[test]
    fn a_jump_outside_the_plan_returns_its_stages() {
        let script = Script::new().send("coder").send("tester");

        assert_eq!(script.edges(), [None, Some(Edge::Outside { from: Some(0), to: 2 })]);
    }

    #[test]
    fn a_step_keeps_the_time_of_its_message() {
        let script = Script::new().send("coder").send("reviewer");

        let last = script.progress().last.expect("a step");

        assert_eq!(last.time, SystemTime::UNIX_EPOCH + Duration::from_secs(1));
    }
}
