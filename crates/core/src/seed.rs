//! Starter content for a fresh defaults database.
//!
//! The groups are clearly-labelled placeholders that exercise every feature;
//! replace them with real, verified groups through the admin interface.
//! Ids are fixed slugs so a user's hidden list keeps working across releases.

use chrono::{NaiveDate, NaiveDateTime};

use crate::model::{Cost, CostKind, Event, Group, Link, Source};
use crate::recurrence::Repeat;
use crate::store::{Result, Store};

fn at(y: i32, m: u32, d: u32, h: u32, min: u32) -> NaiveDateTime {
    NaiveDate::from_ymd_opt(y, m, d)
        .and_then(|d| d.and_hms_opt(h, min, 0))
        .expect("valid seed date")
}

struct Seed {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    neighborhood: &'static str,
    address: &'static str,
    tags: &'static [&'static str],
    cost: (CostKind, &'static str),
    event_title: &'static str,
    start: NaiveDateTime,
    minutes: u32,
    repeat: Repeat,
}

pub fn seed(store: &mut Store) -> Result<()> {
    let seeds = [
        Seed {
            id: "example-pb-volleyball",
            name: "Example: PB Sunset Volleyball",
            description: "Pickup beach volleyball for all levels. Bring water and sunscreen.",
            neighborhood: "pacific-beach",
            address: "Pacific Beach, near Crystal Pier",
            tags: &["fitness", "outdoors"],
            cost: (CostKind::Free, ""),
            event_title: "Saturday pickup games",
            start: at(2026, 10, 3, 16, 0),
            minutes: 150,
            repeat: Repeat::Weekly,
        },
        Seed {
            id: "example-np-board-games",
            name: "Example: North Park Board Game Night",
            description: "Open board game night. Plenty of games on hand, or bring your own.",
            neighborhood: "north-park",
            address: "North Park",
            tags: &["games"],
            cost: (CostKind::Fee, "$5 suggested donation"),
            event_title: "Tuesday game night",
            start: at(2026, 10, 6, 19, 0),
            minutes: 180,
            repeat: Repeat::Weekly,
        },
        Seed {
            id: "example-sunrise-hikers",
            name: "Example: Sunrise Hiking Crew",
            description: "Moderate morning hikes around the county, followed by coffee.",
            neighborhood: "countywide",
            address: "Trailhead varies; posted a week ahead",
            tags: &["outdoors", "fitness"],
            cost: (CostKind::Free, ""),
            event_title: "Monthly sunrise hike",
            start: at(2026, 10, 10, 6, 30),
            minutes: 180,
            repeat: Repeat::MonthlyNthWeekday,
        },
        Seed {
            id: "example-spanish-exchange",
            name: "Example: Hillcrest Spanish Exchange",
            description: "Conversation exchange: half the evening in Spanish, half in English.",
            neighborhood: "hillcrest",
            address: "Hillcrest",
            tags: &["language"],
            cost: (CostKind::Free, ""),
            event_title: "Conversation exchange",
            start: at(2026, 10, 8, 18, 30),
            minutes: 120,
            repeat: Repeat::Biweekly,
        },
        Seed {
            id: "example-ob-salsa",
            name: "Example: OB Salsa Social",
            description: "Beginner lesson for the first half hour, then open social dancing.",
            neighborhood: "ocean-beach",
            address: "Ocean Beach",
            tags: &["dance"],
            cost: (CostKind::Membership, "$40/year, first visit free"),
            event_title: "Friday salsa social",
            start: at(2026, 10, 9, 20, 0),
            minutes: 180,
            repeat: Repeat::Weekly,
        },
    ];

    for s in seeds {
        let mut event = Event::new(Source::Defaults, s.start);
        event.rrule = s.repeat.to_rrule(s.start);
        let frequency = crate::recurrence::describe(&event);

        let group = Group {
            id: s.id.into(),
            name: s.name.into(),
            description: s.description.into(),
            neighborhood: s.neighborhood.into(),
            address: s.address.into(),
            frequency,
            cost: Cost {
                kind: s.cost.0,
                note: s.cost.1.into(),
            },
            tags: s.tags.iter().map(|t| t.to_string()).collect(),
            links: vec![Link {
                kind: "website".into(),
                url: "https://example.com".into(),
            }],
            ..Group::new(Source::Defaults)
        };
        store.save_group(&group)?;

        event.id = format!("{}-event", s.id);
        event.group_id = Some(group.id.clone());
        event.title = s.event_title.into();
        event.neighborhood = group.neighborhood.clone();
        event.location = group.address.clone();
        event.duration_minutes = s.minutes;
        event.cost = group.cost.clone();
        store.save_event(&event)?;
    }

    // One-off events with no group.
    let one_offs = [
        Event {
            id: "big-bay-boom-2027".into(),
            title: "Big Bay Boom (4th of July fireworks)".into(),
            description: "Fireworks over San Diego Bay. Arrive early for a spot on the waterfront."
                .into(),
            neighborhood: "downtown".into(),
            location: "San Diego Bay waterfront".into(),
            duration_minutes: 30,
            url: "https://bigbayboom.com".into(),
            tags: vec!["festival".into()],
            ..Event::new(Source::Defaults, at(2027, 7, 4, 21, 0))
        },
        Event {
            id: "perseids-2027".into(),
            title: "Perseid meteor shower peak".into(),
            description: "Head east to Anza-Borrego or the Laguna Mountains for dark skies. \
                          Best after midnight."
                .into(),
            neighborhood: "countywide".into(),
            location: "Anywhere dark, away from city lights".into(),
            duration_minutes: 6 * 60,
            tags: vec!["sky".into(), "outdoors".into()],
            ..Event::new(Source::Defaults, at(2027, 8, 12, 22, 0))
        },
    ];
    for e in &one_offs {
        store.save_event(e)?;
    }
    Ok(())
}
