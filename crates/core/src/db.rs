//! SQLite schema and queries. The defaults and user databases share one schema.

use std::path::Path;
use std::sync::LazyLock;

use chrono::{NaiveDate, NaiveDateTime};
use rusqlite::{Connection, OpenFlags, Statement, Transaction, params};
use rusqlite_migration::{M, Migrations};

use crate::model::{Cost, CostKind, Event, Group, Link, Source};

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
];

static MIGRATIONS: LazyLock<Migrations<'static>> =
    LazyLock::new(|| Migrations::new(SCHEMA.iter().map(|sql| M::up(sql)).collect()));

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Migration(#[from] rusqlite_migration::Error),
    #[error("database schema version {found} does not match the app's version {expected}")]
    SchemaMismatch { found: i64, expected: i64 },
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

/// Opens an existing database read-only. The schema must already be current,
/// since a read-only connection can't be migrated.
pub fn open_ro(path: &Path) -> Result<Connection> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let found: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    let expected = SCHEMA.len() as i64;
    if found != expected {
        return Err(DbError::SchemaMismatch { found, expected });
    }
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
fn load_list(stmt: &mut Statement, id: &str) -> Result<Vec<String>> {
    Ok(stmt
        .query_map([id], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?)
}

/// Replaces the `value_col` rows of `table` that belong to record `id`.
fn replace_list(
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

fn list_query<'c>(
    conn: &'c Connection,
    (table, id_col, value_col): (&str, &str, &str),
) -> Result<Statement<'c>> {
    Ok(conn.prepare(&format!(
        "SELECT {value_col} FROM {table} WHERE {id_col} = ?1 ORDER BY {value_col}"
    ))?)
}

pub fn load_groups(conn: &Connection, source: Source) -> Result<Vec<Group>> {
    let mut categories = list_query(conn, GROUP_CATEGORIES)?;
    let mut tags = list_query(conn, GROUP_TAGS)?;
    let mut links =
        conn.prepare("SELECT kind, url FROM group_links WHERE group_id = ?1 ORDER BY position")?;
    let mut stmt = conn.prepare(
        "SELECT id, name, description, logo_path, neighborhood, address, frequency,
                cost_kind, cost_note, last_verified
         FROM groups ORDER BY name COLLATE NOCASE",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            Group {
                id: r.get(0)?,
                source,
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
        g.categories = load_list(&mut categories, &g.id)?;
        g.tags = load_list(&mut tags, &g.id)?;
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
    tx.execute(
        "INSERT INTO groups (id, name, description, logo_path, neighborhood, address,
                             frequency, cost_kind, cost_note, last_verified)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
         ON CONFLICT(id) DO UPDATE SET
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
        ],
    )?;
    replace_list(&tx, GROUP_CATEGORIES, &g.id, &g.categories)?;
    replace_list(&tx, GROUP_TAGS, &g.id, &g.tags)?;
    tx.execute("DELETE FROM group_links WHERE group_id = ?1", [&g.id])?;
    for (i, link) in g.links.iter().enumerate() {
        tx.execute(
            "INSERT INTO group_links (group_id, position, kind, url) VALUES (?1, ?2, ?3, ?4)",
            params![g.id, i as i64, link.kind, link.url],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// Deletes a group and the events in the same database that belong to it.
pub fn delete_group(conn: &mut Connection, id: &str) -> Result<()> {
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM events WHERE group_id = ?1", [id])?;
    tx.execute("DELETE FROM groups WHERE id = ?1", [id])?;
    tx.commit()?;
    Ok(())
}

pub fn load_events(conn: &Connection, source: Source) -> Result<Vec<Event>> {
    let mut categories = list_query(conn, EVENT_CATEGORIES)?;
    let mut tags = list_query(conn, EVENT_TAGS)?;
    let mut cancelled =
        conn.prepare("SELECT date FROM event_cancellations WHERE event_id = ?1 ORDER BY date")?;
    let mut stmt = conn.prepare(
        "SELECT id, group_id, title, description, neighborhood, location, start,
                duration_minutes, rrule, cost_kind, cost_note, url
         FROM events ORDER BY start",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(6)?,
            Event {
                id: r.get(0)?,
                source,
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
        e.categories = load_list(&mut categories, &e.id)?;
        e.tags = load_list(&mut tags, &e.id)?;
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
    tx.execute(
        "INSERT INTO events (id, group_id, title, description, neighborhood, location, start,
                             duration_minutes, rrule, cost_kind, cost_note, url)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
         ON CONFLICT(id) DO UPDATE SET
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
        ],
    )?;
    replace_children(&tx, e)?;
    tx.commit()?;
    Ok(())
}

fn replace_children(tx: &Transaction, e: &Event) -> Result<()> {
    replace_list(tx, EVENT_CATEGORIES, &e.id, &e.categories)?;
    replace_list(tx, EVENT_TAGS, &e.id, &e.tags)?;
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

    fn list(conn: &Connection, table: (&str, &str, &str), id: &str) -> Vec<String> {
        load_list(&mut list_query(conn, table).unwrap(), id).unwrap()
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

        assert_eq!(list(&conn, GROUP_CATEGORIES, "run"), ["active"]);
        assert_eq!(list(&conn, GROUP_CATEGORIES, "games"), ["chill"]);
        assert_eq!(list(&conn, GROUP_CATEGORIES, "both"), ["active", "chill"]);
        assert_eq!(list(&conn, EVENT_CATEGORIES, "stars"), ["chill"]);
        assert_eq!(list(&conn, EVENT_CATEGORIES, "hike"), ["active"]);
        assert!(list(&conn, GROUP_TAGS, "run").is_empty());
        assert!(list(&conn, EVENT_TAGS, "stars").is_empty());
    }
}
