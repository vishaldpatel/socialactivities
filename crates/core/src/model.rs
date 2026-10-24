use chrono::{DateTime, NaiveDate, NaiveDateTime, TimeDelta, Utc};

/// A named collection of groups and events. Every group and event belongs to
/// exactly one list.
#[derive(Debug, Clone, PartialEq)]
pub struct ListInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    /// When the list's contents last changed; decides whether an update is newer.
    pub updated_at: DateTime<Utc>,
    /// Lists you own can be edited and renamed. Lists added from elsewhere are
    /// read-only and change only through updates.
    pub owned: bool,
    /// The URL or file path the list was added from.
    pub source: Option<String>,
    /// Where the list's author publishes updates, if they said.
    pub update_url: Option<String>,
}

impl ListInfo {
    /// Where to look for a newer version: the author's update URL, else
    /// wherever the list was added from.
    pub fn update_location(&self) -> Option<&str> {
        self.update_url.as_deref().or(self.source.as_deref())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CostKind {
    #[default]
    Free,
    Fee,
    Membership,
}

impl CostKind {
    pub const ALL: [CostKind; 3] = [CostKind::Free, CostKind::Fee, CostKind::Membership];

    pub fn as_str(self) -> &'static str {
        match self {
            CostKind::Free => "free",
            CostKind::Fee => "fee",
            CostKind::Membership => "membership",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "fee" => CostKind::Fee,
            "membership" => CostKind::Membership,
            _ => CostKind::Free,
        }
    }
}

impl std::fmt::Display for CostKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            CostKind::Free => "Free",
            CostKind::Fee => "Fee",
            CostKind::Membership => "Membership",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Cost {
    pub kind: CostKind,
    /// Free text such as "$5 drop-in" or "$30/year".
    pub note: String,
}

impl std::fmt::Display for Cost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.note.is_empty() {
            write!(f, "{}", self.kind)
        } else {
            write!(f, "{} ({})", self.kind, self.note)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// One of [`crate::catalog::LINK_KINDS`].
    pub kind: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub id: String,
    pub list_id: String,
    pub name: String,
    pub description: String,
    pub neighborhood: String,
    pub address: String,
    /// Human description of how often the group meets, e.g. "Weekly on Saturdays".
    pub frequency: String,
    pub cost: Cost,
    /// Slugs from [`crate::catalog::CATEGORIES`].
    pub categories: Vec<String>,
    /// Free-form tags, normalized with [`crate::catalog::normalize_tag`].
    pub tags: Vec<String>,
    pub links: Vec<Link>,
    pub last_verified: Option<NaiveDate>,
}

impl Group {
    pub fn new(list_id: &str) -> Self {
        Self {
            id: crate::new_id(),
            list_id: list_id.into(),
            name: String::new(),
            description: String::new(),
            neighborhood: String::new(),
            address: String::new(),
            frequency: String::new(),
            cost: Cost::default(),
            categories: Vec::new(),
            tags: Vec::new(),
            links: Vec::new(),
            last_verified: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub id: String,
    pub list_id: String,
    /// One-off events such as fireworks or meteor showers may have no group.
    pub group_id: Option<String>,
    pub title: String,
    pub description: String,
    pub neighborhood: String,
    pub location: String,
    /// Local San Diego time of the first occurrence.
    pub start: NaiveDateTime,
    pub duration_minutes: u32,
    /// iCalendar RRULE body without the `RRULE:` prefix, e.g. `FREQ=WEEKLY;BYDAY=TU`.
    /// `None` means a one-off event.
    pub rrule: Option<String>,
    pub cost: Cost,
    pub url: String,
    /// Categories of this event, in addition to its group's.
    pub categories: Vec<String>,
    /// Free-form tags of this event, in addition to its group's.
    pub tags: Vec<String>,
    /// Dates of individual occurrences that won't happen.
    pub cancelled: Vec<NaiveDate>,
}

impl Event {
    pub fn new(list_id: &str, start: NaiveDateTime) -> Self {
        Self {
            id: crate::new_id(),
            list_id: list_id.into(),
            group_id: None,
            title: String::new(),
            description: String::new(),
            neighborhood: String::new(),
            location: String::new(),
            start,
            duration_minutes: 120,
            rrule: None,
            cost: Cost::default(),
            url: String::new(),
            categories: Vec::new(),
            tags: Vec::new(),
            cancelled: Vec::new(),
        }
    }

    pub fn is_one_off(&self) -> bool {
        self.rrule.is_none()
    }

    pub fn duration(&self) -> TimeDelta {
        TimeDelta::minutes(i64::from(self.duration_minutes))
    }
}

/// A single concrete occurrence of an [`Event`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Occurrence {
    /// Index into the event slice the occurrence was expanded from.
    pub event: usize,
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
}
