use crate::agents::activity::Activity;

pub(crate) fn opencode(turn: bool) -> Activity {
    if turn { Activity::Working } else { Activity::Idle }
}

pub(in crate::agents) mod database;

#[cfg(test)]
pub(crate) mod fake;
