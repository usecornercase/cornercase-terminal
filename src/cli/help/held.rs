pub(in crate::cli) const HELD_HELP: &str =
    "`cornercase send` holds a message while a person is typing in the pane, and the window's bar on that
pane offers two buttons. This is what they do, for a program that draws that bar itself: --send-now
is its send now, --wait-turn its wait turn. Give exactly one.

--send-now puts the first held message through at once, as `send --force` does, but never sends the
person's half-written line with it: if they have a draft in the pane then, the message is pasted after
it and its Enter is left to them, to send both with Enter or edit them first, and its sender gets an
error saying the message is in their input. Without a draft it is pasted and entered, and its sender
gets the usual answer, or the refusal when the pane cannot take it (an agent asking a question). If an
earlier message is between its paste and its Enter, it goes right after that Enter, so held_sends may
only drop a moment later. The other held messages keep waiting.

--wait-turn only says the person saw it: nothing goes earlier or later. It covers the messages held so
far, so one held later turns held_queued off again.

Both exit with status 1 and say why when nothing is held in the pane: the message may just have gone
out. --send-now also refuses a pane that is restarting or resuming its agent, where the window has no bar.

It prints what is held now, after a line saying so when --send-now left the Enter to the person. With
--json it prints {\"pane\", \"held_sends\", \"held_from\", \"held_queued\", \"after_draft\"}. The first four
are what `status --json`, `status --follow` and `view` have on each pane: held_from is the pane that sent
the first held message (null for a sender outside a pane; a pane since closed keeps its id, and ids
last one run of the server), and held_queued is true once --wait-turn covers the newest held message.
after_draft is true when --send-now pasted the message after the person's draft and left its Enter to
them. A server without the feature held-after-draft (`cornercase contract`) sends both as one line.

Examples:
  cornercase held --pane 9 --send-now
  cornercase held --pane 9 --wait-turn --json
  cornercase --remote vps held --pane 9 --send-now --boot 4242-1700000000";
