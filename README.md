# San Diego Social Activities

A desktop app for browsing groups and events in San Diego County where you
actually take part (hiking, volleyball, board games, language exchanges,
dancing), as opposed to watching. See [spec.md](spec.md) for the original idea.

Built with Rust, [Iced](https://iced.rs) 0.14 and SQLite.

## Running

```sh
mise install            # installs the Rust toolchain pinned in mise.toml
cargo run -p sdsa       # browse
cargo run -p sdsa -- --admin   # edit the curated defaults
cargo test --workspace
```

## Layout

```
crates/core   sdsa-core: model, SQLite storage, recurrence expansion (no UI)
crates/app    sdsa: the Iced desktop app
data/defaults.sqlite   curated groups & events shipped with releases
```

## Data model

There are two databases with the same schema:

| Database | Where | Contents |
|---|---|---|
| **defaults** | `defaults.sqlite` next to the executable, falling back to `data/defaults.sqlite` in this repo; override with `--defaults PATH` or `SDSA_DEFAULTS` | Curated groups and events. Read-only unless the app runs with `--admin`. |
| **user** | Platform data dir, e.g. `~/.local/share/sd-social-activities/user.sqlite`; override with `--user PATH` | Groups and events the user adds, plus the groups they've hidden. |

The app shows both together. Shipping new defaults never touches the user's
database, and hidden groups are stored by id, so curated records keep stable
slug ids (see `crates/core/src/seed.rs`).

Recurring events store an iCalendar RRULE (`FREQ=WEEKLY;BYDAY=TU`) plus a start
time in San Diego local time, so times stay put across daylight saving changes.
The editor offers presets (weekly, every other week, monthly on the Nth weekday)
and a custom RRULE field. Individual dates can be cancelled.

Neighborhoods and categories are fixed lists in `crates/core/src/catalog.rs`.

## Curating the defaults

```sh
cargo run -p sdsa -- --admin             # add/edit records in data/defaults.sqlite
cargo run -p sdsa-core --bin build-defaults -- --force   # reset to the seed data
```

The seed groups are clearly labelled **Example:** placeholders. Replace them
with real, verified groups, using the "Last verified" field to track freshness.

The schema lives in `crates/core/src/db.rs` as an append-only list of
migrations. After changing it, rebuild or migrate the defaults (open it with
`--admin`), because the app opens the defaults read-only and refuses a schema
mismatch.
