mod app;
mod calendar;
mod focus;
mod focusable;
mod form;
mod list;
mod lists;
mod location;
mod search;
mod ui;

use std::cell::Cell;
use std::path::PathBuf;

use sdsa_core::store::Installed;
use sdsa_core::{ListFile, Store};

const USAGE: &str = "\
usage: sdsa [--user PATH]

  --user PATH  your database (default: platform data directory)";

/// Draws the whole interface this much larger than Iced's defaults.
const SCALE: f32 = 1.25;

/// The curated list that ships with the app. Installed on first run and
/// replaced whenever a newer copy ships, unless you maintain it or removed it.
const BUNDLED_LIST: &str = include_str!("../../../data/defaults.json");

struct Args {
    user: Option<PathBuf>,
}

impl Args {
    fn parse() -> Result<Self, String> {
        let mut args = Args { user: None };
        let mut it = std::env::args().skip(1);
        while let Some(arg) = it.next() {
            match arg.as_str() {
                "--user" => args.user = Some(it.next().ok_or("--user needs a path")?.into()),
                "-h" | "--help" => return Err(USAGE.into()),
                other => return Err(format!("unknown argument {other:?}\n\n{USAGE}")),
            }
        }
        Ok(args)
    }
}

fn user_path(arg: Option<PathBuf>) -> Result<PathBuf, String> {
    if let Some(p) = arg {
        return Ok(p);
    }
    let dirs = directories::ProjectDirs::from("", "", "sd-social-activities")
        .ok_or("couldn't determine a data directory; pass --user PATH")?;
    let dir = dirs.data_dir();
    std::fs::create_dir_all(dir).map_err(|e| format!("couldn't create {}: {e}", dir.display()))?;
    Ok(dir.join("user.sqlite"))
}

/// Installs or updates the bundled list, returning a message worth showing.
fn install_bundled(store: &mut Store) -> Option<(String, bool)> {
    let result = ListFile::parse(BUNDLED_LIST)
        .map_err(sdsa_core::store::StoreError::from)
        .and_then(|file| Ok((store.install_bundled(&file)?, file.name)));
    match result {
        Ok((Installed::Replaced, name)) => Some((format!("Updated “{name}”"), false)),
        Ok(_) => None,
        Err(e) => Some((format!("Couldn't install the built-in list: {e}"), true)),
    }
}

fn main() -> iced::Result {
    let opened = Args::parse().and_then(|args| {
        let path = user_path(args.user)?;
        let mut store = Store::open(&path).map_err(|e| e.to_string())?;
        let status = install_bundled(&mut store);
        Ok((store, path, status))
    });
    let opened = match opened {
        Ok(opened) => opened,
        Err(msg) => {
            eprintln!("{msg}");
            std::process::exit(2);
        }
    };

    // `boot` must be `Fn`, but it only runs once.
    let opened = Cell::new(Some(opened));
    iced::application(
        move || {
            let (store, path, status) = opened.take().expect("application booted twice");
            app::App::new(store, path, status)
        },
        app::App::update,
        app::App::view,
    )
    .title(app::App::title)
    .subscription(app::App::subscription)
    .scale_factor(|_| SCALE)
    .window_size((1100.0 * SCALE, 760.0 * SCALE))
    .run()
}
