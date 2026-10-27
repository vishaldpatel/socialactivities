// Release builds on Windows open without a console window beside them.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod calendar;
mod focus;
mod focusable;
mod form;
mod help;
mod list;
mod lists;
mod location;
mod saved;
mod search;
mod sky;
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

/// The app's id on Linux, matching its desktop file's name.
#[cfg(target_os = "linux")]
const APP_ID: &str = "io.github.vishaldpatel.sdsa";

/// The window icon, 64×64 RGBA, made by `packaging/make-icons.sh`.
const ICON: &[u8] = include_bytes!("../../../packaging/icon-64.rgba");

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

/// The list that shipped before the run clubs, mostly placeholder groups.
/// Removed from databases that still have a read-only copy.
const RETIRED_LIST: &str = "sd-social-activities";

/// Installs or updates the bundled list, returning a message worth showing.
fn install_bundled(store: &mut Store) -> Option<(String, bool)> {
    let retired = store
        .lists()
        .is_ok_and(|lists| lists.iter().any(|l| l.id == RETIRED_LIST && !l.owned));
    if retired && let Err(e) = store.remove_list(RETIRED_LIST) {
        return Some((format!("Couldn't remove the old built-in list: {e}"), true));
    }
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
        Ok((store, status))
    });
    let opened = match opened {
        Ok(opened) => opened,
        Err(msg) => {
            eprintln!("{msg}");
            // A Windows release has no console to print to.
            #[cfg(windows)]
            rfd::MessageDialog::new()
                .set_level(rfd::MessageLevel::Error)
                .set_title("San Diego Social Activities")
                .set_description(&msg)
                .show();
            std::process::exit(2);
        }
    };

    // `boot` must be `Fn`, but it only runs once.
    let opened = Cell::new(Some(opened));
    iced::application(
        move || {
            let (store, status) = opened.take().expect("application booted twice");
            app::App::boot(store, status)
        },
        app::App::update,
        app::App::view,
    )
    .title(app::App::title)
    .subscription(app::App::subscription)
    .window(window_settings())
    .scale_factor(|_| SCALE)
    .window_size((1100.0 * SCALE, 760.0 * SCALE))
    .run()
}

fn window_settings() -> iced::window::Settings {
    iced::window::Settings {
        icon: iced::window::icon::from_rgba(ICON.to_vec(), 64, 64).ok(),
        // Wayland matches this to the desktop file for the dock icon and name.
        #[cfg(target_os = "linux")]
        platform_specific: iced::window::settings::PlatformSpecific {
            application_id: APP_ID.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}
