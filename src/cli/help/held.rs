pub(in crate::cli) const HELD_HELP: &str =
    "`cornercase send` holds a message while a person is typing in the pane, and the window's bar on that
pane offers two buttons. This is what they do, for a program that draws that bar itself: --send-now
is its send now, --wait-turn its wait turn. Give exactly one.

--send-now sends the first held message at once, as `send --force` does: after what the person has
typed, whatever the pane shows. Its sender gets the usual answer, or the refusal when the pane cannot
take it (an agent asking a question). If an earlier message is between its paste and its Enter, it goes
right after that Enter, so held_sends may only drop a moment later. The other held messages keep waiting.

--wait-turn only says the person saw it: nothing goes earlier or later. It covers the messages held so
far, so one held later turns held_queued off again.

Both exit with status 1 and say why when nothing is held in the pane: the message may just have gone
out. --send-now also refuses a pane that is restarting or resuming its agent, where the window has no bar.

It prints what is held now. With --json it prints {\"pane\", \"held_sends\", \"held_from\", \"held_queued\"},
what `status --json`, `status --follow` and `view` have on each pane: held_from is the pane that sent the
first held message (null for a sender outside a pane; a pane since closed keeps its id, and ids last one
run of the server), and held_queued is true once --wait-turn covers the newest held message.

Examples:
  cornercase held --pane 9 --send-now
  cornercase held --pane 9 --wait-turn --json";
