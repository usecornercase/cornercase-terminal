const INPUT_MARK: char = '❯';
const RULE: char = '─';
const RULE_MIN: usize = 8;
const SURVEY_OPTIONS: [&str; 2] =
    ["1: Bad    2: Fine   3: Good   0: Dismiss", "1: Bad    2: Fine   3: Good   4: Unsure 0: Dismiss"];
const SURVEY_BULLET: &str = "● ";
const SURVEY_INDENT: &str = "  ";
const SURVEY_GAP: usize = 3;
const SURVEY_QUESTION_LINES: usize = 4;

#[expect(clippy::unnecessary_wraps, reason = "Known dispatch shares an optional input-box check across agents")]
pub(in crate::agents) fn input_box(screen: &str) -> Option<bool> {
    Some(box_rule(&screen.lines().collect::<Vec<_>>()).is_some())
}

pub(in crate::agents) fn draft(lines: &[(String, String)]) -> Option<bool> {
    let texts: Vec<&str> = lines.iter().map(|(text, _)| text.as_str()).collect();
    let top = box_rule(&texts)? + 1;
    let inside = lines[top..].iter().take_while(|(text, _)| !rule(text));
    let typed = inside.enumerate().any(|(i, (_, lit))| {
        let lit = if i == 0 { lit.strip_prefix(INPUT_MARK).unwrap_or(lit) } else { lit };
        lit.chars().any(|c| !c.is_whitespace())
    });
    Some(typed)
}

pub(in crate::agents) fn bypass_consent(screen: &str) -> bool {
    let compact: String = screen.chars().filter(|c| !c.is_whitespace()).collect();
    compact.contains("BypassPermissionsmode") && compact.contains("Yes,Iaccept")
}

pub(in crate::agents) fn survey(screen: &str) -> bool {
    let lines: Vec<&str> = screen.lines().collect();
    let Some(rule) = box_rule(&lines) else { return false };
    let above = &lines[..rule];
    let Some(gap) = above.iter().rev().take(SURVEY_GAP).position(|line| survey_options(line)) else { return false };
    let question = &above[..above.len() - 1 - gap];
    question
        .iter()
        .rev()
        .take(SURVEY_QUESTION_LINES)
        .find(|line| !survey_indented(line))
        .is_some_and(|line| line.starts_with(SURVEY_BULLET))
}

fn rule(line: &str) -> bool {
    let line = line.trim();
    line.chars().count() >= RULE_MIN && line.chars().all(|c| c == RULE)
}

fn box_rule(lines: &[&str]) -> Option<usize> {
    lines.windows(2).rposition(|pair| rule(pair[0]) && pair[1].starts_with(INPUT_MARK))
}

fn survey_options(line: &str) -> bool {
    line.strip_prefix(SURVEY_INDENT).is_some_and(|rest| SURVEY_OPTIONS.contains(&rest.trim_end()))
}

fn survey_indented(line: &str) -> bool {
    line.strip_prefix(SURVEY_INDENT).is_some_and(|rest| rest.starts_with(|c: char| !c.is_whitespace()))
}

#[cfg(test)]
mod tests;
