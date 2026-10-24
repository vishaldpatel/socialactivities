# San Diego Social Activities

A desktop app for browsing groups and events in San Diego County where you
actually take part (hiking, volleyball, board games, language exchanges,
dancing), as opposed to watching. See [spec.md](spec.md) for the original idea.

Built with Rust, [Iced](https://iced.rs) 0.14 and SQLite.

## Running

```sh
mise install            # installs the Rust toolchain pinned in mise.toml
cargo run -p sdsa       # optionally: -- --user PATH to use another database
cargo test --workspace
```

## Layout

```
crates/core          sdsa-core: model, SQLite storage, list files, recurrence (no UI)
crates/app           sdsa: the Iced desktop app
data/defaults.json   the curated list compiled into the app
```

## Lists

Every group and event belongs to a **list**. Everything lives in one SQLite
database in the platform data dir (e.g.
`~/.local/share/sd-social-activities/user.sqlite`).

- **Lists you own** ("Yours") can be edited, renamed, exported and removed.
  Everyone starts with "My List"; the editors' "Save to list" picks which one.
- **Added lists** come from a `.json` file or URL and are read-only. "Check for
  Updates" re-reads the list's update URL (or wherever it was added from) and
  offers the new version only if its `updated_at` is newer. Your own events can
  still belong to their groups; they live in your list and survive updates.
- **The built-in list** is `data/defaults.json`, compiled into the app. It's
  installed on first run and replaced whenever a newer copy ships, unless you
  maintain it or removed it.

Hidden groups are remembered by id across all lists. Group and event ids must
be unique across lists; adding a list that reuses another list's ids is
refused.

The list file format is defined in `crates/core/src/listfile.rs`. Logos are
local paths and aren't exported.

## Curating the built-in list

1. In the app's Lists panel, add `data/defaults.json` with **I maintain this
   list** ticked. It becomes editable on your machine and is never overwritten
   by the bundled copy or by updates.
2. Edit its groups and events as usual. Each edit bumps its `updated_at`.
3. **Export…** it back over `data/defaults.json` and rebuild. Everyone else's
   app replaces their read-only copy with yours because it's newer.

To let people get changes without a new app release, host the exported file
somewhere and add `"update_url": "https://…"` to it; "Check for Updates" then
fetches from there.

The **Example:** groups are placeholders. Replace them with real, verified
groups, using the "Last verified" field to track freshness.

## Other details

Recurring events store an iCalendar RRULE (`FREQ=WEEKLY;BYDAY=TU`) plus a start
time in San Diego local time, so times stay put across daylight saving changes.
The editor offers presets (weekly, every other week, monthly on the Nth weekday)
and a custom RRULE field. Individual dates can be cancelled.

Neighborhoods and cities roll up into four regions (North County, San Diego
City, East County, South County); those, and the two categories ("Shake 'n
Bake" and "Chill"), are fixed lists in `crates/core/src/catalog.rs`. Records
marked "Countywide / Various" match every region. Tags are free-form, stored
lowercase, and an event inherits its group's tags and categories.

The schema lives in `crates/core/src/db.rs` as an append-only list of
migrations; databases are upgraded when the app opens them.
