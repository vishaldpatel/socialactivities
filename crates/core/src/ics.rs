//! iCalendar (.ics) files for adding an event to a calendar app.
//!
//! A recurring event is written as the whole series, with cancelled dates
//! excluded, so calendars show it the way the app does.

use chrono::{DateTime, NaiveDateTime, Utc};

use crate::catalog::{COUNTYWIDE, neighborhood_name};
use crate::model::{CostKind, Event, Group};
use crate::recurrence::TZ_NAME;

/// San Diego's rules since 2007. Calendars that know the zone by name ignore
/// this, but some (Outlook) need it spelled out.
const VTIMEZONE: &str = "\
BEGIN:VTIMEZONE
TZID:America/Los_Angeles
BEGIN:DAYLIGHT
TZOFFSETFROM:-0800
TZOFFSETTO:-0700
TZNAME:PDT
DTSTART:19700308T020000
RRULE:FREQ=YEARLY;BYMONTH=3;BYDAY=2SU
END:DAYLIGHT
BEGIN:STANDARD
TZOFFSETFROM:-0700
TZOFFSETTO:-0800
TZNAME:PST
DTSTART:19701101T020000
RRULE:FREQ=YEARLY;BYMONTH=11;BYDAY=1SU
END:STANDARD
END:VTIMEZONE";

/// `event` as a calendar file. `now` is when it was made (`DTSTAMP`).
pub fn calendar(event: &Event, group: Option<&Group>, now: DateTime<Utc>) -> String {
    let local = |dt: NaiveDateTime| format!(";TZID={TZ_NAME}:{}", dt.format("%Y%m%dT%H%M%S"));

    let mut lines = vec![
        "BEGIN:VCALENDAR".to_string(),
        "VERSION:2.0".into(),
        "PRODID:-//San Diego Social Activities//EN".into(),
        "CALSCALE:GREGORIAN".into(),
    ];
    lines.extend(VTIMEZONE.lines().map(String::from));
    lines.push("BEGIN:VEVENT".into());
    lines.push(format!("UID:{}@sd-social-activities", event.id));
    lines.push(format!("DTSTAMP:{}", now.format("%Y%m%dT%H%M%SZ")));
    lines.push(format!("DTSTART{}", local(event.start)));
    lines.push(format!("DTEND{}", local(event.start + event.duration())));
    if let Some(rule) = &event.rrule {
        lines.push(format!("RRULE:{}", rule.trim().trim_start_matches("RRULE:")));
        for date in &event.cancelled {
            lines.push(format!("EXDATE{}", local(date.and_time(event.start.time()))));
        }
    }
    lines.push(format!("SUMMARY:{}", escape(&event.title)));
    let description = describe(event, group);
    if !description.is_empty() {
        lines.push(format!("DESCRIPTION:{}", escape(&description)));
    }
    let location = location(event);
    if !location.is_empty() {
        lines.push(format!("LOCATION:{}", escape(&location)));
    }
    if !event.url.is_empty() {
        lines.push(format!("URL:{}", event.url));
    }
    lines.push("END:VEVENT".into());
    lines.push("END:VCALENDAR".into());

    lines.iter().map(|l| fold(l) + "\r\n").collect()
}

/// The event's description, with the group, cost and link worth having on hand.
fn describe(event: &Event, group: Option<&Group>) -> String {
    let mut parts = Vec::new();
    if let Some(g) = group {
        parts.push(format!("Hosted by {}", g.name));
    }
    if !event.description.is_empty() {
        parts.push(event.description.clone());
    }
    if event.cost.kind != CostKind::Free {
        parts.push(format!("Cost: {}", event.cost));
    }
    if !event.url.is_empty() {
        parts.push(event.url.clone());
    }
    parts.join("\n\n")
}

/// The venue, then the neighborhood if the venue doesn't already say it.
/// "Countywide / Various" isn't a place a calendar can use, so it's left off.
fn location(event: &Event) -> String {
    let area = match event.neighborhood.as_str() {
        COUNTYWIDE => "",
        slug => neighborhood_name(slug),
    };
    match (event.location.trim(), area) {
        (venue, area) if venue.is_empty() || venue == area => area.to_string(),
        (venue, "") => venue.to_string(),
        (venue, area) if venue.contains(area) => venue.to_string(),
        (venue, area) => format!("{venue}, {area}"),
    }
}

/// Escapes text values (RFC 5545 §3.3.11).
fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' | ';' | ',' => {
                out.push('\\');
                out.push(c);
            }
            '\n' => out.push_str("\\n"),
            '\r' => {}
            _ => out.push(c),
        }
    }
    out
}

/// Splits a line into 75-byte pieces, continued with a leading space, without
/// breaking a character (RFC 5545 §3.1).
fn fold(line: &str) -> String {
    let mut out = String::with_capacity(line.len() + line.len() / 70 * 3);
    let mut width = 0;
    for c in line.chars() {
        if width + c.len_utf8() > 75 {
            out.push_str("\r\n ");
            width = 1;
        }
        out.push(c);
        width += c.len_utf8();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveDate, TimeZone};

    fn dt(y: i32, m: u32, d: u32, h: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(h, 0, 0)
            .unwrap()
    }

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 25, 17, 30, 0).unwrap()
    }

    #[test]
    fn writes_a_weekly_series_in_san_diego_time() {
        let mut e = Event::new("list", dt(2026, 10, 6, 19));
        e.id = "abc".into();
        e.title = "Board games, snacks; fun".into();
        e.rrule = Some("FREQ=WEEKLY;BYDAY=TU".into());
        e.cancelled.push(NaiveDate::from_ymd_opt(2026, 10, 27).unwrap());
        e.neighborhood = "encinitas".into();
        e.location = "Culture Brewing".into();
        let ics = calendar(&e, None, now());

        assert!(ics.ends_with("END:VCALENDAR\r\n"));
        assert!(ics.lines().all(|l| l.len() <= 76), "lines are folded");
        for line in [
            "UID:abc@sd-social-activities",
            "DTSTAMP:20261025T173000Z",
            "DTSTART;TZID=America/Los_Angeles:20261006T190000",
            "DTEND;TZID=America/Los_Angeles:20261006T210000",
            "RRULE:FREQ=WEEKLY;BYDAY=TU",
            "EXDATE;TZID=America/Los_Angeles:20261027T190000",
            "SUMMARY:Board games\\, snacks\\; fun",
            "LOCATION:Culture Brewing\\, Encinitas",
            "TZID:America/Los_Angeles",
        ] {
            assert!(ics.contains(&format!("{line}\r\n")), "missing {line}");
        }
    }

    #[test]
    fn one_off_events_have_no_rule_and_free_isnt_mentioned() {
        let mut e = Event::new("list", dt(2026, 7, 4, 21));
        e.description = "Bring a blanket.\nParking fills up.".into();
        e.cancelled.push(NaiveDate::from_ymd_opt(2026, 7, 4).unwrap());
        let ics = calendar(&e, None, now());
        let vevent = &ics[ics.find("BEGIN:VEVENT").unwrap()..];
        assert!(!vevent.contains("RRULE"));
        assert!(!vevent.contains("EXDATE"));
        assert!(!vevent.contains("Cost"));
        assert!(ics.contains("DESCRIPTION:Bring a blanket.\\nParking fills up.\r\n"));
    }

    #[test]
    fn describes_the_group_and_cost() {
        let mut g = Group::new("list");
        g.name = "Run Club".into();
        let mut e = Event::new("list", dt(2026, 10, 6, 19));
        e.cost.kind = CostKind::Fee;
        e.cost.note = "$5".into();
        let text = describe(&e, Some(&g));
        assert_eq!(text, "Hosted by Run Club\n\nCost: Fee ($5)");
    }

    #[test]
    fn countywide_isnt_a_location() {
        let mut e = Event::new("list", dt(2026, 8, 12, 22));
        e.neighborhood = COUNTYWIDE.into();
        e.location = "Anywhere dark".into();
        assert_eq!(location(&e), "Anywhere dark");
        e.location.clear();
        assert_eq!(location(&e), "");
    }

    #[test]
    fn folds_without_splitting_characters() {
        let line = format!("SUMMARY:{}", "é".repeat(60));
        let folded = fold(&line);
        let pieces: Vec<&str> = folded.split("\r\n").collect();
        assert!(pieces.len() > 1);
        assert!(pieces.iter().all(|p| p.len() <= 75));
        assert_eq!(folded.replace("\r\n ", ""), line);
    }
}
