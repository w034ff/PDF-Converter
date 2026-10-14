//! The document creation time of a PDF/A document, in UTC and without a date-time dependency
//! (design §4.3).

use std::time::{SystemTime, UNIX_EPOCH};

const SECONDS_PER_MINUTE: u64 = 60;
const SECONDS_PER_HOUR: u64 = 60 * SECONDS_PER_MINUTE;
const SECONDS_PER_DAY: u64 = 24 * SECONDS_PER_HOUR;
/// Days from 0000-03-01 to 1970-01-01, the origin of [`utc_from_unix`]'s calendar arithmetic.
const DAYS_FROM_YEAR_0_MARCH_TO_UNIX_EPOCH: u64 = 719_468;
const DAYS_PER_400_YEARS: u64 = 146_097;
/// The last year a PDF date holds with four digits.
const MAX_YEAR: u64 = 9999;

/// A calendar time in UTC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UtcTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

/// 1970-01-01T00:00:00Z. Written when the clock cannot give a usable time: a PDF/A document
/// without a creation time cannot be finished, and this value is plainly not a real
/// creation time.
const FALLBACK: UtcTime = UtcTime {
    year: 1970,
    month: 1,
    day: 1,
    hour: 0,
    minute: 0,
    second: 0,
};

/// The current time in UTC, or [`FALLBACK`] when the clock is before 1970 or past the year 9999.
pub(crate) fn now_utc() -> UtcTime {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|elapsed| elapsed.as_secs());
    utc_or_fallback(seconds)
}

/// `seconds` as a calendar time, or [`FALLBACK`] if the clock gave none (`None`) or one past
/// [`MAX_YEAR`].
fn utc_or_fallback(seconds: Option<u64>) -> UtcTime {
    seconds.and_then(utc_from_unix).unwrap_or(FALLBACK)
}

/// Converts seconds since 1970-01-01T00:00:00Z to a calendar time, or `None` if the year is
/// past [`MAX_YEAR`].
///
/// The day count is turned into a date with Howard Hinnant's `civil_from_days` algorithm,
/// which counts years from March so that the leap day is the last day of a year.
pub(crate) fn utc_from_unix(seconds: u64) -> Option<UtcTime> {
    let days = seconds / SECONDS_PER_DAY;
    let seconds_of_day = seconds % SECONDS_PER_DAY;

    let shifted_days = days + DAYS_FROM_YEAR_0_MARCH_TO_UNIX_EPOCH;
    let era = shifted_days / DAYS_PER_400_YEARS;
    let day_of_era = shifted_days % DAYS_PER_400_YEARS;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_from_march = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_from_march + 2) / 5 + 1;
    let month = if month_from_march < 10 {
        month_from_march + 3
    } else {
        month_from_march - 9
    };
    let year = year_of_era + era * 400 + u64::from(month <= 2);

    if year > MAX_YEAR {
        return None;
    }
    Some(UtcTime {
        year: u16::try_from(year).ok()?,
        month: u8::try_from(month).ok()?,
        day: u8::try_from(day).ok()?,
        hour: u8::try_from(seconds_of_day / SECONDS_PER_HOUR).ok()?,
        minute: u8::try_from(seconds_of_day % SECONDS_PER_HOUR / SECONDS_PER_MINUTE).ok()?,
        second: u8::try_from(seconds_of_day % SECONDS_PER_MINUTE).ok()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn time(year: u16, month: u8, day: u8, hour: u8, minute: u8, second: u8) -> UtcTime {
        UtcTime {
            year,
            month,
            day,
            hour,
            minute,
            second,
        }
    }

    #[test]
    fn the_unix_epoch_is_1970_01_01() {
        assert_eq!(utc_from_unix(0), Some(time(1970, 1, 1, 0, 0, 0)));
    }

    #[test]
    fn leap_days_exist_in_leap_years_only() {
        // 2000-02-29T00:00:00Z (a year divisible by 400) and 2024-02-29T00:00:00Z.
        assert_eq!(utc_from_unix(951_782_400), Some(time(2000, 2, 29, 0, 0, 0)));
        assert_eq!(
            utc_from_unix(1_709_164_800),
            Some(time(2024, 2, 29, 0, 0, 0))
        );
        // 2100 is divisible by 100 but not by 400, so 2100-02-28 is followed by 2100-03-01.
        assert_eq!(
            utc_from_unix(4_107_456_000),
            Some(time(2100, 2, 28, 0, 0, 0))
        );
        assert_eq!(
            utc_from_unix(4_107_456_000 + SECONDS_PER_DAY),
            Some(time(2100, 3, 1, 0, 0, 0))
        );
    }

    #[test]
    fn the_end_of_a_year_stays_in_that_year() {
        assert_eq!(
            utc_from_unix(946_684_799),
            Some(time(1999, 12, 31, 23, 59, 59))
        );
        assert_eq!(utc_from_unix(946_684_800), Some(time(2000, 1, 1, 0, 0, 0)));
    }

    #[test]
    fn a_present_day_time_has_every_field() {
        // 2026-10-14T01:36:19Z
        assert_eq!(
            utc_from_unix(1_791_941_779),
            Some(time(2026, 10, 14, 1, 36, 19))
        );
    }

    #[test]
    fn a_year_past_the_pdf_date_range_is_none() {
        // 9999-12-31T23:59:59Z is the last second a PDF date holds.
        assert_eq!(
            utc_from_unix(253_402_300_799),
            Some(time(9999, 12, 31, 23, 59, 59))
        );
        assert_eq!(utc_from_unix(253_402_300_800), None);
        assert_eq!(utc_from_unix(u64::MAX), None);
    }

    #[test]
    fn an_unusable_clock_gives_the_fallback() {
        let fallback = time(1970, 1, 1, 0, 0, 0);
        assert_eq!(utc_or_fallback(None), fallback);
        assert_eq!(utc_or_fallback(Some(u64::MAX)), fallback);
        assert_eq!(
            utc_or_fallback(Some(951_782_400)),
            time(2000, 2, 29, 0, 0, 0)
        );
    }

    #[test]
    fn now_is_a_time_after_this_code_was_written() {
        assert!(now_utc().year >= 2026);
    }
}
