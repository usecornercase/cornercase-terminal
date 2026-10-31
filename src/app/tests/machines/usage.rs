use std::sync::mpsc::Sender;

use rstest::rstest;

use super::*;
use crate::agents::stats::View;
use crate::app::machine_usage::{Cache, GONE, UNREADABLE};
use crate::control::Response;
use crate::control::usage::{PlanInfo, Plans};
use crate::machines::link::Down;
use crate::machines::link::fake::{request, respond};

#[derive(Clone)]
enum Reply {
    Answer(Response),
    Refuse,
}

struct Asked {
    reply: Reply,
    gate: Arc<Mutex<Receiver<()>>>,
    seen: Seen,
}

impl Connect for Asked {
    fn connect(&self, _: &Killer) -> Result<Line> {
        self.seen.note("ask".into());
        let Reply::Answer(response) = self.reply.clone() else {
            let reason = "ssh: connect to host vps.lan port 22: Connection refused".into();
            return Err(Error::Ssh { host: "vps.lan".into(), reason });
        };
        let (line, mut theirs) = line(version::CURRENT, None);
        let (gate, seen) = (Arc::clone(&self.gate), self.seen.clone());
        std::thread::spawn(move || {
            let asked = request(&mut theirs);
            seen.note(format!("asked {}", asked["command"].as_str().unwrap_or_default()));
            let _ = gate.lock().recv();
            respond(&mut theirs, &response);
        });
        Ok(line)
    }
}

fn gated(reply: Reply) -> (Rig, Sender<()>) {
    let mut rig = Rig::new(Vec::new(), There::Missing);
    let (open, gate) = mpsc::channel();
    let gate = Arc::new(Mutex::new(gate));
    let seen = rig.seen.clone();
    rig.app.machines.set_connector(Box::new(move |destination| {
        if seen.count("link") > 0 {
            return Box::new(Asked { reply: reply.clone(), gate: Arc::clone(&gate), seen: seen.clone() });
        }
        seen.note("link".into());
        Box::new(Fake { destination: destination.into(), there: There::Server("vps-1".into()), seen: seen.clone() })
    }));
    rig.edit(vec![machine("vps", "vps.lan")], Duration::from_secs(2));
    rig.until("the machine links", |app| app.machines.held()[0].status.state == State::Linked);
    (rig, open)
}

fn linked(reply: Reply) -> Rig {
    gated(reply).0
}

fn answer(value: &impl serde::Serialize) -> Reply {
    Reply::Answer(Response::Ok(serde_json::to_value(value).expect("a JSON answer")))
}

fn counted() -> View {
    let mut view = View::default();
    let total = view.windows.get_mut("total").expect("the total window");
    (total.counts.agents, total.counts.prompts) = (2, 5);
    view
}

impl Rig {
    fn cache(&self) -> &Cache {
        &self.app.machines.held()[0].cache
    }

    fn heard_back(&mut self) {
        self.until("the machine answers", |app| {
            let cache = &app.machines.held()[0].cache;
            !cache.stats.asking && !cache.usage.asking
        });
    }

    fn stats_read(&mut self) -> Option<u64> {
        self.app.ask_machine::<View>("vps");
        self.heard_back();
        self.cache().stats.read.as_ref().map(|(view, _)| view.windows["total"].counts.prompts)
    }
}

mod a_question {
    use super::*;

    #[test]
    fn fills_the_cache_with_the_machine_s_stats() {
        let mut rig = linked(answer(&counted()));
        let link = rig.links()[0];

        assert_eq!(rig.app.ask_machine::<View>("VPS"), Some(link));
        rig.heard_back();

        let (view, _) = rig.cache().stats.read.as_ref().expect("the stats are cached");
        assert_eq!((view.windows["total"].counts.prompts, view.empty), (5, false));
        assert_eq!(rig.cache().stats.error, None);
        assert_eq!((rig.seen("ask"), rig.seen("asked stats")), (1, 1));
    }

    #[test]
    fn fills_the_cache_with_the_machine_s_plans() {
        let plans =
            Plans { agents: vec![PlanInfo { agent: "claude".into(), ..PlanInfo::default() }], ..Plans::default() };
        let mut rig = linked(answer(&plans));

        rig.app.ask_machine::<Plans>("vps");
        rig.heard_back();

        assert_eq!(rig.cache().usage.read.as_ref().map(|(plans, _)| plans.clone()), Some(plans));
        assert!(rig.cache().stats.read.is_none());
        assert_eq!(rig.seen("asked usage"), 1);
    }

    #[test]
    fn runs_once_at_a_time() {
        let (mut rig, open) = gated(answer(&counted()));
        let link = rig.links()[0];

        rig.app.ask_machine::<View>("vps");
        assert_eq!(rig.app.ask_machine::<View>("vps"), Some(link));
        open.send(()).expect("let the machine answer");
        rig.heard_back();

        assert_eq!(rig.seen("ask"), 1);
        assert!(rig.cache().stats.read.is_some());
    }

    #[test]
    fn is_not_asked_of_a_machine_that_is_not_linked() {
        let mut rig = Rig::new(vec![machine("vps", "vps.lan")], There::Missing);
        rig.until("the link reports", |app| app.machines.held()[0].status.state == State::Unreachable);

        assert_eq!(rig.app.ask_machine::<View>("vps"), None);
        assert_eq!(rig.app.ask_machine::<Plans>("pi"), None);

        assert_eq!(rig.seen("start vps.lan"), 1);
        assert!(!rig.cache().stats.asking);
    }

    #[test]
    fn keeps_its_error_without_the_destination() {
        let mut rig = linked(Reply::Refuse);

        rig.app.ask_machine::<View>("vps");
        rig.heard_back();

        let error = rig.cache().stats.error.clone().expect("the error is kept");
        assert_eq!(error, "ssh: connect to host vps port 22: Connection refused");
        assert!(rig.cache().stats.read.is_none());
    }

    #[test]
    fn says_why_an_answer_cannot_be_read() {
        let mut rig = linked(answer(&"not a view"));

        assert_eq!(rig.stats_read(), None);

        assert_eq!(rig.cache().stats.error.as_deref(), Some(UNREADABLE));
    }
}

mod the_cache {
    use super::*;

    #[rstest]
    #[case::this_computer(Down::ThisComputer, None)]
    #[case::the_same_server(Down::SameServer(99), None)]
    #[case::a_machine_that_is_down(Down::Unreachable("gone".into()), Some(5))]
    fn is_forgotten_when_the_machine_turns_out_not_to_be_another(#[case] down: Down, #[case] kept: Option<u64>) {
        let mut rig = linked(answer(&counted()));
        assert_eq!(rig.stats_read(), Some(5));

        let link = rig.links()[0];
        rig.app.machine_event(MachineEvent::Link(Report { link, event: Event::Down { down, failed: 1 } }));

        assert_eq!(rig.cache().stats.read.as_ref().map(|(view, _)| view.windows["total"].counts.prompts), kept);
    }

    #[test]
    fn goes_with_a_machine_removed_from_the_list() {
        let mut rig = linked(answer(&counted()));
        assert_eq!(rig.stats_read(), Some(5));

        rig.edit(Vec::new(), Duration::from_secs(3));
        rig.edit(vec![machine("vps", "vps.lan")], Duration::from_secs(4));

        assert!(rig.cache().stats.read.is_none());
    }
}

mod requests;
