use std::ops::RangeInclusive;

use serde::{Deserialize, Serialize};

pub(crate) const DEFAULT_COLS: u16 = 160;
pub(crate) const DEFAULT_ROWS: u16 = 48;
pub(crate) const COLS: RangeInclusive<u16> = 90..=1000;
pub(crate) const ROWS: RangeInclusive<u16> = 10..=500;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct StartServer {
    pub(crate) cols: Option<u16>,
    pub(crate) rows: Option<u16>,
}

impl StartServer {
    pub(crate) fn size(&self) -> Result<(u16, u16), String> {
        let cols = within("cols", self.cols.unwrap_or(DEFAULT_COLS), &COLS)?;
        let rows = within("rows", self.rows.unwrap_or(DEFAULT_ROWS), &ROWS)?;
        Ok((cols, rows))
    }
}

pub(super) fn within(name: &str, value: u16, range: &RangeInclusive<u16>) -> Result<u16, String> {
    if range.contains(&value) {
        Ok(value)
    } else {
        Err(format!("{name} must be between {} and {}, not {value}", range.start(), range.end()))
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum SizeFrom {
    #[default]
    Window,
    StartServer,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Started {
    pub(crate) already: bool,
    pub(crate) restored: bool,
    pub(crate) complete: bool,
    pub(crate) cols: u16,
    pub(crate) rows: u16,
    pub(crate) size: SizeFrom,
    pub(crate) boot: String,
    pub(crate) version: String,
}

#[cfg(test)]
mod tests;
