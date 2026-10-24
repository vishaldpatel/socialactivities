//! The JSON file format for sharing a list of groups and events.
//!
//! Records are written with their own types rather than the model's, so the
//! format stays stable as the model changes. Missing optional fields default,
//! so files from older versions keep loading.

use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::model::{Cost, CostKind, Event, Group, Link, ListInfo};

/// The newest format this version of the app reads and writes.
pub const FORMAT: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum ListFileError {
    #[error("not a valid list file: {0}")]
    Json(#[from] serde_json::Error),
    #[error("this list needs a newer version of the app (format {0})")]
    TooNew(u32),
    #[error("the list file is missing its {0}")]
    Missing(&'static str),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListFile {
    pub format: u32,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub updated_at: DateTime<Utc>,
    /// Where to fetch newer versions of this list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub update_url: Option<String>,
    #[serde(default)]
    pub groups: Vec<GroupRecord>,
    #[serde(default)]
    pub events: Vec<EventRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CostRecord {
    #[serde(default = "free")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}

fn free() -> String {
    CostKind::Free.as_str().into()
}

impl Default for CostRecord {
    fn default() -> Self {
        Self {
            kind: free(),
            note: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LinkRecord {
    pub kind: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroupRecord {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub neighborhood: String,
    #[serde(default)]
    pub address: String,
    #[serde(default)]
    pub frequency: String,
    #[serde(default)]
    pub cost: CostRecord,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub links: Vec<LinkRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_verified: Option<NaiveDate>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventRecord {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub neighborhood: String,
    #[serde(default)]
    pub location: String,
    /// San Diego local time of the first occurrence.
    pub start: NaiveDateTime,
    pub duration_minutes: u32,
    /// iCalendar RRULE body; absent for one-off events.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rrule: Option<String>,
    #[serde(default)]
    pub cost: CostRecord,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cancelled: Vec<NaiveDate>,
}

impl From<&Cost> for CostRecord {
    fn from(c: &Cost) -> Self {
        Self {
            kind: c.kind.as_str().into(),
            note: c.note.clone(),
        }
    }
}

impl From<CostRecord> for Cost {
    fn from(c: CostRecord) -> Self {
        Self {
            kind: CostKind::parse(&c.kind),
            note: c.note,
        }
    }
}

impl ListFile {
    pub fn parse(json: &str) -> Result<Self, ListFileError> {
        let file: ListFile = serde_json::from_str(json)?;
        if file.format > FORMAT {
            return Err(ListFileError::TooNew(file.format));
        }
        if file.id.trim().is_empty() {
            return Err(ListFileError::Missing("id"));
        }
        if file.name.trim().is_empty() {
            return Err(ListFileError::Missing("name"));
        }
        Ok(file)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("list files always serialize")
    }

    /// The file for `list`, with the given groups and events (which should be
    /// the ones belonging to it).
    pub fn new(list: &ListInfo, groups: &[Group], events: &[Event]) -> Self {
        Self {
            format: FORMAT,
            id: list.id.clone(),
            name: list.name.clone(),
            description: list.description.clone(),
            updated_at: list.updated_at,
            update_url: list.update_url.clone(),
            groups: groups.iter().map(GroupRecord::from).collect(),
            events: events.iter().map(EventRecord::from).collect(),
        }
    }

    /// The list's details as installed from this file.
    pub fn info(&self, owned: bool, source: Option<String>) -> ListInfo {
        ListInfo {
            id: self.id.clone(),
            name: self.name.clone(),
            description: self.description.clone(),
            updated_at: self.updated_at,
            owned,
            source,
            update_url: self.update_url.clone(),
        }
    }

    pub fn groups(&self) -> Vec<Group> {
        self.groups.iter().map(|g| g.to_group(&self.id)).collect()
    }

    pub fn events(&self) -> Vec<Event> {
        self.events.iter().map(|e| e.to_event(&self.id)).collect()
    }
}

impl From<&Group> for GroupRecord {
    fn from(g: &Group) -> Self {
        Self {
            id: g.id.clone(),
            name: g.name.clone(),
            description: g.description.clone(),
            neighborhood: g.neighborhood.clone(),
            address: g.address.clone(),
            frequency: g.frequency.clone(),
            cost: (&g.cost).into(),
            categories: g.categories.clone(),
            tags: g.tags.clone(),
            links: g
                .links
                .iter()
                .map(|l| LinkRecord {
                    kind: l.kind.clone(),
                    url: l.url.clone(),
                })
                .collect(),
            last_verified: g.last_verified,
        }
    }
}

impl GroupRecord {
    fn to_group(&self, list_id: &str) -> Group {
        Group {
            id: self.id.clone(),
            list_id: list_id.into(),
            name: self.name.clone(),
            description: self.description.clone(),
            logo_path: None,
            neighborhood: self.neighborhood.clone(),
            address: self.address.clone(),
            frequency: self.frequency.clone(),
            cost: self.cost.clone().into(),
            categories: self.categories.clone(),
            tags: self.tags.clone(),
            links: self
                .links
                .iter()
                .map(|l| Link {
                    kind: l.kind.clone(),
                    url: l.url.clone(),
                })
                .collect(),
            last_verified: self.last_verified,
        }
    }
}

impl From<&Event> for EventRecord {
    fn from(e: &Event) -> Self {
        Self {
            id: e.id.clone(),
            group_id: e.group_id.clone(),
            title: e.title.clone(),
            description: e.description.clone(),
            neighborhood: e.neighborhood.clone(),
            location: e.location.clone(),
            start: e.start,
            duration_minutes: e.duration_minutes,
            rrule: e.rrule.clone(),
            cost: (&e.cost).into(),
            url: e.url.clone(),
            categories: e.categories.clone(),
            tags: e.tags.clone(),
            cancelled: e.cancelled.clone(),
        }
    }
}

impl EventRecord {
    fn to_event(&self, list_id: &str) -> Event {
        Event {
            id: self.id.clone(),
            list_id: list_id.into(),
            group_id: self.group_id.clone(),
            title: self.title.clone(),
            description: self.description.clone(),
            neighborhood: self.neighborhood.clone(),
            location: self.location.clone(),
            start: self.start,
            duration_minutes: self.duration_minutes,
            rrule: self.rrule.clone(),
            cost: self.cost.clone().into(),
            url: self.url.clone(),
            categories: self.categories.clone(),
            tags: self.tags.clone(),
            cancelled: self.cancelled.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> (ListInfo, Vec<Group>, Vec<Event>) {
        let list = ListInfo {
            id: "list-1".into(),
            name: "Beach crew".into(),
            description: "Things by the water".into(),
            updated_at: "2026-10-24T18:00:00Z".parse().unwrap(),
            owned: true,
            source: None,
            update_url: Some("https://example.com/beach.json".into()),
        };
        let mut g = Group::new("list-1");
        g.name = "Volleyball".into();
        g.neighborhood = "pacific-beach".into();
        g.categories = vec!["active".into()];
        g.tags = vec!["beach".into()];
        g.links = vec![Link {
            kind: "website".into(),
            url: "https://example.com".into(),
        }];
        g.cost = Cost {
            kind: CostKind::Fee,
            note: "$5".into(),
        };
        let start = NaiveDate::from_ymd_opt(2026, 10, 3)
            .unwrap()
            .and_hms_opt(16, 0, 0)
            .unwrap();
        let mut e = Event::new("list-1", start);
        e.title = "Pickup games".into();
        e.group_id = Some(g.id.clone());
        e.neighborhood = "pacific-beach".into();
        e.rrule = Some("FREQ=WEEKLY;BYDAY=SA".into());
        e.cancelled = vec![NaiveDate::from_ymd_opt(2026, 10, 31).unwrap()];
        (list, vec![g], vec![e])
    }

    #[test]
    fn round_trips_through_json() {
        let (list, groups, events) = sample();
        let json = ListFile::new(&list, &groups, &events).to_json();
        let file = ListFile::parse(&json).unwrap();
        assert_eq!(file.info(true, None), list);
        assert_eq!(file.groups(), groups);
        assert_eq!(file.events(), events);
    }

    #[test]
    fn logos_stay_local() {
        let (list, mut groups, events) = sample();
        groups[0].logo_path = Some("/home/me/logo.png".into());
        let json = ListFile::new(&list, &groups, &events).to_json();
        assert!(!json.contains("logo"));
    }

    #[test]
    fn minimal_files_load_with_defaults() {
        let json = r#"{
            "format": 1, "id": "x", "name": "X", "updated_at": "2026-01-01T00:00:00Z",
            "groups": [{"id": "g", "name": "G", "neighborhood": "hillcrest"}],
            "events": [{"id": "e", "title": "E", "neighborhood": "hillcrest",
                        "start": "2026-01-02T19:00:00", "duration_minutes": 60}]
        }"#;
        let file = ListFile::parse(json).unwrap();
        assert_eq!(file.groups()[0].cost, Cost::default());
        assert_eq!(file.events()[0].rrule, None);
    }

    #[test]
    fn rejects_bad_files() {
        assert!(matches!(
            ListFile::parse("nope"),
            Err(ListFileError::Json(_))
        ));
        let too_new =
            r#"{"format": 99, "id": "x", "name": "X", "updated_at": "2026-01-01T00:00:00Z"}"#;
        assert!(matches!(
            ListFile::parse(too_new),
            Err(ListFileError::TooNew(99))
        ));
        let unnamed =
            r#"{"format": 1, "id": "x", "name": " ", "updated_at": "2026-01-01T00:00:00Z"}"#;
        assert!(matches!(
            ListFile::parse(unnamed),
            Err(ListFileError::Missing("name"))
        ));
    }
}
