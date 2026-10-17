use serde::{Deserialize, Serialize};

use super::Fields;
use crate::host::theme::HostTheme;

const ANSI_COLORS: usize = 16;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Theme {
    pub(crate) theme: Option<Box<HostTheme>>,
    pub(crate) clear: bool,
}

#[derive(Debug)]
pub(crate) enum Asked<'a> {
    Read,
    Set(&'a HostTheme),
    Clear,
}

impl Theme {
    pub(crate) fn asked(&self) -> Result<Asked<'_>, String> {
        match (self.theme.as_deref(), self.clear) {
            (Some(_), true) => Err("`theme` takes either colours or `clear`, not both".to_string()),
            (Some(theme), false) => check(theme).map(|()| Asked::Set(theme)),
            (None, true) => Ok(Asked::Clear),
            (None, false) => Ok(Asked::Read),
        }
    }

    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.add("colours", self.theme.is_some()).add("clear", self.clear);
    }
}

pub(crate) fn check(theme: &HostTheme) -> Result<(), String> {
    if theme.foreground.is_none() {
        return Err("a theme needs its foreground, as [r, g, b]".to_string());
    }
    if theme.background.is_none() {
        return Err("a theme needs its background, as [r, g, b]".to_string());
    }
    match theme.palette[..ANSI_COLORS].iter().position(Option::is_none) {
        Some(index) => Err(format!("a theme needs the 16 ANSI colours: palette entry {index} is missing")),
        None => Ok(()),
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ThemeFrom {
    #[default]
    Default,
    Window,
    Client,
}

impl ThemeFrom {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Window => "window",
            Self::Client => "client",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Scheme {
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ThemeInfo {
    pub(crate) from: ThemeFrom,
    pub(crate) scheme: Option<Scheme>,
}

impl ThemeInfo {
    pub(crate) fn of(from: ThemeFrom, theme: &HostTheme) -> Self {
        let scheme = theme.is_light().map(|light| if light { Scheme::Light } else { Scheme::Dark });
        Self { from, scheme }
    }
}

#[cfg(test)]
mod tests;
