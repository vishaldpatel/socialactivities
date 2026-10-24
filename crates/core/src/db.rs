//! SQLite schema and queries.

use std::path::Path;
use std::sync::LazyLock;

use chrono::{DateTime, NaiveDate, NaiveDateTime, SecondsFormat, Utc};
use rusqlite::{Connection, OptionalExtension, Statement, Transaction, params};
use rusqlite_migration::{M, Migrations};

use crate::model::{Cost, CostKind, Event, Group, Link, ListInfo};

const DATETIME_FMT: &str = "%Y-%m-%dT%H:%M:%S";
const DATE_FMT: &str = "%Y-%m-%d";

/// Schema migrations, oldest first. Never edit a released entry; append a new one.
const SCHEMA: &[&str] = &[
    "CREATE TABLE groups (
            id            TEXT PRIMARY KEY,
            name          TEXT NOT NULL,
            description   TEXT NOT NULL DEFAULT '',
            logo_path     TEXT,
            neighborhood  TEXT NOT NULL,
            address       TEXT NOT NULL DEFAULT '',
            frequency     TEXT NOT NULL DEFAULT '',
            cost_kind     TEXT NOT NULL DEFAULT 'free',
            cost_note     TEXT NOT NULL DEFAULT '',
            last_verified TEXT
        );
        CREATE TABLE group_tags (
            group_id TEXT NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
            tag      TEXT NOT NULL,
            PRIMARY KEY (group_id, tag)
        );
        CREATE TABLE group_links (
            group_id TEXT NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
            position INTEGER NOT NULL,
            kind     TEXT NOT NULL,
            url      TEXT NOT NULL,
            PRIMARY KEY (group_id, position)
        );
        -- group_id is not a foreign key: a user's event may belong to a group
        -- that lives in the defaults database.
        CREATE TABLE events (
            id               TEXT PRIMARY KEY,
            group_id         TEXT,
            title            TEXT NOT NULL,
            description      TEXT NOT NULL DEFAULT '',
            neighborhood     TEXT NOT NULL,
            location         TEXT NOT NULL DEFAULT '',
            start            TEXT NOT NULL,
            duration_minutes INTEGER NOT NULL,
            rrule            TEXT,
            cost_kind        TEXT NOT NULL DEFAULT 'free',
            cost_note        TEXT NOT NULL DEFAULT '',
            url              TEXT NOT NULL DEFAULT ''
        );
        CREATE INDEX events_group ON events(group_id);
        CREATE TABLE event_tags (
            event_id TEXT NOT NULL REFERENCES events(id) ON DELETE CASCADE,
            tag      TEXT NOT NULL,
            PRIMARY KEY (event_id, tag)
        );
        CREATE TABLE event_cancellations (
            event_id TEXT NOT NULL REFERENCES events(id) ON DELETE CASCADE,
            date     TEXT NOT NULL,
            PRIMARY KEY (event_id, date)
        );
        -- Only meaningful in the user database; may reference groups in either.
        CREATE TABLE hidden_groups (
            group_id TEXT PRIMARY KEY
        );",
    // v2: collapse the original twelve categories into "active" and "chill".
    // "outdoors" alone counts as active, but not alongside a chill category
    // (stargazing is outdoors, but hardly a workout).
    "CREATE TEMP TABLE chill_tags (tag TEXT PRIMARY KEY);
        INSERT INTO chill_tags VALUES ('games'), ('trivia'), ('language'), ('professional'),
            ('volunteering'), ('creative'), ('festival'), ('sky');

        INSERT OR IGNORE INTO group_tags (group_id, tag)
            SELECT group_id, 'chill' FROM group_tags WHERE tag IN (SELECT tag FROM chill_tags);
        INSERT OR IGNORE INTO group_tags (group_id, tag)
            SELECT group_id, 'active' FROM group_tags g
            WHERE tag IN ('fitness', 'water', 'dance')
               OR (tag = 'outdoors' AND NOT EXISTS (
                   SELECT 1 FROM group_tags c
                   WHERE c.group_id = g.group_id AND c.tag IN (SELECT tag FROM chill_tags)));
        DELETE FROM group_tags WHERE tag NOT IN ('active', 'chill');

        INSERT OR IGNORE INTO event_tags (event_id, tag)
            SELECT event_id, 'chill' FROM event_tags WHERE tag IN (SELECT tag FROM chill_tags);
        INSERT OR IGNORE INTO event_tags (event_id, tag)
            SELECT event_id, 'active' FROM event_tags e
            WHERE tag IN ('fitness', 'water', 'dance')
               OR (tag = 'outdoors' AND NOT EXISTS (
                   SELECT 1 FROM event_tags c
                   WHERE c.event_id = e.event_id AND c.tag IN (SELECT tag FROM chill_tags)));
        DELETE FROM event_tags WHERE tag NOT IN ('active', 'chill');

        DROP TABLE chill_tags;",
    // v3: the fixed categories move to their own tables, and `*_tags` now hold
    // free-form tags.
    "ALTER TABLE group_tags RENAME TO group_categories;
        ALTER TABLE group_categories RENAME COLUMN tag TO category;
        ALTER TABLE event_tags RENAME TO event_categories;
        ALTER TABLE event_categories RENAME COLUMN tag TO category;
        CREATE TABLE group_tags (
            group_id TEXT NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
            tag      TEXT NOT NULL,
            PRIMARY KEY (group_id, tag)
        );
        CREATE TABLE event_tags (
            event_id TEXT NOT NULL REFERENCES events(id) ON DELETE CASCADE,
            tag      TEXT NOT NULL,
            PRIMARY KEY (event_id, tag)
        );",
    // v4: everything belongs to a list. Existing records move into a new
    // "My List". list_id isn't a foreign key because SQLite can't add one with
    // ALTER TABLE; the store keeps it consistent.
    "CREATE TABLE lists (
            id          TEXT PRIMARY KEY,
            name        TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            updated_at  TEXT NOT NULL,
            owned       INTEGER NOT NULL,
            source      TEXT,
            update_url  TEXT
        );
        ALTER TABLE groups ADD COLUMN list_id TEXT;
        ALTER TABLE events ADD COLUMN list_id TEXT;
        CREATE INDEX groups_list ON groups(list_id);
        CREATE INDEX events_list ON events(list_id);
        INSERT INTO lists (id, name, updated_at, owned) VALUES
            (lower(hex(randomblob(16))), 'My List', strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), 1);
        UPDATE groups SET list_id = (SELECT id FROM lists);
        UPDATE events SET list_id = (SELECT id FROM lists);
        -- Lists the user removed, so the one shipped with the app isn't reinstalled.
        CREATE TABLE removed_lists (id TEXT PRIMARY KEY);",
];

static MIGRATIONS: LazyLock<Migrations<'static>> =
    LazyLock::new(|| Migrations::new(SCHEMA.iter().map(|sql| M::up(sql)).collect()));

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Migration(#[from] rusqlite_migration::Error),
    #[error("bad value in column {column}: {value:?}")]
    BadValue { column: &'static str, value: String },
}

pub type Result<T> = std::result::Result<T, DbError>;

/// Opens (creating if needed) a writable database and migrates it to the latest schema.
pub fn open_rw(path: &Path) -> Result<Connection> {
    let mut conn = Connection::open(path)?;
    conn.pragma_update(None, "foreign_keys", true)?;
    MIGRATIONS.to_latest(&mut conn)?;
    Ok(conn)
}

/// An empty, migrated in-memory database.
pub fn open_memory() -> Result<Connection> {
    let mut conn = Connection::open_in_memory()?;
    conn.pragma_update(None, "foreign_keys", true)?;
    MIGRATIONS.to_latest(&mut conn)?;
    Ok(conn)
}

fn parse_datetime(s: String) -> Result<NaiveDateTime> {
    NaiveDateTime::parse_from_str(&s, DATETIME_FMT).map_err(|_| DbError::BadValue {
        column: "start",
        value: s,
    })
}

fn parse_date(column: &'static str, s: String) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(&s, DATE_FMT).map_err(|_| DbError::BadValue { column, value: s })
}

/// Reads a one-column list of strings for one record with a prepared `stmt`.
fn load_values(stmt: &mut Statement, id: &str) -> Result<Vec<String>> {
    Ok(stmt
        .query_map([id], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?)
}

/// Replaces the `value_col` rows of `table` that belong to record `id`.
fn replace_values(
    tx: &Transaction,
    (table, id_col, value_col): (&str, &str, &str),
    id: &str,
    values: &[String],
) -> Result<()> {
    tx.execute(&format!("DELETE FROM {table} WHERE {id_col} = ?1"), [id])?;
    let insert = format!("INSERT OR IGNORE INTO {table} ({id_col}, {value_col}) VALUES (?1, ?2)");
    for value in values {
        tx.execute(&insert, [id, value])?;
    }
    Ok(())
}

const GROUP_CATEGORIES: (&str, &str, &str) = ("group_categories", "group_id", "category");
const GROUP_TAGS: (&str, &str, &str) = ("group_tags", "group_id", "tag");
const EVENT_CATEGORIES: (&str, &str, &str) = ("event_categories", "event_id", "category");
const EVENT_TAGS: (&str, &str, &str) = ("event_tags", "event_id", "tag");

fn values_query<'c>(
    conn: &'c Connection,
    (table, id_col, value_col): (&str, &str, &str),
) -> Result<Statement<'c>> {
    Ok(conn.prepare(&format!(
        "SELECT {value_col} FROM {table} WHERE {id_col} = ?1 ORDER BY {value_col}"
    ))?)
}

pub fn load_groups(conn: &Connection) -> Result<Vec<Group>> {
    let mut categories = values_query(conn, GROUP_CATEGORIES)?;
    let mut tags = values_query(conn, GROUP_TAGS)?;
    let mut links =
        conn.prepare("SELECT kind, url FROM group_links WHERE group_id = ?1 ORDER BY position")?;
    let mut stmt = conn.prepare(
        "SELECT id, name, description, logo_path, neighborhood, address, frequency,
                cost_kind, cost_note, last_verified, list_id
         FROM groups ORDER BY name COLLATE NOCASE",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            Group {
                id: r.get(0)?,
                list_id: r.get(10)?,
                name: r.get(1)?,
                description: r.get(2)?,
                logo_path: r.get(3)?,
                neighborhood: r.get(4)?,
                address: r.get(5)?,
                frequency: r.get(6)?,
                cost: Cost {
                    kind: CostKind::parse(&r.get::<_, String>(7)?),
                    note: r.get(8)?,
                },
                categories: Vec::new(),
                tags: Vec::new(),
                links: Vec::new(),
                last_verified: None,
            },
            r.get::<_, Option<String>>(9)?,
        ))
    })?;

    let mut groups = Vec::new();
    for row in rows {
        let (mut g, verified) = row?;
        g.last_verified = verified
            .map(|v| parse_date("last_verified", v))
            .transpose()?;
        g.categories = load_values(&mut categories, &g.id)?;
        g.tags = load_values(&mut tags, &g.id)?;
        g.links = links
            .query_map([&g.id], |r| {
                Ok(Link {
                    kind: r.get(0)?,
                    url: r.get(1)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        groups.push(g);
    }
    Ok(groups)
}

pub fn save_group(conn: &mut Connection, g: &Group) -> Result<()> {
    let tx = conn.transaction()?;
    put_group(&tx, g)?;
    tx.commit()?;
    Ok(())
}

fn put_group(tx: &Transaction, g: &Group) -> Result<()> {
    tx.execute(
        "INSERT INTO groups (id, name, description, logo_path, neighborhood, address,
                             frequency, cost_kind, cost_note, last_verified, list_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
         ON CONFLICT(id) DO UPDATE SET
            list_id = excluded.list_id,
            name = excluded.name, description = excluded.description,
            logo_path = excluded.logo_path, neighborhood = excluded.neighborhood,
            address = excluded.address, frequency = excluded.frequency,
            cost_kind = excluded.cost_kind, cost_note = excluded.cost_note,
            last_verified = excluded.last_verified",
        params![
            g.id,
            g.name,
            g.description,
            g.logo_path,
            g.neighborhood,
            g.address,
            g.frequency,
            g.cost.kind.as_str(),
            g.cost.note,
            g.last_verified.map(|d| d.format(DATE_FMT).to_string()),
            g.list_id,
        ],
    )?;
    replace_values(tx, GROUP_CATEGORIES, &g.id, &g.categories)?;
    replace_values(tx, GROUP_TAGS, &g.id, &g.tags)?;
    tx.execute("DELETE FROM group_links WHERE group_id = ?1", [&g.id])?;
    for (i, link) in g.links.iter().enumerate() {
        tx.execute(
            "INSERT INTO group_links (group_id, position, kind, url) VALUES (?1, ?2, ?3, ?4)",
            params![g.id, i as i64, link.kind, link.url],
        )?;
    }
    Ok(())
}

/// Deletes a group and its events in the same list. Events in other lists
/// that refer to it are left, without a group.
pub fn delete_group(conn: &mut Connection, id: &str, list_id: &str) -> Result<()> {
    let tx = conn.transaction()?;
    tx.execute(
        "DELETE FROM events WHERE group_id = ?1 AND list_id = ?2",
        [id, list_id],
    )?;
    tx.execute("DELETE FROM groups WHERE id = ?1", [id])?;
    tx.commit()?;
    Ok(())
}

pub fn load_events(conn: &Connection) -> Result<Vec<Event>> {
    let mut categories = values_query(conn, EVENT_CATEGORIES)?;
    let mut tags = values_query(conn, EVENT_TAGS)?;
    let mut cancelled =
        conn.prepare("SELECT date FROM event_cancellations WHERE event_id = ?1 ORDER BY date")?;
    let mut stmt = conn.prepare(
        "SELECT id, group_id, title, description, neighborhood, location, start,
                duration_minutes, rrule, cost_kind, cost_note, url, list_id
         FROM events ORDER BY start",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(6)?,
            Event {
                id: r.get(0)?,
                list_id: r.get(12)?,
                group_id: r.get(1)?,
                title: r.get(2)?,
                description: r.get(3)?,
                neighborhood: r.get(4)?,
                location: r.get(5)?,
                start: NaiveDateTime::default(),
                duration_minutes: r.get(7)?,
                rrule: r.get(8)?,
                cost: Cost {
                    kind: CostKind::parse(&r.get::<_, String>(9)?),
                    note: r.get(10)?,
                },
                url: r.get(11)?,
                categories: Vec::new(),
                tags: Vec::new(),
                cancelled: Vec::new(),
            },
        ))
    })?;

    let mut events = Vec::new();
    for row in rows {
        let (start, mut e) = row?;
        e.start = parse_datetime(start)?;
        e.categories = load_values(&mut categories, &e.id)?;
        e.tags = load_values(&mut tags, &e.id)?;
        e.cancelled = cancelled
            .query_map([&e.id], |r| r.get::<_, String>(0))?
            .map(|d| parse_date("date", d?))
            .collect::<Result<_>>()?;
        events.push(e);
    }
    Ok(events)
}

pub fn save_event(conn: &mut Connection, e: &Event) -> Result<()> {
    let tx = conn.transaction()?;
    put_event(&tx, e)?;
    tx.commit()?;
    Ok(())
}

fn put_event(tx: &Transaction, e: &Event) -> Result<()> {
    tx.execute(
        "INSERT INTO events (id, group_id, title, description, neighborhood, location, start,
                             duration_minutes, rrule, cost_kind, cost_note, url, list_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
         ON CONFLICT(id) DO UPDATE SET
            list_id = excluded.list_id,
            group_id = excluded.group_id, title = excluded.title,
            description = excluded.description, neighborhood = excluded.neighborhood,
            location = excluded.location, start = excluded.start,
            duration_minutes = excluded.duration_minutes, rrule = excluded.rrule,
            cost_kind = excluded.cost_kind, cost_note = excluded.cost_note, url = excluded.url",
        params![
            e.id,
            e.group_id,
            e.title,
            e.description,
            e.neighborhood,
            e.location,
            e.start.format(DATETIME_FMT).to_string(),
            e.duration_minutes,
            e.rrule,
            e.cost.kind.as_str(),
            e.cost.note,
            e.url,
            e.list_id,
        ],
    )?;
    replace_children(tx, e)
}

fn replace_children(tx: &Transaction, e: &Event) -> Result<()> {
    replace_values(tx, EVENT_CATEGORIES, &e.id, &e.categories)?;
    replace_values(tx, EVENT_TAGS, &e.id, &e.tags)?;
    tx.execute(
        "DELETE FROM event_cancellations WHERE event_id = ?1",
        [&e.id],
    )?;
    for d in &e.cancelled {
        tx.execute(
            "INSERT OR IGNORE INTO event_cancellations (event_id, date) VALUES (?1, ?2)",
            [&e.id, &d.format(DATE_FMT).to_string()],
        )?;
    }
    Ok(())
}

pub fn delete_event(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM events WHERE id = ?1", [id])?;
    Ok(())
}

/// Full precision, so a timestamp read back compares equal to the one written
/// (an update is only applied when strictly newer).
fn format_timestamp(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::AutoSi, true)
}

fn parse_timestamp(s: String) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(&s)
        .map(|t| t.with_timezone(&Utc))
        .map_err(|_| DbError::BadValue {
            column: "updated_at",
            value: s,
        })
}

/// Lists you own first, then by name.
pub fn load_lists(conn: &Connection) -> Result<Vec<ListInfo>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, description, updated_at, owned, source, update_url
         FROM lists ORDER BY owned DESC, name COLLATE NOCASE",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(3)?,
            ListInfo {
                id: r.get(0)?,
                name: r.get(1)?,
                description: r.get(2)?,
                updated_at: DateTime::default(),
                owned: r.get(4)?,
                source: r.get(5)?,
                update_url: r.get(6)?,
            },
        ))
    })?;
    rows.map(|row| {
        let (updated_at, mut list) = row?;
        list.updated_at = parse_timestamp(updated_at)?;
        Ok(list)
    })
    .collect()
}

fn put_list(tx: &Transaction, l: &ListInfo) -> Result<()> {
    tx.execute(
        "INSERT INTO lists (id, name, description, updated_at, owned, source, update_url)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(id) DO UPDATE SET
            name = excluded.name, description = excluded.description,
            updated_at = excluded.updated_at, owned = excluded.owned,
            source = excluded.source, update_url = excluded.update_url",
        params![
            l.id,
            l.name,
            l.description,
            format_timestamp(l.updated_at),
            l.owned,
            l.source,
            l.update_url,
        ],
    )?;
    Ok(())
}

pub fn save_list(conn: &mut Connection, l: &ListInfo) -> Result<()> {
    let tx = conn.transaction()?;
    put_list(&tx, l)?;
    tx.commit()?;
    Ok(())
}

/// Marks a list's contents as changed at `at`.
pub fn touch_list(conn: &Connection, id: &str, at: DateTime<Utc>) -> Result<()> {
    conn.execute(
        "UPDATE lists SET updated_at = ?2 WHERE id = ?1",
        [id, &format_timestamp(at)],
    )?;
    Ok(())
}

/// The list a group or event (by `table`) currently belongs to.
pub fn list_of(conn: &Connection, table: Table, id: &str) -> Result<Option<String>> {
    let sql = format!("SELECT list_id FROM {} WHERE id = ?1", table.name());
    Ok(conn
        .query_row(&sql, [id], |r| r.get(0))
        .optional()?
        .flatten())
}

#[derive(Debug, Clone, Copy)]
pub enum Table {
    Groups,
    Events,
}

impl Table {
    fn name(self) -> &'static str {
        match self {
            Table::Groups => "groups",
            Table::Events => "events",
        }
    }
}

/// Deletes a list and everything in it, and remembers that it was removed.
pub fn delete_list(conn: &mut Connection, id: &str) -> Result<()> {
    let tx = conn.transaction()?;
    clear_list(&tx, id)?;
    tx.execute("DELETE FROM lists WHERE id = ?1", [id])?;
    tx.execute("INSERT OR IGNORE INTO removed_lists (id) VALUES (?1)", [id])?;
    tx.commit()?;
    Ok(())
}

pub fn was_removed(conn: &Connection, id: &str) -> Result<bool> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM removed_lists WHERE id = ?1",
            [id],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

fn clear_list(tx: &Transaction, id: &str) -> Result<()> {
    tx.execute("DELETE FROM events WHERE list_id = ?1", [id])?;
    tx.execute("DELETE FROM groups WHERE list_id = ?1", [id])?;
    Ok(())
}

/// Writes a list and replaces all of its groups and events, atomically.
pub fn replace_list(
    conn: &mut Connection,
    list: &ListInfo,
    groups: &[Group],
    events: &[Event],
) -> Result<()> {
    let tx = conn.transaction()?;
    put_list(&tx, list)?;
    tx.execute("DELETE FROM removed_lists WHERE id = ?1", [&list.id])?;
    clear_list(&tx, &list.id)?;
    for g in groups {
        put_group(&tx, g)?;
    }
    for e in events {
        put_event(&tx, e)?;
    }
    tx.commit()?;
    Ok(())
}

pub fn load_hidden(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT group_id FROM hidden_groups")?;
    let ids = stmt
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(ids)
}

pub fn set_hidden(conn: &Connection, group_id: &str, hidden: bool) -> Result<()> {
    if hidden {
        conn.execute(
            "INSERT OR IGNORE INTO hidden_groups (group_id) VALUES (?1)",
            [group_id],
        )?;
    } else {
        conn.execute("DELETE FROM hidden_groups WHERE group_id = ?1", [group_id])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(conn: &Connection, table: (&str, &str, &str), id: &str) -> Vec<String> {
        load_values(&mut values_query(conn, table).unwrap(), id).unwrap()
    }

    #[test]
    fn v4_moves_existing_records_into_my_list() {
        let mut conn = Connection::open_in_memory().unwrap();
        Migrations::new(SCHEMA[..3].iter().map(|sql| M::up(sql)).collect())
            .to_latest(&mut conn)
            .unwrap();
        conn.execute_batch(
            "INSERT INTO groups (id, name, neighborhood) VALUES ('g', 'G', 'x');
             INSERT INTO events (id, title, neighborhood, start, duration_minutes) VALUES
                ('e', 'E', 'x', '2027-08-12T22:00:00', 60);",
        )
        .unwrap();

        MIGRATIONS.to_latest(&mut conn).unwrap();

        let lists = load_lists(&conn).unwrap();
        assert_eq!(lists.len(), 1);
        assert_eq!(lists[0].name, "My List");
        assert!(lists[0].owned);
        assert_eq!(load_groups(&conn).unwrap()[0].list_id, lists[0].id);
        assert_eq!(load_events(&conn).unwrap()[0].list_id, lists[0].id);
    }

    #[test]
    fn v2_collapses_categories_and_v3_moves_them() {
        let mut conn = Connection::open_in_memory().unwrap();
        Migrations::new(vec![M::up(SCHEMA[0])])
            .to_latest(&mut conn)
            .unwrap();
        conn.execute_batch(
            "INSERT INTO groups (id, name, neighborhood) VALUES
                ('run', 'Run', 'x'), ('games', 'Games', 'x'), ('both', 'Both', 'x');
             INSERT INTO group_tags VALUES
                ('run', 'fitness'), ('run', 'outdoors'),
                ('games', 'games'), ('games', 'trivia'),
                ('both', 'dance'), ('both', 'language');
             INSERT INTO events (id, title, neighborhood, start, duration_minutes) VALUES
                ('stars', 'Stars', 'x', '2027-08-12T22:00:00', 60),
                ('hike', 'Hike', 'x', '2027-08-12T08:00:00', 60);
             INSERT INTO event_tags VALUES
                ('stars', 'sky'), ('stars', 'outdoors'), ('hike', 'outdoors');",
        )
        .unwrap();

        MIGRATIONS.to_latest(&mut conn).unwrap();

        assert_eq!(values(&conn, GROUP_CATEGORIES, "run"), ["active"]);
        assert_eq!(values(&conn, GROUP_CATEGORIES, "games"), ["chill"]);
        assert_eq!(values(&conn, GROUP_CATEGORIES, "both"), ["active", "chill"]);
        assert_eq!(values(&conn, EVENT_CATEGORIES, "stars"), ["chill"]);
        assert_eq!(values(&conn, EVENT_CATEGORIES, "hike"), ["active"]);
        assert!(values(&conn, GROUP_TAGS, "run").is_empty());
        assert!(values(&conn, EVENT_TAGS, "stars").is_empty());
    }
}
