//! Calendar days, as a licence writes them (`2027-06-30`), counted from 1 January 1970 (UTC).

use std::time::{SystemTime, UNIX_EPOCH};

/// A day, as days since 1 January 1970.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Day(pub i64);

impl Day {
    /// Today, by the computer's clock (UTC).
    pub fn today() -> Day {
        let secs = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
        Day(secs.div_euclid(86_400))
    }

    /// Reads `YYYY-MM-DD`.
    pub fn parse(text: &str) -> Option<Day> {
        let mut parts = text.trim().splitn(3, '-');
        let year: i64 = parts.next()?.parse().ok()?;
        let month: i64 = parts.next()?.parse().ok()?;
        let day: i64 = parts.next()?.parse().ok()?;
        if !(1..=12).contains(&month) || day < 1 || day > days_in_month(year, month) {
            return None;
        }
        Some(Day(days_from_civil(year, month, day)))
    }

    pub fn plus(self, days: i64) -> Day {
        Day(self.0 + days)
    }
}

impl std::fmt::Display for Day {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (y, m, d) = civil_from_days(self.0);
        write!(f, "{y:04}-{m:02}-{d:02}")
    }
}

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        2 if is_leap(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Howard Hinnant's days-from-civil, for the proleptic Gregorian calendar.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::Day;

    #[test]
    fn days_count_from_1970_and_read_back() {
        assert_eq!(Day::parse("1970-01-01"), Some(Day(0)));
        assert_eq!(Day::parse("1970-01-02"), Some(Day(1)));
        // 2000-01-01 is day 10,957; January (31) and a leap February (29) on.
        assert_eq!(Day::parse("2000-03-01"), Some(Day(10_957 + 60)));
        for text in ["2024-02-29", "2026-11-22", "2027-06-30", "1999-12-31"] {
            assert_eq!(Day::parse(text).unwrap().to_string(), text);
        }
        for bad in ["2025-02-29", "2026-13-01", "2026-04-31", "26-1", "", "2026-01-xx"] {
            assert_eq!(Day::parse(bad), None, "{bad}");
        }
    }
}
