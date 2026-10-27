//! The app's data: lists of groups and events, kept in one SQLite database.
//!
//! Lists you own can be edited. Lists added from a file or URL are read-only
//! and change only by being replaced with a newer version of the same list.

use std::collections::HashSet;
use std::path::Path;

use chrono::Utc;
use rusqlite::Connection;

use crate::db::{self, DbError, Table};
use crate::listfile::{ListFile, ListFileError};
use crate::model::{Event, Group, ListInfo};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error(transparent)]
    Db(#[from] DbError),
    #[error(transparent)]
    File(#[from] ListFileError),
    #[error("“{0}” was added from elsewhere and can't be edited")]
    ReadOnly(String),
    #[error("that list no longer exists")]
    NoSuchList,
    #[error("“{0}” is a list you maintain, so it can't be replaced by an update")]
    Owned(String),
    #[error("this list shares items with “{0}”, so it can't be added alongside it")]
    Conflict(String),
}

pub type Result<T> = std::result::Result<T, StoreError>;

const LAST_LIST: &str = "last_list";

/// What happened when a list file was installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Installed {
    Added,
    Replaced,
    /// The installed copy is as new or newer.
    Unchanged,
}

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        Ok(Self {
            conn: db::open_rw(path)?,
        })
    }

    /// A store backed by an empty in-memory database, for tests.
    pub fn in_memory() -> Result<Self> {
        Ok(Self {
            conn: db::open_memory()?,
        })
    }

    pub fn lists(&self) -> Result<Vec<ListInfo>> {
        Ok(db::load_lists(&self.conn)?)
    }

    fn list(&self, id: &str) -> Result<ListInfo> {
        self.lists()?
            .into_iter()
            .find(|l| l.id == id)
            .ok_or(StoreError::NoSuchList)
    }

    /// The list `id`, if it exists and is yours to edit.
    fn owned_list(&self, id: &str) -> Result<ListInfo> {
        let list = self.list(id)?;
        if list.owned {
            Ok(list)
        } else {
            Err(StoreError::ReadOnly(list.name))
        }
    }

    /// All groups, sorted by name.
    pub fn groups(&self) -> Result<Vec<Group>> {
        Ok(db::load_groups(&self.conn)?)
    }

    pub fn events(&self) -> Result<Vec<Event>> {
        Ok(db::load_events(&self.conn)?)
    }

    /// Records that a list you own changed, so exports carry a newer timestamp.
    fn touch(&self, list_id: &str) -> Result<()> {
        Ok(db::touch_list(&self.conn, list_id, Utc::now())?)
    }

    /// Checks that a record can be saved into `list_id` (and moved out of its
    /// current list, if different). Returns the list it's moving from.
    fn check_move(&self, table: Table, id: &str, list_id: &str) -> Result<Option<String>> {
        self.owned_list(list_id)?;
        let previous = db::list_of(&self.conn, table, id)?.filter(|p| p != list_id);
        if let Some(previous) = &previous {
            self.owned_list(previous)?;
        }
        Ok(previous)
    }

    pub fn save_group(&mut self, group: &Group) -> Result<()> {
        let moved_from = self.check_move(Table::Groups, &group.id, &group.list_id)?;
        db::save_group(&mut self.conn, group)?;
        self.touch(&group.list_id)?;
        self.remember_list(&group.list_id)?;
        if let Some(from) = moved_from {
            self.touch(&from)?;
        }
        Ok(())
    }

    pub fn delete_group(&mut self, group: &Group) -> Result<()> {
        self.owned_list(&group.list_id)?;
        db::delete_group(&mut self.conn, &group.id, &group.list_id)?;
        db::set_hidden(&self.conn, &group.id, false)?;
        self.touch(&group.list_id)
    }

    pub fn save_event(&mut self, event: &Event) -> Result<()> {
        let moved_from = self.check_move(Table::Events, &event.id, &event.list_id)?;
        db::save_event(&mut self.conn, event)?;
        self.touch(&event.list_id)?;
        self.remember_list(&event.list_id)?;
        if let Some(from) = moved_from {
            self.touch(&from)?;
        }
        Ok(())
    }

    pub fn delete_event(&mut self, event: &Event) -> Result<()> {
        self.owned_list(&event.list_id)?;
        db::delete_event(&self.conn, &event.id)?;
        self.touch(&event.list_id)
    }

    /// Creates a new, empty list you own.
    pub fn create_list(&mut self, name: &str) -> Result<ListInfo> {
        let list = ListInfo {
            id: crate::new_id(),
            name: name.trim().into(),
            description: String::new(),
            updated_at: Utc::now(),
            owned: true,
            source: None,
            update_url: None,
        };
        db::save_list(&mut self.conn, &list)?;
        Ok(list)
    }

    /// The first list you own, creating "My List" if there are none.
    pub fn default_list(&mut self) -> Result<ListInfo> {
        match self.lists()?.into_iter().find(|l| l.owned) {
            Some(list) => Ok(list),
            None => self.create_list("My List"),
        }
    }

    /// Changes a list's details. They're part of what gets exported, so this
    /// counts as a change to the list.
    pub fn edit_list(
        &mut self,
        id: &str,
        name: &str,
        description: &str,
        update_url: Option<&str>,
    ) -> Result<()> {
        let mut list = self.owned_list(id)?;
        list.name = name.trim().into();
        list.description = description.trim().into();
        list.update_url = update_url
            .map(str::trim)
            .filter(|u| !u.is_empty())
            .map(Into::into);
        list.updated_at = Utc::now();
        Ok(db::save_list(&mut self.conn, &list)?)
    }

    /// The list you last saved a group or event to, if it's still yours.
    pub fn last_list(&self) -> Result<Option<String>> {
        let Some(id) = db::setting(&self.conn, LAST_LIST)? else {
            return Ok(None);
        };
        Ok(self.owned_list(&id).ok().map(|l| l.id))
    }

    fn remember_list(&self, id: &str) -> Result<()> {
        Ok(db::set_setting(&self.conn, LAST_LIST, id)?)
    }

    /// Removes a list and everything in it.
    pub fn remove_list(&mut self, id: &str) -> Result<()> {
        Ok(db::delete_list(&mut self.conn, id)?)
    }

    /// The list as a shareable file.
    pub fn export(&self, id: &str) -> Result<ListFile> {
        let list = self.list(id)?;
        let groups: Vec<Group> = self
            .groups()?
            .into_iter()
            .filter(|g| g.list_id == id)
            .collect();
        let events: Vec<Event> = self
            .events()?
            .into_iter()
            .filter(|e| e.list_id == id)
            .collect();
        Ok(ListFile::new(&list, &groups, &events))
    }

    /// Fails if any of the file's groups or events already belong to a
    /// different list.
    fn check_conflicts(&self, file: &ListFile) -> Result<()> {
        let ids: HashSet<&str> = (file.groups.iter().map(|g| g.id.as_str()))
            .chain(file.events.iter().map(|e| e.id.as_str()))
            .collect();
        let elsewhere = (self.groups()?.into_iter().map(|g| (g.id, g.list_id)))
            .chain(self.events()?.into_iter().map(|e| (e.id, e.list_id)))
            .find(|(id, list_id)| *list_id != file.id && ids.contains(id.as_str()));
        match elsewhere {
            Some((_, list_id)) => Err(StoreError::Conflict(self.list(&list_id)?.name)),
            None => Ok(()),
        }
    }

    /// Adds a list from a file, replacing any installed copy of the same list
    /// unless it's one you maintain. `owned` makes it editable here, for the
    /// person who maintains it.
    pub fn add_list(
        &mut self,
        file: &ListFile,
        source: Option<String>,
        owned: bool,
    ) -> Result<Installed> {
        self.check_conflicts(file)?;
        let existing = match self.list(&file.id) {
            Ok(list) => Some(list),
            Err(StoreError::NoSuchList) => None,
            Err(e) => return Err(e),
        };
        if let Some(list) = existing.as_ref().filter(|l| l.owned) {
            return Err(StoreError::Owned(list.name.clone()));
        }
        let existed = existing.is_some();
        db::replace_list(
            &mut self.conn,
            &file.info(owned, source),
            &file.groups(),
            &file.events(),
        )?;
        Ok(if existed {
            Installed::Replaced
        } else {
            Installed::Added
        })
    }

    /// Whether `file` is a newer version of an installed list you don't own.
    pub fn is_update(&self, file: &ListFile) -> Result<bool> {
        let list = self.list(&file.id)?;
        Ok(!list.owned && file.updated_at > list.updated_at)
    }

    /// Replaces an added list with a newer version, keeping where it came from.
    /// If the new version doesn't say where its updates are published, the
    /// update URL already known is kept, so updates keep arriving.
    pub fn update_list(&mut self, file: &ListFile) -> Result<Installed> {
        let list = self.list(&file.id)?;
        if list.owned {
            return Err(StoreError::Owned(list.name));
        }
        if file.updated_at <= list.updated_at {
            return Ok(Installed::Unchanged);
        }
        self.check_conflicts(file)?;
        let mut info = file.info(false, list.source);
        info.update_url = info.update_url.or(list.update_url);
        db::replace_list(&mut self.conn, &info, &file.groups(), &file.events())?;
        Ok(Installed::Replaced)
    }

    /// Installs the list that ships with the app: added if missing (unless you
    /// removed it), updated if the shipped copy is newer, and left alone if you
    /// maintain it.
    pub fn install_bundled(&mut self, file: &ListFile) -> Result<Installed> {
        match self.list(&file.id) {
            Err(StoreError::NoSuchList) if db::was_removed(&self.conn, &file.id)? => {
                Ok(Installed::Unchanged)
            }
            Err(StoreError::NoSuchList) => self.add_list(file, None, false),
            Ok(list) if list.owned => Ok(Installed::Unchanged),
            Ok(_) => self.update_list(file),
            Err(e) => Err(e),
        }
    }

    /// Groups the user has chosen to hide, by id, across all lists.
    pub fn hidden_groups(&self) -> Result<HashSet<String>> {
        Ok(db::load_hidden(&self.conn)?.into_iter().collect())
    }

    pub fn set_hidden(&self, group_id: &str, hidden: bool) -> Result<()> {
        Ok(db::set_hidden(&self.conn, group_id, hidden)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, NaiveDate, NaiveDateTime};

    fn start() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 10, 6)
            .unwrap()
            .and_hms_opt(19, 0, 0)
            .unwrap()
    }

    fn at(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    /// A store with one list you own holding a group and its event.
    fn store_with_list() -> (Store, ListInfo, Group, Event) {
        let mut store = Store::in_memory().unwrap();
        let list = store.default_list().unwrap();
        let mut g = Group::new(&list.id);
        g.name = "Trivia".into();
        g.categories = vec!["active".into(), "chill".into()];
        g.tags = vec!["beginner friendly".into(), "trivia".into()];
        g.last_verified = NaiveDate::from_ymd_opt(2026, 10, 1);
        store.save_group(&g).unwrap();
        let mut e = Event::new(&list.id, start());
        e.group_id = Some(g.id.clone());
        e.rrule = Some("FREQ=WEEKLY;BYDAY=TU".into());
        e.cancelled = vec![NaiveDate::from_ymd_opt(2026, 10, 13).unwrap()];
        e.categories = vec!["chill".into()];
        e.tags = vec!["outdoors".into()];
        store.save_event(&e).unwrap();
        (store, list, g, e)
    }

    /// `list` as published by its maintainer at `updated_at`.
    fn published(store: &Store, list: &ListInfo, updated_at: &str) -> ListFile {
        let mut file = store.export(&list.id).unwrap();
        file.updated_at = at(updated_at);
        file
    }

    #[test]
    fn fresh_store_has_my_list() {
        let mut store = Store::in_memory().unwrap();
        let lists = store.lists().unwrap();
        assert_eq!(lists.len(), 1);
        assert_eq!(lists[0].name, "My List");
        assert_eq!(store.default_list().unwrap(), lists[0]);
    }

    #[test]
    fn round_trips_groups_and_events() {
        let (store, _, g, e) = store_with_list();
        assert_eq!(store.groups().unwrap(), vec![g]);
        assert_eq!(store.events().unwrap(), vec![e]);
    }

    #[test]
    fn editing_touches_the_list() {
        let (mut store, list, mut g, _) = store_with_list();
        db::touch_list(&store.conn, &list.id, at("2020-01-01T00:00:00Z")).unwrap();
        g.name = "Pub trivia".into();
        store.save_group(&g).unwrap();
        assert!(store.list(&list.id).unwrap().updated_at > at("2020-01-01T00:00:00Z"));
    }

    #[test]
    fn added_lists_are_read_only_and_update_only_when_newer() {
        let (mut source, list, mut g, _) = store_with_list();
        let v1 = published(&source, &list, "2026-10-01T00:00:00Z");

        let mut store = Store::in_memory().unwrap();
        let url = Some("https://example.com/list.json".to_string());
        assert_eq!(
            store.add_list(&v1, url.clone(), false).unwrap(),
            Installed::Added
        );
        let mut copy = store.groups().unwrap()[0].clone();
        copy.name = "Mine now".into();
        assert!(matches!(
            store.save_group(&copy),
            Err(StoreError::ReadOnly(_))
        ));
        assert!(matches!(
            store.edit_list(&list.id, "x", "", None),
            Err(StoreError::ReadOnly(_))
        ));

        assert!(!store.is_update(&v1).unwrap());
        assert_eq!(store.update_list(&v1).unwrap(), Installed::Unchanged);

        g.name = "Trivia night".into();
        source.save_group(&g).unwrap();
        let v2 = published(&source, &list, "2026-10-02T00:00:00Z");
        assert!(store.is_update(&v2).unwrap());
        assert_eq!(store.update_list(&v2).unwrap(), Installed::Replaced);
        assert_eq!(store.groups().unwrap()[0].name, "Trivia night");
        let installed = store.list(&list.id).unwrap();
        assert_eq!(
            installed.source, url,
            "updates keep where the list came from"
        );
        assert!(!installed.owned);
    }

    #[test]
    fn your_own_events_in_an_added_list_survive_its_updates() {
        let (source, list, g, _) = store_with_list();
        let mut store = Store::in_memory().unwrap();
        store
            .add_list(
                &published(&source, &list, "2026-10-01T00:00:00Z"),
                None,
                false,
            )
            .unwrap();
        let mine = store
            .lists()
            .unwrap()
            .into_iter()
            .find(|l| l.owned)
            .unwrap();
        let mut afterparty = Event::new(&mine.id, start());
        afterparty.group_id = Some(g.id.clone());
        store.save_event(&afterparty).unwrap();

        store
            .update_list(&published(&source, &list, "2026-10-02T00:00:00Z"))
            .unwrap();
        assert!(store.events().unwrap().contains(&afterparty));
    }

    #[test]
    fn maintained_lists_are_editable_and_never_overwritten() {
        let (source, list, _, _) = store_with_list();
        let file = published(&source, &list, "2026-10-01T00:00:00Z");
        let mut store = Store::in_memory().unwrap();
        store.add_list(&file, None, true).unwrap();
        assert!(store.edit_list(&list.id, "Renamed", "", None).is_ok());

        let newer = published(&source, &list, "2030-01-01T00:00:00Z");
        assert_eq!(store.install_bundled(&newer).unwrap(), Installed::Unchanged);
        assert!(matches!(
            store.update_list(&newer),
            Err(StoreError::Owned(_))
        ));
        assert_eq!(store.list(&list.id).unwrap().name, "Renamed");
    }

    #[test]
    fn bundled_list_installs_then_updates() {
        let (source, list, _, _) = store_with_list();
        let mut store = Store::in_memory().unwrap();
        let v1 = published(&source, &list, "2026-10-01T00:00:00Z");
        assert_eq!(store.install_bundled(&v1).unwrap(), Installed::Added);
        assert_eq!(store.install_bundled(&v1).unwrap(), Installed::Unchanged);
        let v2 = published(&source, &list, "2026-10-02T00:00:00Z");
        assert_eq!(store.install_bundled(&v2).unwrap(), Installed::Replaced);
    }

    #[test]
    fn updates_keep_the_update_url_when_the_new_version_has_none() {
        let (source, list, _, _) = store_with_list();
        let mut store = Store::in_memory().unwrap();
        let mut v1 = published(&source, &list, "2026-10-01T00:00:00Z");
        v1.update_url = Some("https://x.org/l.json".into());
        store.install_bundled(&v1).unwrap();
        let v2 = published(&source, &list, "2026-10-02T00:00:00Z");
        assert_eq!(v2.update_url, None);
        assert_eq!(store.update_list(&v2).unwrap(), Installed::Replaced);
        let installed = store.list(&list.id).unwrap();
        assert_eq!(installed.update_location(), Some("https://x.org/l.json"));

        let mut v3 = published(&source, &list, "2026-10-03T00:00:00Z");
        v3.update_url = Some("https://y.org/l.json".into());
        store.update_list(&v3).unwrap();
        let moved = store.list(&list.id).unwrap();
        assert_eq!(moved.update_url.as_deref(), Some("https://y.org/l.json"));
    }

    #[test]
    fn removed_bundled_list_stays_removed_until_added_again() {
        let (source, list, _, _) = store_with_list();
        let file = published(&source, &list, "2026-10-01T00:00:00Z");
        let mut store = Store::in_memory().unwrap();
        store.install_bundled(&file).unwrap();
        store.remove_list(&list.id).unwrap();
        assert_eq!(store.install_bundled(&file).unwrap(), Installed::Unchanged);
        assert!(store.list(&list.id).is_err());

        store.add_list(&file, None, false).unwrap();
        store.remove_list(&list.id).unwrap();
        store.add_list(&file, None, false).unwrap();
        let newer = published(&source, &list, "2026-10-02T00:00:00Z");
        assert_eq!(store.install_bundled(&newer).unwrap(), Installed::Replaced);
    }

    #[test]
    fn adding_never_overwrites_a_list_you_maintain() {
        let (source, list, _, _) = store_with_list();
        let file = published(&source, &list, "2026-10-01T00:00:00Z");
        let mut store = Store::in_memory().unwrap();
        store.add_list(&file, None, true).unwrap();
        assert!(matches!(
            store.add_list(&file, None, false),
            Err(StoreError::Owned(_))
        ));
        // Claiming an added list as yours is fine.
        let mut other = Store::in_memory().unwrap();
        other.add_list(&file, None, false).unwrap();
        assert_eq!(
            other.add_list(&file, None, true).unwrap(),
            Installed::Replaced
        );
        assert!(other.list(&list.id).unwrap().owned);
    }

    #[test]
    fn subsecond_timestamps_survive_storage() {
        let (source, list, _, _) = store_with_list();
        let mut store = Store::in_memory().unwrap();
        let file = published(&source, &list, "2026-10-24T17:38:54.558175876Z");
        assert_eq!(store.install_bundled(&file).unwrap(), Installed::Added);
        assert_eq!(store.install_bundled(&file).unwrap(), Installed::Unchanged);
    }

    #[test]
    fn conflicting_ids_are_refused() {
        let (store, list, _, _) = store_with_list();
        let mut file = published(&store, &list, "2026-10-01T00:00:00Z");
        // Another store holding the same group in its own list.
        let mut other = Store::in_memory().unwrap();
        let mine = other.default_list().unwrap();
        for g in store.groups().unwrap() {
            other
                .save_group(&Group {
                    list_id: mine.id.clone(),
                    ..g
                })
                .unwrap();
        }
        file.id = "someone-elses".into();
        assert!(matches!(
            other.add_list(&file, None, false),
            Err(StoreError::Conflict(_))
        ));
    }

    #[test]
    fn removing_a_list_removes_its_contents() {
        let (mut store, list, _, _) = store_with_list();
        store.remove_list(&list.id).unwrap();
        assert!(store.groups().unwrap().is_empty());
        assert!(store.events().unwrap().is_empty());
        assert!(store.lists().unwrap().is_empty());
        // A new list you own appears when one is needed.
        assert_eq!(store.default_list().unwrap().name, "My List");
    }

    #[test]
    fn moving_between_owned_lists() {
        let (mut store, _, g, _) = store_with_list();
        let other = store.create_list("Weekend").unwrap();
        store
            .save_group(&Group {
                list_id: other.id.clone(),
                ..g
            })
            .unwrap();
        assert_eq!(store.groups().unwrap()[0].list_id, other.id);
    }

    #[test]
    fn editing_list_details() {
        let (mut store, list, _, _) = store_with_list();
        store
            .edit_list(
                &list.id,
                " Beach ",
                " Sand things ",
                Some(" https://x.org/l.json "),
            )
            .unwrap();
        let edited = store.list(&list.id).unwrap();
        assert_eq!(edited.name, "Beach");
        assert_eq!(edited.description, "Sand things");
        assert_eq!(edited.update_url.as_deref(), Some("https://x.org/l.json"));
        assert!(edited.updated_at > list.updated_at);
        assert_eq!(
            store.export(&list.id).unwrap().update_url.as_deref(),
            Some("https://x.org/l.json"),
            "exports carry the update URL"
        );
        store.edit_list(&list.id, "Beach", "", Some("  ")).unwrap();
        assert_eq!(store.list(&list.id).unwrap().update_url, None);
    }

    #[test]
    fn remembers_the_last_list_saved_to() {
        let (mut store, list, g, _) = store_with_list();
        assert_eq!(store.last_list().unwrap(), Some(list.id.clone()));
        let other = store.create_list("Weekend").unwrap();
        let mut e = Event::new(&other.id, start());
        e.group_id = Some(g.id.clone());
        store.save_event(&e).unwrap();
        assert_eq!(store.last_list().unwrap(), Some(other.id.clone()));
        store.remove_list(&other.id).unwrap();
        assert_eq!(
            store.last_list().unwrap(),
            None,
            "a removed list is forgotten"
        );
    }

    #[test]
    fn hiding_and_deleting() {
        let (mut store, _, g, _) = store_with_list();
        store.set_hidden(&g.id, true).unwrap();
        assert!(store.hidden_groups().unwrap().contains(&g.id));
        store.delete_group(&g).unwrap();
        assert!(store.groups().unwrap().is_empty());
        assert!(store.events().unwrap().is_empty());
        assert!(store.hidden_groups().unwrap().is_empty());
    }
}
