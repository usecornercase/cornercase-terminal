use std::time::SystemTime;

use super::*;
use crate::agents::Known;
use crate::agents::stats::{Counted, PEAK};
use crate::agents::usage::Page;

const PICKS: [&str; 3] = ["this computer", "vps", "all"];
const REFUSED: &str = "vps: ssh: connect to host vps port 22: Connection refused · numbers read just now";

impl Rig {
    fn opened(&mut self) -> ui::Usage {
        self.app.open_usage();
        self.heard_back();
        self.app.usage_view()
    }

    fn pick(&mut self, i: usize) -> ui::Usage {
        let picks = ui::usage_picks(AREA, &self.app.usage_view());
        click(&mut self.app, picks[i].as_position());
        self.heard_back();
        self.app.usage_view()
    }

    fn counted_here(&mut self, prompts: usize) {
        let now = SystemTime::now();
        self.app.agents.stats.counted(Known::Claude, Counted { prompts: vec![now; prompts], tokens: Vec::new() });
    }
}

fn prompts(usage: &ui::Usage) -> u64 {
    usage.stats.windows["total"].counts.prompts
}

mod the_machine_picker {
    use super::*;

    #[test]
    fn starts_on_every_machine_and_sums_this_computer_and_each_one() {
        let mut rig = linked(answer(&counted()));
        rig.counted_here(2);

        let usage = rig.opened();

        assert_eq!((prompts(&usage), usage.notes), (7, vec![PEAK.to_string()]));
        assert_eq!((usage.machines, usage.picked), (PICKS.map(String::from).to_vec(), 2));
        assert_eq!(rig.seen("asked stats"), 1);
    }

    #[test]
    fn shows_only_the_numbers_of_the_machine_picked_with_a_click() {
        let mut rig = linked(answer(&counted()));
        rig.counted_here(2);
        rig.opened();

        let there = rig.pick(1);
        let here = rig.pick(0);

        assert_eq!((there.picked, prompts(&there), there.notes), (1, 5, Vec::new()));
        assert_eq!((here.picked, prompts(&here), here.notes), (0, 2, Vec::new()));
        assert_eq!(rig.seen("asked stats"), 2);
    }

    #[test]
    fn asks_for_the_limits_on_the_limits_tab() {
        let mut rig = linked(answer(&counted()));
        rig.opened();

        rig.app.switch_usage(Page::Limits);
        rig.heard_back();

        assert_eq!((rig.seen("asked stats"), rig.seen("asked usage")), (1, 1));
    }

    #[test]
    fn goes_back_to_every_machine_when_the_one_picked_leaves_the_list() {
        let mut rig = linked(answer(&counted()));
        rig.opened();
        rig.pick(1);

        rig.edit(vec![machine("pi", "pi.lan")], Duration::from_secs(3));

        let usage = rig.app.usage_view();
        assert_eq!((usage.machines[1].as_str(), usage.picked), ("pi", 2));
    }
}

mod a_machine_that_is_down {
    use super::*;

    #[test]
    fn is_summed_from_its_last_numbers_and_says_when_it_was_seen() {
        let mut rig = linked(answer(&counted()));
        assert_eq!(rig.stats_read(), Some(5));
        rig.down(Down::Unreachable("gone".into()));

        let usage = rig.opened();

        assert_eq!((prompts(&usage), usage.notes), (5, vec![PEAK.to_string(), "vps: seen just now".to_string()]));
        assert_eq!(rig.seen("ask"), 1);
    }

    #[test]
    fn without_numbers_is_left_out_and_says_why() {
        let mut rig = Rig::new(vec![machine("vps", "vps.lan")], There::Missing);
        rig.until("the link reports", |app| app.machines.held()[0].status.state == State::Unreachable);
        rig.counted_here(2);

        let all = rig.opened();
        let there = rig.pick(1);

        let why = "cornercase was not found on vps: install it there";
        assert_eq!((prompts(&all), all.notes), (2, vec![PEAK.to_string(), format!("vps: not summed · {why}")]));
        assert_eq!((there.stats.empty, there.notes), (true, vec![format!("vps: {why}")]));
    }
}

mod a_machine_whose_last_answer_failed {
    use super::*;

    #[test]
    fn is_summed_from_its_last_numbers_and_says_why_they_are_old() {
        let mut rig = linked(once(&counted()));
        rig.counted_here(2);
        rig.opened();
        send_key(&mut rig.app, KeyCode::Esc, KeyModifiers::NONE);

        let usage = rig.opened();

        assert_eq!((prompts(&usage), usage.notes), (7, vec![PEAK.to_string(), REFUSED.to_string()]));
        assert_eq!(rig.seen("ask"), 2);
    }

    #[test]
    fn picked_shows_its_last_numbers_and_says_why_they_are_old() {
        let mut rig = linked(once(&counted()));
        rig.opened();

        let there = rig.pick(1);

        assert_eq!((prompts(&there), there.notes), (5, vec![REFUSED.to_string()]));
    }
}

mod sharing {
    use super::*;
    use crate::host::clipboard;

    impl Rig {
        fn share(&mut self) -> Vec<Vec<u8>> {
            let usage = self.app.usage_view();
            click(&mut self.app, ui::usage_share(AREA, &usage).as_position());
            self.app.take_host_writes()
        }
    }

    #[test]
    fn copies_the_link_of_the_sum_on_every_machine_and_of_the_machine_picked() {
        let mut rig = linked(answer(&counted()));
        rig.counted_here(2);
        let here = rig.app.stats_view(SystemTime::now()).share_link();

        let all = rig.opened();
        let summed = rig.share();
        let there = rig.pick(1);
        let picked = rig.share();

        assert_eq!((prompts(&all), prompts(&there)), (7, 5));
        assert_eq!(
            (summed, picked),
            (vec![clipboard::osc52(&all.stats.share_link())], vec![clipboard::osc52(&there.stats.share_link())])
        );
        assert_ne!(all.stats.share_link(), here);
        assert_ne!(all.stats.share_link(), there.stats.share_link());
    }
}

mod the_limits {
    use super::*;

    fn plans(identity: Option<&str>) -> Plans {
        let info = PlanInfo {
            agent: "claude".into(),
            title: "Claude Code".into(),
            age: Some(0),
            identity: identity.map(str::to_string),
            ..PlanInfo::default()
        };
        Plans { agents: vec![info], ..Plans::default() }
    }

    fn machines(usage: &ui::Usage) -> Vec<(String, Vec<String>)> {
        let titles =
            |machine: &ui::UsageMachine| machine.sections.iter().map(|section| section.title.clone()).collect();
        usage.by_machine.iter().map(|machine| (machine.name.clone(), titles(machine))).collect()
    }

    impl Rig {
        fn limits(&mut self) -> ui::Usage {
            self.opened();
            self.app.switch_usage(Page::Limits);
            self.heard_back();
            self.app.usage_view()
        }
    }

    #[test]
    fn show_a_section_for_this_computer_then_one_per_machine() {
        let mut rig = linked(answer(&plans(Some("a"))));

        let usage = rig.limits();

        let here = usage.by_machine[0].sections.iter().map(|section| section.title.clone()).collect::<Vec<_>>();
        assert_eq!(usage.sections.len(), 0);
        assert_eq!(machines(&usage)[1], ("vps".to_string(), vec!["Claude Code".to_string()]));
        assert_eq!(
            (usage.by_machine[0].name.as_str(), here),
            ("this computer", vec!["Claude Code".to_string(), "Codex".to_string()])
        );
    }

    #[test]
    fn show_only_the_machine_picked() {
        let mut rig = linked(answer(&plans(Some("a"))));
        rig.limits();

        let there = rig.pick(1);
        let here = rig.pick(0);

        assert_eq!(machines(&there), [("vps".to_string(), vec!["Claude Code".to_string()])]);
        assert_eq!((here.by_machine.len(), here.sections.len()), (0, 2));
    }

    #[test]
    fn of_a_machine_that_is_down_say_when_it_was_seen() {
        let mut rig = linked(answer(&plans(None)));
        rig.app.ask_machine::<Plans>("vps");
        rig.heard_back();
        rig.down(Down::Unreachable("gone".into()));

        let usage = rig.limits();

        let vps = &usage.by_machine[1];
        assert_eq!((vps.status.as_str(), vps.error.as_deref(), vps.sections.len()), ("seen just now", None, 1));
    }

    #[test]
    fn of_a_machine_whose_last_answer_failed_say_why_beside_its_last_rows() {
        let mut rig = linked(once(&plans(None)));
        rig.app.ask_machine::<Plans>("vps");
        rig.heard_back();

        let usage = rig.limits();

        let vps = &usage.by_machine[1];
        let why = REFUSED.trim_start_matches("vps: ");
        assert_eq!((vps.status.as_str(), vps.error.as_deref(), vps.sections.len()), ("", Some(why), 1));
    }

    #[test]
    fn of_a_machine_never_read_say_why() {
        let mut rig = Rig::new(vec![machine("vps", "vps.lan")], There::Missing);
        rig.until("the link reports", |app| app.machines.held()[0].status.state == State::Unreachable);

        let usage = rig.limits();

        let why = "cornercase was not found on vps: install it there";
        assert_eq!(machines(&usage)[1], ("vps".to_string(), Vec::new()));
        assert_eq!(usage.by_machine[1].error.as_deref(), Some(why));
    }
}
