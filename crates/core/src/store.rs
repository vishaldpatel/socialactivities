//! Combines the read-only curated defaults with the user's own database.

use std::collections::HashSet;
use std::path::Path;

use rusqlite::Connection;

use crate::db::{self, DbError};
use crate::model::{Event, Group, Source};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error(transparent)]
    Db(#[from] DbError),
    #[error("curated defaults can only be changed in admin mode")]
    ReadOnly,
}

pub type Result<T> = std::result::Result<T, StoreError>;

pub struct Store {
    defaults: Connection,
    user: Connection,
    admin: bool,
    /// Set when the defaults couldn't be opened and an empty set is used instead.
    pub defaults_warning: Option<String>,
}

impl Store {
    /// Opens the user database at `user_path` and the defaults at `defaults_path`.
    ///
    /// In admin mode the defaults are opened writable (and created if missing) so
    /// the curated data can be edited. Otherwise they are read-only, and a missing
    /// or incompatible file falls back to an empty set with a warning.
    pub fn open(defaults_path: &Path, user_path: &Path, admin: bool) -> Result<Self> {
        let user = db::open_rw(user_path)?;
        let (defaults, defaults_warning) = if admin {
            (db::open_rw(defaults_path)?, None)
        } else if !defaults_path.exists() {
            let warning = format!("No curated defaults found at {}", defaults_path.display());
            (db::open_memory()?, Some(warning))
        } else {
            match db::open_ro(defaults_path) {
                Ok(conn) => (conn, None),
                Err(e) => {
                    let warning = format!("Couldn't load {}: {e}", defaults_path.display());
                    (db::open_memory()?, Some(warning))
                }
            }
        };
        Ok(Self {
            defaults,
            user,
            admin,
            defaults_warning,
        })
    }

    /// A store backed by two empty in-memory databases, for tests.
    pub fn in_memory(admin: bool) -> Result<Self> {
        Ok(Self {
            defaults: db::open_memory()?,
            user: db::open_memory()?,
            admin,
            defaults_warning: None,
        })
    }

    pub fn is_admin(&self) -> bool {
        self.admin
    }

    /// Where newly created records go.
    pub fn new_record_source(&self) -> Source {
        if self.admin {
            Source::Defaults
        } else {
            Source::User
        }
    }

    pub fn can_edit(&self, source: Source) -> bool {
        self.admin || source == Source::User
    }

    fn conn_mut(&mut self, source: Source) -> Result<&mut Connection> {
        match source {
            Source::User => Ok(&mut self.user),
            Source::Defaults if self.admin => Ok(&mut self.defaults),
            Source::Defaults => Err(StoreError::ReadOnly),
        }
    }

    /// All groups from both databases, sorted by name.
    pub fn groups(&self) -> Result<Vec<Group>> {
        let mut groups = db::load_groups(&self.defaults, Source::Defaults)?;
        groups.extend(db::load_groups(&self.user, Source::User)?);
        groups.sort_by_key(|g| g.name.to_lowercase());
        Ok(groups)
    }

    /// All events from both databases.
    pub fn events(&self) -> Result<Vec<Event>> {
        let mut events = db::load_events(&self.defaults, Source::Defaults)?;
        events.extend(db::load_events(&self.user, Source::User)?);
        Ok(events)
    }

    pub fn save_group(&mut self, group: &Group) -> Result<()> {
        Ok(db::save_group(self.conn_mut(group.source)?, group)?)
    }

    pub fn delete_group(&mut self, group: &Group) -> Result<()> {
        db::delete_group(self.conn_mut(group.source)?, &group.id)?;
        db::set_hidden(&self.user, &group.id, false)?;
        Ok(())
    }

    pub fn save_event(&mut self, event: &Event) -> Result<()> {
        Ok(db::save_event(self.conn_mut(event.source)?, event)?)
    }

    pub fn delete_event(&mut self, event: &Event) -> Result<()> {
        Ok(db::delete_event(self.conn_mut(event.source)?, &event.id)?)
    }

    /// Groups the user has chosen to hide. Always stored in the user database,
    /// so hiding works for curated groups and survives updates to the defaults.
    pub fn hidden_groups(&self) -> Result<HashSet<String>> {
        Ok(db::load_hidden(&self.user)?.into_iter().collect())
    }

    pub fn set_hidden(&self, group_id: &str, hidden: bool) -> Result<()> {
        Ok(db::set_hidden(&self.user, group_id, hidden)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn start() -> chrono::NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 10, 6)
            .unwrap()
            .and_hms_opt(19, 0, 0)
            .unwrap()
    }

    #[test]
    fn user_mode_cannot_edit_defaults() {
        let mut store = Store::in_memory(false).unwrap();
        let g = Group::new(Source::Defaults);
        assert!(matches!(store.save_group(&g), Err(StoreError::ReadOnly)));
        assert!(store.save_group(&Group::new(Source::User)).is_ok());
    }

    #[test]
    fn round_trips_groups_and_events() {
        let mut store = Store::in_memory(true).unwrap();
        let mut g = Group::new(Source::Defaults);
        g.name = "Trivia".into();
        g.categories = vec!["active".into(), "chill".into()];
        g.tags = vec!["beginner friendly".into(), "trivia".into()];
        g.links = vec![crate::model::Link {
            kind: "website".into(),
            url: "https://example.com".into(),
        }];
        g.last_verified = NaiveDate::from_ymd_opt(2026, 10, 1);
        store.save_group(&g).unwrap();

        let mut e = Event::new(Source::User, start());
        e.group_id = Some(g.id.clone());
        e.rrule = Some("FREQ=WEEKLY;BYDAY=TU".into());
        e.cancelled = vec![NaiveDate::from_ymd_opt(2026, 10, 13).unwrap()];
        e.categories = vec!["chill".into()];
        e.tags = vec!["outdoors".into()];
        store.save_event(&e).unwrap();

        let groups = store.groups().unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0], g);
        assert_eq!(store.events().unwrap(), vec![e]);
    }

    #[test]
    fn hiding_and_deleting() {
        let mut store = Store::in_memory(true).unwrap();
        let g = Group::new(Source::Defaults);
        store.save_group(&g).unwrap();
        let mut e = Event::new(Source::Defaults, start());
        e.group_id = Some(g.id.clone());
        store.save_event(&e).unwrap();

        store.set_hidden(&g.id, true).unwrap();
        assert!(store.hidden_groups().unwrap().contains(&g.id));

        store.delete_group(&g).unwrap();
        assert!(store.groups().unwrap().is_empty());
        assert!(store.events().unwrap().is_empty());
        assert!(store.hidden_groups().unwrap().is_empty());
    }
}
