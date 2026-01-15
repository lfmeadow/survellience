use chrono::{DateTime, Timelike, Utc};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TimeBucket {
    pub date: chrono::NaiveDate,
    pub hour: u32,
    pub minute: u32,
    pub bucket_minutes: u64,
}

impl TimeBucket {
    pub fn from_timestamp(ts_ms: i64, bucket_minutes: u64) -> Self {
        let dt = DateTime::<Utc>::from_timestamp_millis(ts_ms)
            .unwrap_or_else(|| Utc::now());
        let naive = dt.naive_utc();
        let date = naive.date();
        let hour = naive.hour();
        let minute_bucket = (naive.minute() / bucket_minutes as u32) * bucket_minutes as u32;
        
        Self {
            date,
            hour,
            minute: minute_bucket,
            bucket_minutes,
        }
    }

    pub fn from_now(bucket_minutes: u64) -> Self {
        Self::from_timestamp(Utc::now().timestamp_millis(), bucket_minutes)
    }

    pub fn path_segments(&self) -> (String, String) {
        let date_str = self.date.format("%Y-%m-%d").to_string();
        let hour_str = format!("{:02}", self.hour);
        (date_str, hour_str)
    }

    pub fn file_prefix(&self) -> String {
        let (date_str, hour_str) = self.path_segments();
        let minute_str = format!("{:02}", self.minute);
        format!("snapshots_{}T{}-{}", date_str, hour_str, minute_str)
    }

    pub fn next_bucket(&self) -> Self {
        let current_naive = self.date.and_hms_opt(self.hour, self.minute, 0).unwrap();
        let next_naive = current_naive + chrono::Duration::minutes(self.bucket_minutes as i64);
        
        Self {
            date: next_naive.date(),
            hour: next_naive.hour(),
            minute: (next_naive.minute() / self.bucket_minutes as u32) * self.bucket_minutes as u32,
            bucket_minutes: self.bucket_minutes,
        }
    }
}

impl fmt::Display for TimeBucket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "date={}/hour={:02}/{}",
            self.date.format("%Y-%m-%d"),
            self.hour,
            self.file_prefix()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time_bucket_from_timestamp() {
        // 2024-01-15 14:37:00 UTC
        let ts = DateTime::parse_from_rfc3339("2024-01-15T14:37:00Z")
            .unwrap()
            .timestamp_millis();
        
        let bucket = TimeBucket::from_timestamp(ts, 5);
        assert_eq!(bucket.date, chrono::NaiveDate::from_ymd_opt(2024, 1, 15).unwrap());
        assert_eq!(bucket.hour, 14);
        assert_eq!(bucket.minute, 35); // Rounded down to 5-minute bucket
    }

    #[test]
    fn test_time_bucket_path_segments() {
        let bucket = TimeBucket {
            date: chrono::NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
            hour: 14,
            minute: 35,
            bucket_minutes: 5,
        };
        
        let (date, hour) = bucket.path_segments();
        assert_eq!(date, "2024-01-15");
        assert_eq!(hour, "14");
    }

    #[test]
    fn test_time_bucket_file_prefix() {
        let bucket = TimeBucket {
            date: chrono::NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
            hour: 14,
            minute: 35,
            bucket_minutes: 5,
        };
        
        assert_eq!(bucket.file_prefix(), "snapshots_2024-01-15T14-35");
    }

    #[test]
    fn test_next_bucket() {
        let bucket = TimeBucket {
            date: chrono::NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
            hour: 14,
            minute: 35,
            bucket_minutes: 5,
        };
        
        let next = bucket.next_bucket();
        assert_eq!(next.minute, 40);
    }
}
