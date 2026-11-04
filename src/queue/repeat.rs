use std::fmt;

use jiff::civil::{Date, Time, Weekday};
use jiff::tz::TimeZone;
use jiff::{Timestamp, ToSpan};
use serde::{Deserialize, Serialize};

use super::clock;

pub(crate) const MIN_GAP: i64 = 30 * 60;
const MAX_GAP: i64 = 366 * 86_400;
const USE: &str = "use every 2h, every 3d, every day 09:00, every weekday 09:00 or every monday 09:00";
const TOO_OFTEN: &str = "a routine repeats at most every 30 minutes, since each run makes a worktree";
const TOO_RARE: &str = "a routine repeats at least once a year";
const DAYS: [(&str, Weekday); 7] = [
    ("monday", Weekday::Monday),
    ("tuesday", Weekday::Tuesday),
    ("wednesday", Weekday::Wednesday),
    ("thursday", Weekday::Thursday),
    ("friday", Weekday::Friday),
    ("saturday", Weekday::Saturday),
    ("sunday", Weekday::Sunday),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Unit {
    Minutes,
    Hours,
    Days,
    Weeks,
}

impl Unit {
    fn letter(self) -> char {
        match self {
            Self::Minutes => 'm',
            Self::Hours => 'h',
            Self::Days => 'd',
            Self::Weeks => 'w',
        }
    }

    fn seconds(self) -> i64 {
        match self {
            Self::Minutes => 60,
            Self::Hours => 3600,
            Self::Days => 86_400,
            Self::Weeks => 604_800,
        }
    }

    fn of(letter: char) -> Option<Self> {
        [Self::Minutes, Self::Hours, Self::Days, Self::Weeks].into_iter().find(|unit| unit.letter() == letter)
    }

    fn civil(self) -> bool {
        matches!(self, Self::Days | Self::Weeks)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub(crate) enum Repeat {
    Every { amount: u32, unit: Unit, clock: Option<Time> },
    Day(Time),
    Weekday(Time),
    Week(Weekday, Time),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Walk {
    pub(crate) last: i64,
    pub(crate) skipped: bool,
    pub(crate) next: i64,
}

fn failed(error: &impl ToString) -> String {
    error.to_string()
}

fn weekday_of(name: &str) -> Option<Weekday> {
    DAYS.iter().find(|(day, _)| *day == name).map(|(_, weekday)| *weekday)
}

fn name_of(weekday: Weekday) -> &'static str {
    DAYS.iter().find(|(_, day)| *day == weekday).map_or("monday", |(name, _)| name)
}

fn clock_of(text: &str) -> Result<Time, String> {
    let (hour, minute) = text.split_once(':').ok_or(USE)?;
    if hour.len() > 2 || minute.len() != 2 {
        return Err(USE.into());
    }
    clock::time(text)
}

fn every(head: &str) -> Result<(u32, Unit), String> {
    let split = head.len().saturating_sub(1);
    let unit = head.get(split..).and_then(|letter| letter.chars().next()).and_then(Unit::of).ok_or(USE)?;
    let amount = head.get(..split).and_then(|number| number.parse::<u32>().ok()).ok_or(USE)?;
    Ok((amount, unit))
}

fn read(text: &str) -> Result<(Repeat, bool), String> {
    let lower = text.trim().to_lowercase();
    let lower = lower.strip_prefix("every ").unwrap_or(&lower).trim();
    let mut words = lower.split_whitespace();
    let head = words.next().ok_or(USE)?;
    let mut rest: Vec<&str> = words.collect();
    if rest.first() == Some(&"at") {
        rest.remove(0);
    }
    let clock = match rest.as_slice() {
        [] => None,
        [one] => Some(clock_of(one)?),
        _ => return Err(USE.into()),
    };
    let given = clock.is_some();
    let repeat = match (head, clock) {
        ("day", Some(clock)) => Repeat::Day(clock),
        ("weekday", Some(clock)) => Repeat::Weekday(clock),
        (name, Some(clock)) if weekday_of(name).is_some() => Repeat::Week(weekday_of(name).ok_or(USE)?, clock),
        ("day" | "weekday", None) => return Err(USE.into()),
        (name, None) if weekday_of(name).is_some() => return Err(USE.into()),
        (head, clock) => {
            let (amount, unit) = every(head)?;
            if clock.is_some() && !unit.civil() {
                return Err(USE.into());
            }
            Repeat::Every { amount, unit, clock }
        }
    };
    Ok((repeat, given))
}

fn at_clock(zone: &TimeZone, date: Date, time: Time) -> Result<i64, String> {
    zone.to_ambiguous_timestamp(date.to_datetime(time))
        .compatible()
        .map(Timestamp::as_second)
        .map_err(|error| failed(&error))
}

fn date_of(zone: &TimeZone, at: i64) -> Result<Date, String> {
    Ok(Timestamp::from_second(at).map_err(|error| failed(&error))?.to_zoned(zone.clone()).date())
}

fn clock_in(zone: &TimeZone, at: i64) -> Result<Time, String> {
    Ok(Timestamp::from_second(at).map_err(|error| failed(&error))?.to_zoned(zone.clone()).time())
}

fn plus_days(date: Date, days: i64) -> Result<Date, String> {
    date.checked_add(days.days()).map_err(|error| failed(&error))
}

impl Repeat {
    pub(crate) fn parse(text: &str, zone: &str, now: i64) -> Result<Self, String> {
        let (repeat, given) = read(text)?;
        let repeat = match repeat {
            Repeat::Every { amount, unit, clock: None } if unit.civil() && !given => {
                let zone = clock::zone(zone)?;
                Repeat::Every { amount, unit, clock: Some(clock_in(&zone, now)?) }
            }
            other => other,
        };
        repeat.checked()
    }

    fn checked(self) -> Result<Self, String> {
        if let Self::Every { amount, unit, .. } = self {
            let seconds = i64::from(amount) * unit.seconds();
            if seconds < MIN_GAP {
                return Err(TOO_OFTEN.into());
            }
            if seconds > MAX_GAP {
                return Err(TOO_RARE.into());
            }
        }
        Ok(self)
    }

    fn clock(self) -> Option<Time> {
        match self {
            Self::Every { clock, .. } => clock,
            Self::Day(clock) | Self::Weekday(clock) | Self::Week(_, clock) => Some(clock),
        }
    }

    fn with_clock(self, clock: Option<Time>) -> Self {
        match self {
            Self::Every { amount, unit, .. } => Self::Every { amount, unit, clock },
            other => other,
        }
    }

    pub(crate) fn accepts(self, text: &str) -> bool {
        match read(text) {
            Ok((other, true)) => other == self,
            Ok((other, false)) => other.with_clock(self.clock()) == self,
            Err(_) => false,
        }
    }

    fn matches(self, date: Date) -> bool {
        match self {
            Self::Every { .. } | Self::Day(_) => true,
            Self::Weekday(_) => !matches!(date.weekday(), Weekday::Saturday | Weekday::Sunday),
            Self::Week(day, _) => date.weekday() == day,
        }
    }

    fn step(self) -> Option<(i64, Unit)> {
        match self {
            Self::Every { amount, unit, .. } => Some((i64::from(amount), unit)),
            _ => None,
        }
    }

    fn absolute(self) -> Option<i64> {
        self.step().filter(|(_, unit)| !unit.civil()).map(|(amount, unit)| amount * unit.seconds())
    }

    fn on_date(self, zone: &TimeZone, date: Date) -> Result<i64, String> {
        at_clock(zone, date, self.clock().ok_or(USE)?)
    }

    fn after_date(self, date: Date) -> Result<Date, String> {
        if let Some((amount, unit)) = self.step() {
            return plus_days(date, amount * unit.seconds() / 86_400);
        }
        let mut next = plus_days(date, 1)?;
        while !self.matches(next) {
            next = plus_days(next, 1)?;
        }
        Ok(next)
    }

    pub(crate) fn first(self, zone: &str, now: i64) -> Result<i64, String> {
        let zone = clock::zone(zone)?;
        if let Some(step) = self.absolute() {
            return now.checked_add(step).ok_or_else(|| TOO_RARE.into());
        }
        if self.step().is_some() {
            return self.on_date(&zone, self.after_date(date_of(&zone, now)?)?);
        }
        let mut date = date_of(&zone, now)?;
        while !self.matches(date) || self.on_date(&zone, date)? <= now {
            date = plus_days(date, 1)?;
        }
        self.on_date(&zone, date)
    }

    fn intended_date(self, zone: &TimeZone, at: i64) -> Result<Date, String> {
        let date = date_of(zone, at)?;
        match self.clock() {
            Some(clock) if self.step().is_none_or(|(_, unit)| unit.civil()) && clock_in(zone, at)? < clock => {
                plus_days(date, -1)
            }
            _ => Ok(date),
        }
    }

    pub(crate) fn after(self, zone: &str, at: i64) -> Result<i64, String> {
        if let Some(step) = self.absolute() {
            return at.checked_add(step).ok_or_else(|| TOO_RARE.into());
        }
        let zone = clock::zone(zone)?;
        self.on_date(&zone, self.after_date(self.intended_date(&zone, at)?)?)
    }

    pub(crate) fn walk(self, zone: &str, at: i64, now: i64) -> Result<Walk, String> {
        if let Some(step) = self.absolute() {
            let times = (now - at).max(0) / step;
            let last = at + times * step;
            return Ok(Walk { last, skipped: last > at, next: last + step });
        }
        let mut last = at;
        let mut next = self.after(zone, last)?;
        while next <= now {
            last = next;
            next = self.after(zone, last)?;
        }
        Ok(Walk { last, skipped: last > at, next })
    }
}

impl fmt::Display for Repeat {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let clock = |time: &Time| time.strftime("%H:%M").to_string();
        match self {
            Self::Every { amount, unit, clock: None } => write!(formatter, "{amount}{}", unit.letter()),
            Self::Every { amount, unit, clock: Some(time) } => {
                write!(formatter, "{amount}{} {}", unit.letter(), clock(time))
            }
            Self::Day(time) => write!(formatter, "day {}", clock(time)),
            Self::Weekday(time) => write!(formatter, "weekday {}", clock(time)),
            Self::Week(day, time) => write!(formatter, "{} {}", name_of(*day), clock(time)),
        }
    }
}

impl TryFrom<String> for Repeat {
    type Error = String;

    fn try_from(text: String) -> Result<Self, String> {
        match read(&text)? {
            (Repeat::Every { unit, clock: None, .. }, _) if unit.civil() => Err(USE.into()),
            (repeat, _) => repeat.checked(),
        }
    }
}

impl From<Repeat> for String {
    fn from(repeat: Repeat) -> Self {
        repeat.to_string()
    }
}

#[cfg(test)]
mod tests;
