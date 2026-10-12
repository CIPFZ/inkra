//! Minimal UTC timestamp formatting without a date-time dependency.

use std::time::{SystemTime, UNIX_EPOCH};

/// Broken-down UTC time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Utc {
    /// Year, e.g. 2026.
    pub year: i64,
    /// Month 1..=12.
    pub month: u32,
    /// Day 1..=31.
    pub day: u32,
    /// Hour 0..=23.
    pub hour: u32,
    /// Minute 0..=59.
    pub minute: u32,
    /// Second 0..=59.
    pub second: u32,
    /// Millisecond 0..=999.
    pub millis: u32,
}

impl Utc {
    /// Converts a [`SystemTime`] (times before 1970 clamp to the epoch).
    pub fn from_system(t: SystemTime) -> Self {
        let d = t.duration_since(UNIX_EPOCH).unwrap_or_default();
        let secs = d.as_secs() as i64;
        let days = secs.div_euclid(86_400);
        let rem = secs.rem_euclid(86_400);
        // Howard Hinnant's civil_from_days.
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
        let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
        let year = yoe + era * 400 + i64::from(month <= 2);
        Self {
            year,
            month,
            day,
            hour: (rem / 3600) as u32,
            minute: (rem % 3600 / 60) as u32,
            second: (rem % 60) as u32,
            millis: d.subsec_millis(),
        }
    }

    /// The current time.
    pub fn now() -> Self {
        Self::from_system(SystemTime::now())
    }

    /// `YYYYMMDDHHmmss`.
    pub fn compact(&self) -> String {
        format!(
            "{:04}{:02}{:02}{:02}{:02}{:02}",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }

    /// `YYYYMMDDHHmmssSSS`.
    pub fn compact_millis(&self) -> String {
        format!("{}{:03}", self.compact(), self.millis)
    }

    /// `YYYY-MM-DD`.
    pub fn date(&self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }

    /// `YYYY-MM-DD HH:mm`.
    pub fn date_time(&self) -> String {
        format!("{} {:02}:{:02}", self.date(), self.hour, self.minute)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn known_dates() {
        let t = UNIX_EPOCH + Duration::from_millis(1_791_844_860_123);
        let u = Utc::from_system(t);
        assert_eq!((u.year, u.month, u.day), (2026, 10, 12));
        assert_eq!(u.compact_millis(), "20261012224100123");
        let leap = Utc::from_system(UNIX_EPOCH + Duration::from_secs(951_782_400));
        assert_eq!(leap.date(), "2000-02-29");
        assert_eq!(Utc::from_system(UNIX_EPOCH).compact(), "19700101000000");
    }
}
