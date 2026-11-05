use std::fmt::{self, Display};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub(crate) struct Stamp(pub SystemTime);

impl Stamp {
    pub(crate) fn parse(text: &str) -> Option<SystemTime> {
        let (date, time) = text.strip_suffix('Z')?.split_once('T')?;
        let [year, month, day] = numbers(date, '-')?;
        let (clock, fraction) = time.split_once('.').unwrap_or((time, ""));
        let [hour, minute, second] = numbers(clock, ':')?;
        let valid = (1..=12).contains(&month) && (1..=31).contains(&day) && hour < 24 && minute < 60 && second < 60;
        if !valid || fraction.len() > 9 || !fraction.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let nanos = format!("{fraction:0<9}").parse().ok()?;
        let days = u64::try_from(days_from_civil(
            i64::try_from(year).ok().filter(|year| *year < 1 << 40)?,
            month.cast_signed(),
            day.cast_signed(),
        ))
        .ok()?;
        let secs = days.checked_mul(86_400)?.checked_add(hour * 3600 + minute * 60 + second)?;
        UNIX_EPOCH.checked_add(Duration::new(secs, nanos))
    }
}

fn numbers(text: &str, separator: char) -> Option<[u64; 3]> {
    let mut parts = text.split(separator).map(|part| part.bytes().all(|b| b.is_ascii_digit()).then(|| part.parse()));
    let found = [parts.next()??.ok()?, parts.next()??.ok()?, parts.next()??.ok()?];
    parts.next().is_none().then_some(found)
}

impl Display for Stamp {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let since = self.0.duration_since(UNIX_EPOCH).unwrap_or_default();
        let secs = since.as_secs();
        let (year, month, day) = civil_from_days((secs / 86_400).cast_signed());
        let (hour, minute, second) = (secs / 3600 % 24, secs / 60 % 60, secs % 60);
        let millis = since.subsec_millis();
        write!(f, "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z")
    }
}

pub(crate) fn age(secs: i64) -> String {
    let minutes = secs.max(0) / 60;
    let hours = minutes / 60;
    let days = hours / 24;
    if minutes < 60 {
        format!("{minutes}m")
    } else if hours < 24 {
        format!("{hours}h")
    } else if days < 30 {
        format!("{days}d")
    } else if days < 365 {
        format!("{}mo", days / 30)
    } else {
        format!("{}y", days / 365)
    }
}

pub(crate) fn ago(elapsed: Duration) -> String {
    let minutes = elapsed.as_secs() / 60;
    let hours = minutes / 60;
    let days = hours / 24;
    if minutes < 1 {
        String::from("just now")
    } else if hours < 1 {
        format!("{minutes} min ago")
    } else if days < 1 {
        format!("{hours}h ago")
    } else if days == 1 {
        String::from("1 day ago")
    } else {
        format!("{days} days ago")
    }
}

pub(crate) fn parse_time(text: &str) -> Option<i64> {
    let field = |from: usize, to: usize| text.get(from..to)?.parse::<i64>().ok();
    let (year, month, day) = (field(0, 4)?, field(5, 7)?, field(8, 10)?);
    let (hour, minute, second) = (field(11, 13)?, field(14, 16)?, field(17, 19)?);
    let local = days_from_civil(year, month, day) * 86_400 + hour * 3_600 + minute * 60 + second;
    Some(local - offset(text.get(19..).unwrap_or_default()))
}

fn offset(rest: &str) -> i64 {
    let zone = rest.trim_start_matches(|c: char| c == '.' || c.is_ascii_digit());
    let sign = match zone.chars().next() {
        Some('+') => 1,
        Some('-') => -1,
        _ => return 0,
    };
    let digits: String = zone.chars().filter(char::is_ascii_digit).collect();
    let part = |range: std::ops::Range<usize>| digits.get(range).and_then(|d| d.parse::<i64>().ok()).unwrap_or(0);
    sign * (part(0..2) * 3_600 + part(2..4) * 60)
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let day_of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

pub(crate) fn now() -> i64 {
    secs(SystemTime::now())
}

pub(crate) fn secs(at: SystemTime) -> i64 {
    at.duration_since(UNIX_EPOCH).map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

pub(crate) fn date(millis: i64) -> String {
    let (year, month, day) = civil_from_days(millis.div_euclid(86_400_000));
    format!("{year:04}-{month:02}-{day:02}")
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let days = days + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days - era * 146_097;
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted + 2) / 5 + 1;
    let month = if shifted < 10 { shifted + 3 } else { shifted - 9 };
    (year_of_era + era * 400 + i64::from(month <= 2), month, day)
}

#[cfg(test)]
mod tests;
