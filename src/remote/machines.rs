const LONGEST: usize = 255;
pub(crate) const NOT_A_DESTINATION: &str = "one word that does not start with -, such as devbox or me@devbox";

pub(crate) fn valid(destination: &str) -> bool {
    !destination.is_empty()
        && destination.len() <= LONGEST
        && !destination.starts_with('-')
        && !destination.chars().any(|c| c.is_whitespace() || c.is_control())
}

#[cfg(test)]
mod tests;
