use std::time::{SystemTime, UNIX_EPOCH};

use super::*;
use crate::agents::grok::fake::FakeGrok;

#[test]
fn a_later_submission_confirms_a_pending_send() {
    let (mut app, rx, _dirs) = app_with(1);
    let grok = FakeGrok::new(&app.projects[0].path);
    type_line(&mut app, &grok.command_line());
    refreshing(&mut app, &rx, "grok is running", |a| {
        a.term().and_then(Term::foreground_pid).is_some_and(|pid| {
            crate::agents::detect(&a.config, &process::args(pid)).as_deref() == Some(crate::agents::Known::Grok.name())
        })
    });
    let pid = app.term().and_then(Term::foreground_pid).expect("the pid");
    grok.bind(pid);
    let now = u64::try_from(SystemTime::now().duration_since(UNIX_EPOCH).expect("clock").as_millis()).expect("millis");
    grok.add_prompt(now.saturating_sub(60_000));
    refreshing(&mut app, &rx, "the first prompt is recorded", |a| {
        a.term().is_some_and(|term| term.context.prompted().is_some())
    });
    let pane = first(&app);
    ask(&mut app, None, send_text(pane, "next", true, false));
    grok.add_prompt(now.saturating_add(60_000));
    done(answered(&mut app, &rx, "the later prompt confirms the send"));
}
