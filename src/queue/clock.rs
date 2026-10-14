use jiff::civil::{DateTime, Time};
use jiff::tz::TimeZone;
use jiff::{Timestamp, ToSpan};

pub(crate) fn local_zone() -> String {
    let now = jiff::Zoned::now();
    now.time_zone().iana_name().map_or_else(|| now.offset().to_string(), str::to_string)
}

fn zone(name: &str) -> Result<TimeZone, String> {
    jiff::fmt::temporal::DateTimeParser::new().parse_time_zone(name).map_err(|error| error.to_string())
}

fn time(text: &str) -> Result<Time, String> {
    let (hour, minute) = text.split_once(':').ok_or("use a time such as 14:30")?;
    Time::new(hour.parse().map_err(|_| "invalid hour")?, minute.parse().map_err(|_| "invalid minute")?, 0, 0)
        .map_err(|error| error.to_string())
}

pub(crate) fn parse(text: &str, name: &str, now: i64) -> Result<i64, String> {
    let text = text.trim();
    if let Some(delay) = text.strip_prefix("in ") {
        let at = delay.len().saturating_sub(1);
        let number = delay.get(..at).ok_or("use in 30m or in 2h")?;
        let unit = delay.get(at..).ok_or("use in 30m or in 2h")?;
        let multiplier = match unit {
            "m" => 60,
            "h" => 3600,
            "d" => 86400,
            _ => return Err("use in 30m or in 2h".into()),
        };
        let seconds = number
            .parse::<i64>()
            .ok()
            .filter(|number| *number > 0)
            .and_then(|number| number.checked_mul(multiplier))
            .ok_or("the delay must be positive and within range")?;
        return now
            .checked_add(seconds)
            .filter(|at| Timestamp::from_second(*at).is_ok())
            .ok_or_else(|| "the delay is too long".into());
    }
    let zone = zone(name)?;
    let today = Timestamp::from_second(now).map_err(|error| error.to_string())?.to_zoned(zone.clone()).date();
    let datetime = if let Some(clock) = text.strip_prefix("tomorrow ") {
        today.checked_add(1.day()).map_err(|error| error.to_string())?.to_datetime(time(clock)?)
    } else if text.contains(' ') {
        let (date, clock) = text.split_once(' ').ok_or("use 2026-10-08 09:00")?;
        date.parse::<jiff::civil::Date>().map_err(|error| error.to_string())?.to_datetime(time(clock)?)
    } else {
        let mut datetime = today.to_datetime(time(text)?);
        if instant(&zone, datetime)? <= now {
            datetime = datetime.checked_add(1.day()).map_err(|error| error.to_string())?;
        }
        datetime
    };
    let at = instant(&zone, datetime)?;
    if at <= now {
        return Err("choose a time in the future".into());
    }
    Ok(at)
}

fn instant(zone: &TimeZone, datetime: DateTime) -> Result<i64, String> {
    zone.to_ambiguous_timestamp(datetime)
        .unambiguous()
        .map(Timestamp::as_second)
        .map_err(|error| format!("that local time is ambiguous or does not exist: {error}"))
}

pub(crate) fn display(at: i64, name: &str) -> String {
    Timestamp::from_second(at).ok().zip(zone(name).ok()).map_or_else(
        || at.to_string(),
        |(timestamp, zone)| timestamp.to_zoned(zone).strftime("%Y-%m-%d %H:%M").to_string(),
    )
}

#[cfg(test)]
mod tests;
