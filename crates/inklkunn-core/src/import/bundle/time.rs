//! RFC 3339 times as the instants they are (RFC 007 D-2: *compared as instants, never as strings*).
//!
//! A history record's `at` is read here and nowhere else, so that `2024-03-02T10:15:00+09:00` and `2024-03-02T01:15:00Z`
//! are one moment, and `2024-03-02T01:15:00.5Z` is half a second after both. A text that is not an RFC 3339 date-time
//! (`T` between date and time, a zone that is `Z` or a signed `hh:mm`) is `None`: it is a refusal of the record, never a guess.

/// A moment: whole seconds since the Unix epoch (UTC) and the nanoseconds after them. The derived order is the order of
/// time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Instant {
    /// Whole seconds since 1970-01-01T00:00:00Z.
    pub secs: i64,
    /// Nanoseconds after `secs`, below one second.
    pub nanos: u32,
}

/// The digits of `part` as a number, when it is non-empty ASCII digits and nothing else.
fn number(part: &str) -> Option<u32> {
    if part.is_empty() || part.len() > 9 || !part.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    part.parse().ok()
}

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if is_leap(year) => 29,
        _ => 28,
    }
}

/// Days from 1970-01-01 to the civil date (proleptic Gregorian; Howard Hinnant's `days_from_civil`).
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = i64::from(month);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The two-digit field `part` names, as a number.
fn two(part: &str) -> Option<u32> {
    if part.len() == 2 { number(part) } else { None }
}

/// The moment `text` names, or `None` when it is not an RFC 3339 date-time.
pub fn parse(text: &str) -> Option<Instant> {
    let (date, rest) = text.split_once(['T', 't'])?;
    let mut d = date.split('-');
    let year_text = d.next()?;
    if year_text.len() != 4 {
        return None;
    }
    let year = i64::from(number(year_text)?);
    let month = two(d.next()?)?;
    let day = two(d.next()?)?;
    if d.next().is_some()
        || !(1..=12).contains(&month)
        || day == 0
        || day > days_in_month(year, month)
    {
        return None;
    }
    // The clock and the zone: `hh:mm:ss[.f…]` then `Z` or `±hh:mm`.
    let zone_at = rest.find(['Z', 'z', '+', '-'])?;
    let (clock, zone) = rest.split_at_checked(zone_at)?;
    let (hms, fraction) = match clock.split_once('.') {
        Some((hms, fraction)) => (hms, Some(fraction)),
        None => (clock, None),
    };
    let mut c = hms.split(':');
    let hour = two(c.next()?)?;
    let minute = two(c.next()?)?;
    let second = two(c.next()?)?;
    // A leap second (`:60`) is a time RFC 3339 allows; it sorts after `:59` and before the next minute.
    if c.next().is_some() || hour > 23 || minute > 59 || second > 60 {
        return None;
    }
    let nanos = match fraction {
        None => 0,
        Some(f) => {
            if f.is_empty() || !f.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            // Digits past nanoseconds do not change an order that nanoseconds already decide.
            let mut padded: String = f.chars().take(9).collect();
            while padded.len() < 9 {
                padded.push('0');
            }
            padded.parse::<u32>().ok()?
        }
    };
    let offset_secs: i64 = match zone {
        "Z" | "z" => 0,
        _ => {
            let sign = match zone.chars().next()? {
                '+' => 1,
                '-' => -1,
                _ => return None,
            };
            let (hh, mm) = zone.get(1..)?.split_once(':')?;
            let (hh, mm) = (two(hh)?, two(mm)?);
            if hh > 23 || mm > 59 {
                return None;
            }
            sign * (i64::from(hh) * 3600 + i64::from(mm) * 60)
        }
    };
    let secs = days_from_civil(year, month, day) * 86_400
        + i64::from(hour) * 3600
        + i64::from(minute) * 60
        + i64::from(second)
        - offset_secs;
    Some(Instant { secs, nanos })
}
