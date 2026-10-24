//! Expanding events into concrete occurrences, and the simple repeat presets
//! the editor offers on top of raw RRULEs.

use chrono::{Datelike, NaiveDateTime, TimeZone, Weekday};
use rrule::{RRuleSet, Tz};

use crate::model::{Event, Occurrence};

/// Everything in this app happens in San Diego.
pub const TZ: Tz = Tz::America__Los_Angeles;

/// Upper bound on occurrences per event in one expansion.
const MAX_OCCURRENCES: u16 = 1000;

#[derive(Debug, thiserror::Error)]
pub enum RecurrenceError {
    #[error("invalid recurrence rule: {0}")]
    Invalid(String),
    #[error("start time {0} does not exist in San Diego (daylight saving gap)")]
    NonexistentTime(NaiveDateTime),
}

/// The current wall-clock time in San Diego, regardless of the machine's time zone.
pub fn now_local() -> NaiveDateTime {
    chrono::Utc::now().with_timezone(&TZ).naive_local()
}

fn localize(dt: NaiveDateTime) -> Result<chrono::DateTime<Tz>, RecurrenceError> {
    TZ.from_local_datetime(&dt)
        .earliest()
        .ok_or(RecurrenceError::NonexistentTime(dt))
}

fn rule_set(start: NaiveDateTime, rule: &str) -> Result<RRuleSet, RecurrenceError> {
    let text = format!(
        "DTSTART;TZID=America/Los_Angeles:{}\nRRULE:{}",
        start.format("%Y%m%dT%H%M%S"),
        rule.trim().trim_start_matches("RRULE:")
    );
    text.parse()
        .map_err(|e: rrule::RRuleError| RecurrenceError::Invalid(e.to_string()))
}

/// Checks that `rule` parses and is valid when starting at `start`.
pub fn validate(start: NaiveDateTime, rule: &str) -> Result<(), RecurrenceError> {
    localize(start)?;
    rule_set(start, rule).map(|_| ())
}

/// Start times of `event` that overlap `[from, to)`, excluding cancelled dates.
pub fn event_starts(
    event: &Event,
    from: NaiveDateTime,
    to: NaiveDateTime,
) -> Result<Vec<NaiveDateTime>, RecurrenceError> {
    let starts = match &event.rrule {
        None => vec![event.start],
        Some(rule) => {
            // Back up by the duration so occurrences already in progress at `from` count.
            let after = localize(from - event.duration())?;
            let before = localize(to)?;
            rule_set(event.start, rule)?
                .after(after)
                .before(before)
                .all(MAX_OCCURRENCES)
                .dates
                .into_iter()
                .map(|d| d.naive_local())
                .collect()
        }
    };
    Ok(starts
        .into_iter()
        .filter(|s| *s < to && *s + event.duration() > from)
        .filter(|s| !event.cancelled.contains(&s.date()))
        .collect())
}

/// All occurrences of `events` in `[from, to)`, sorted by start time.
///
/// Events with invalid rules are skipped; [`validate`] catches those at edit time.
pub fn occurrences(events: &[Event], from: NaiveDateTime, to: NaiveDateTime) -> Vec<Occurrence> {
    let mut out: Vec<Occurrence> = events
        .iter()
        .enumerate()
        .flat_map(|(i, e)| {
            event_starts(e, from, to)
                .unwrap_or_default()
                .into_iter()
                .map(move |start| Occurrence {
                    event: i,
                    start,
                    end: start + e.duration(),
                })
        })
        .collect();
    out.sort_by_key(|o| o.start);
    out
}

/// The repeat options the editor offers. Anything else is [`Repeat::Custom`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Repeat {
    Once,
    Weekly,
    Biweekly,
    /// e.g. "second Tuesday of every month", derived from the start date.
    MonthlyNthWeekday,
    Custom,
}

impl Repeat {
    pub const ALL: [Repeat; 5] = [
        Repeat::Once,
        Repeat::Weekly,
        Repeat::Biweekly,
        Repeat::MonthlyNthWeekday,
        Repeat::Custom,
    ];

    /// The RRULE for this preset given the first occurrence. `Once` and `Custom` have none.
    pub fn to_rrule(self, start: NaiveDateTime) -> Option<String> {
        let day = weekday_code(start.weekday());
        match self {
            Repeat::Once | Repeat::Custom => None,
            Repeat::Weekly => Some(format!("FREQ=WEEKLY;BYDAY={day}")),
            Repeat::Biweekly => Some(format!("FREQ=WEEKLY;INTERVAL=2;BYDAY={day}")),
            Repeat::MonthlyNthWeekday => {
                Some(format!("FREQ=MONTHLY;BYDAY={}{day}", nth_weekday(start)))
            }
        }
    }

    /// Recognizes which preset produced `rule`, if any.
    pub fn detect(start: NaiveDateTime, rule: Option<&str>) -> Repeat {
        let Some(rule) = rule else {
            return Repeat::Once;
        };
        [Repeat::Weekly, Repeat::Biweekly, Repeat::MonthlyNthWeekday]
            .into_iter()
            .find(|r| r.to_rrule(start).as_deref() == Some(rule))
            .unwrap_or(Repeat::Custom)
    }
}

impl std::fmt::Display for Repeat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Repeat::Once => "Does not repeat",
            Repeat::Weekly => "Weekly",
            Repeat::Biweekly => "Every other week",
            Repeat::MonthlyNthWeekday => "Monthly (same weekday)",
            Repeat::Custom => "Custom RRULE",
        })
    }
}

/// Short human description of an event's schedule.
pub fn describe(event: &Event) -> String {
    let start = event.start;
    let day = start.format("%A").to_string();
    let time = start.format("%-I:%M %p");
    match Repeat::detect(start, event.rrule.as_deref()) {
        Repeat::Once => start.format("%a %b %-d, %Y at %-I:%M %p").to_string(),
        Repeat::Weekly => format!("Every {day} at {time}"),
        Repeat::Biweekly => format!("Every other {day} at {time}"),
        Repeat::MonthlyNthWeekday => {
            let nth = match nth_weekday(start) {
                1 => "1st",
                2 => "2nd",
                3 => "3rd",
                4 => "4th",
                _ => "5th",
            };
            format!("{nth} {day} of the month at {time}")
        }
        Repeat::Custom => format!(
            "Repeats ({}) at {time}",
            event.rrule.as_deref().unwrap_or("")
        ),
    }
}

fn nth_weekday(date: NaiveDateTime) -> u32 {
    (date.day() - 1) / 7 + 1
}

fn weekday_code(day: Weekday) -> &'static str {
    match day {
        Weekday::Mon => "MO",
        Weekday::Tue => "TU",
        Weekday::Wed => "WE",
        Weekday::Thu => "TH",
        Weekday::Fri => "FR",
        Weekday::Sat => "SA",
        Weekday::Sun => "SU",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Source;
    use chrono::NaiveDate;

    fn dt(y: i32, m: u32, d: u32, h: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(h, 0, 0)
            .unwrap()
    }

    #[test]
    fn weekly_occurrences_in_window() {
        // Tuesday 2026-10-06 7pm.
        let mut e = Event::new(Source::User, dt(2026, 10, 6, 19));
        e.rrule = Repeat::Weekly.to_rrule(e.start);
        let starts = event_starts(&e, dt(2026, 10, 20, 0), dt(2026, 11, 4, 0)).unwrap();
        assert_eq!(
            starts,
            vec![
                dt(2026, 10, 20, 19),
                dt(2026, 10, 27, 19),
                dt(2026, 11, 3, 19)
            ]
        );
    }

    #[test]
    fn local_time_is_stable_across_dst() {
        // DST ends 2026-11-01; the meetup should stay at 7pm local.
        let mut e = Event::new(Source::User, dt(2026, 10, 27, 19));
        e.rrule = Repeat::Weekly.to_rrule(e.start);
        let starts = event_starts(&e, dt(2026, 10, 27, 0), dt(2026, 11, 11, 0)).unwrap();
        assert!(starts.iter().all(|s| s.format("%H").to_string() == "19"));
        assert_eq!(starts.len(), 3);
    }

    #[test]
    fn cancelled_dates_are_skipped() {
        let mut e = Event::new(Source::User, dt(2026, 10, 6, 19));
        e.rrule = Repeat::Weekly.to_rrule(e.start);
        e.cancelled
            .push(NaiveDate::from_ymd_opt(2026, 10, 27).unwrap());
        let starts = event_starts(&e, dt(2026, 10, 20, 0), dt(2026, 11, 4, 0)).unwrap();
        assert_eq!(starts, vec![dt(2026, 10, 20, 19), dt(2026, 11, 3, 19)]);
    }

    #[test]
    fn monthly_nth_weekday() {
        // 2026-10-10 is the 2nd Saturday.
        let start = dt(2026, 10, 10, 9);
        assert_eq!(
            Repeat::MonthlyNthWeekday.to_rrule(start).as_deref(),
            Some("FREQ=MONTHLY;BYDAY=2SA")
        );
        let mut e = Event::new(Source::User, start);
        e.rrule = Repeat::MonthlyNthWeekday.to_rrule(start);
        let starts = event_starts(&e, dt(2026, 11, 1, 0), dt(2027, 1, 1, 0)).unwrap();
        assert_eq!(starts, vec![dt(2026, 11, 14, 9), dt(2026, 12, 12, 9)]);
    }

    #[test]
    fn multi_day_one_off_overlapping_window() {
        let mut e = Event::new(Source::Defaults, dt(2026, 7, 23, 9));
        e.duration_minutes = 4 * 24 * 60;
        assert_eq!(
            event_starts(&e, dt(2026, 7, 25, 0), dt(2026, 8, 1, 0))
                .unwrap()
                .len(),
            1
        );
        assert!(
            event_starts(&e, dt(2026, 7, 28, 0), dt(2026, 8, 1, 0))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn detects_presets() {
        let start = dt(2026, 10, 6, 19);
        for r in [Repeat::Weekly, Repeat::Biweekly, Repeat::MonthlyNthWeekday] {
            assert_eq!(Repeat::detect(start, r.to_rrule(start).as_deref()), r);
        }
        assert_eq!(Repeat::detect(start, None), Repeat::Once);
        assert_eq!(Repeat::detect(start, Some("FREQ=DAILY")), Repeat::Custom);
    }

    #[test]
    fn rejects_bad_rules() {
        assert!(validate(dt(2026, 10, 6, 19), "FREQ=SOMETIMES").is_err());
        assert!(validate(dt(2026, 10, 6, 19), "FREQ=DAILY;INTERVAL=3").is_ok());
    }
}
