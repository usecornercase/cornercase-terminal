use jiff::civil::Date;
use jiff::tz::TimeZone;
use jiff::{Timestamp, ToSpan};

use super::journal::{Entry, Event};
use super::{Choice, KEPT_DAYS};
use crate::queue::clock;

const LOOKBACK_DAYS: i64 = KEPT_DAYS;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Range {
    pub(crate) choice: Choice,
    pub(crate) from: i64,
    pub(crate) to: i64,
    pub(crate) label: String,
    pub(crate) day: Option<String>,
    pub(crate) zone: String,
}

pub(crate) fn zone_of(name: &str) -> TimeZone {
    clock::zone(name).unwrap_or(TimeZone::UTC)
}

fn local_date(seconds: i64, zone: &TimeZone) -> Option<Date> {
    Timestamp::from_second(seconds).ok().map(|at| at.to_zoned(zone.clone()).date())
}

fn start_of(date: Date, zone: &TimeZone) -> Option<i64> {
    date.to_zoned(zone.clone()).ok().map(|zoned| zoned.timestamp().as_second())
}

fn active(entry: &Entry) -> bool {
    entry.event == Event::Prompt || (entry.event == Event::Status && entry.to.as_deref() == Some("working"))
}

pub(crate) fn last_active_day(entries: &[Entry], now: i64, zone: &TimeZone) -> Option<Date> {
    let today = local_date(now, zone)?;
    entries
        .iter()
        .filter(|entry| active(entry))
        .filter_map(Entry::seconds)
        .filter_map(|at| local_date(at, zone))
        .filter(|day| *day < today)
        .max()
}

pub(crate) fn lookback_from(now: i64, zone: &TimeZone) -> i64 {
    local_date(now, zone)
        .and_then(|today| today.checked_sub(LOOKBACK_DAYS.days()).ok())
        .and_then(|day| start_of(day, zone))
        .unwrap_or(now - LOOKBACK_DAYS * 86_400)
}

fn lower(date: Date, zone: &TimeZone, now: i64, clock: bool) -> String {
    let format = if clock { "%a %-d %b %H:%M" } else { "%a %-d %b" };
    let stamp = if clock {
        Timestamp::from_second(now).ok()
    } else {
        start_of(date, zone).and_then(|s| Timestamp::from_second(s).ok())
    };
    stamp.map(|at| at.to_zoned(zone.clone()).strftime(format).to_string().to_lowercase()).unwrap_or_default()
}

pub(crate) fn resolve(choice: Choice, entries: &[Entry], now: i64, zone_name: &str) -> Option<Range> {
    let zone = zone_of(zone_name);
    let today = local_date(now, &zone)?;
    let zone_name = zone_name.to_string();
    match choice {
        Choice::Now => Some(Range {
            choice,
            from: now - 86_400,
            to: now,
            label: lower(today, &zone, now, true),
            day: None,
            zone: zone_name,
        }),
        Choice::Yesterday => {
            let day = last_active_day(entries, now, &zone)?;
            let (from, to) = (start_of(day, &zone)?, start_of(day.tomorrow().ok()?, &zone)?);
            let label = format!("yesterday · {}", lower(day, &zone, now, false));
            Some(Range { choice, from, to, label, day: Some(day.to_string()), zone: zone_name })
        }
        Choice::Week => {
            let first = today.checked_sub(6.days()).ok()?;
            Some(Range {
                choice,
                from: start_of(first, &zone)?,
                to: now,
                label: String::from("7 days"),
                day: Some(today.to_string()),
                zone: zone_name,
            })
        }
    }
}

pub(crate) fn stamp(at: i64, zone_name: &str) -> String {
    let zone = zone_of(zone_name);
    Timestamp::from_second(at)
        .map(|at| at.to_zoned(zone).strftime("%a %-d %b %H:%M").to_string().to_lowercase())
        .unwrap_or_default()
}

pub(crate) fn today_of(now: i64, zone_name: &str) -> Option<String> {
    local_date(now, &zone_of(zone_name)).map(|date| date.to_string())
}

#[cfg(test)]
mod tests;
