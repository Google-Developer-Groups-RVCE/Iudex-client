use chrono::{DateTime, Duration, Utc};

use crate::api::models::Contest;
use crate::error::{ClientError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContestStatus {
    Upcoming,
    Running,
    Ended,
}

#[derive(Debug, Clone)]
pub struct ContestTiming {
    pub status: ContestStatus,
    /// Time until the contest starts (Upcoming) or ends (Running). `None` once
    /// the contest has ended, since there is nothing left to count down to.
    pub time_remaining: Option<Duration>,
}

/// Classifies a contest relative to `now`. `now` is injected rather than read
/// from the clock inside so the branches stay deterministically testable.
pub fn contest_timing(contest: &Contest, now: DateTime<Utc>) -> Result<ContestTiming> {
    let start = parse_time(&contest.start_time)?;
    let end = parse_time(&contest.end_time)?;

    let timing = if now < start {
        ContestTiming {
            status: ContestStatus::Upcoming,
            time_remaining: Some(start - now),
        }
    } else if now < end {
        ContestTiming {
            status: ContestStatus::Running,
            time_remaining: Some(end - now),
        }
    } else {
        ContestTiming {
            status: ContestStatus::Ended,
            time_remaining: None,
        }
    };

    Ok(timing)
}

fn parse_time(value: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|err| ClientError::ParseError(format!("invalid timestamp '{}': {}", value, err)))
}

/// Renders a coarse, human-facing duration ("2d 3h", "4h 12m", "45m"). Negative
/// durations are clamped to zero so a just-expired countdown never reads oddly.
pub fn format_duration(duration: Duration) -> String {
    let total = duration.num_seconds().max(0);
    let days = total / 86_400;
    let hours = (total % 86_400) / 3_600;
    let minutes = (total % 3_600) / 60;

    if days > 0 {
        format!("{}d {}h", days, hours)
    } else if hours > 0 {
        format!("{}h {}m", hours, minutes)
    } else {
        format!("{}m", minutes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::models::Contest;

    fn contest_with_window(start: &str, end: &str) -> Contest {
        Contest {
            id: "c".to_string(),
            title: "Test".to_string(),
            description: String::new(),
            start_time: start.to_string(),
            end_time: end.to_string(),
            problems: Vec::new(),
        }
    }

    fn at(value: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(value)
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn before_start_is_upcoming() {
        let contest = contest_with_window("2026-08-08T10:00:00Z", "2026-08-08T12:00:00Z");
        let timing = contest_timing(&contest, at("2026-08-08T09:00:00Z")).unwrap();
        assert_eq!(timing.status, ContestStatus::Upcoming);
        assert_eq!(timing.time_remaining.unwrap().num_minutes(), 60);
    }

    #[test]
    fn inside_window_is_running() {
        let contest = contest_with_window("2026-08-08T10:00:00Z", "2026-08-08T12:00:00Z");
        let timing = contest_timing(&contest, at("2026-08-08T11:30:00Z")).unwrap();
        assert_eq!(timing.status, ContestStatus::Running);
        assert_eq!(timing.time_remaining.unwrap().num_minutes(), 30);
    }

    #[test]
    fn after_end_is_ended_with_no_remaining() {
        let contest = contest_with_window("2026-08-08T10:00:00Z", "2026-08-08T12:00:00Z");
        let timing = contest_timing(&contest, at("2026-08-08T13:00:00Z")).unwrap();
        assert_eq!(timing.status, ContestStatus::Ended);
        assert!(timing.time_remaining.is_none());
    }

    #[test]
    fn malformed_timestamp_is_an_error() {
        let contest = contest_with_window("not-a-date", "2026-08-08T12:00:00Z");
        assert!(contest_timing(&contest, at("2026-08-08T11:00:00Z")).is_err());
    }

    #[test]
    fn format_duration_picks_coarsest_units() {
        assert_eq!(format_duration(Duration::minutes(45)), "45m");
        assert_eq!(format_duration(Duration::minutes(252)), "4h 12m");
        assert_eq!(format_duration(Duration::hours(51)), "2d 3h");
        assert_eq!(format_duration(Duration::seconds(-10)), "0m");
    }
}
