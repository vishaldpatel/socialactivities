mod app;
mod form;
mod list;
mod ui;

use std::cell::Cell;
use std::path::PathBuf;

use sdsa_core::Store;

const USAGE: &str = "\
usage: sdsa [--admin] [--defaults PATH] [--user PATH]

  --admin          edit the curated defaults database instead of only your own data
  --defaults PATH  curated defaults database (also: SDSA_DEFAULTS)
  --user PATH      your personal database (default: platform data directory)";

struct Args {
    admin: bool,
    defaults: Option<PathBuf>,
    user: Option<PathBuf>,
}

impl Args {
    fn parse() -> Result<Self, String> {
        let mut args = Args {
            admin: false,
            defaults: None,
            user: None,
        };
        let mut it = std::env::args().skip(1);
        while let Some(arg) = it.next() {
            match arg.as_str() {
                "--admin" => args.admin = true,
                "--defaults" => {
                    args.defaults = Some(it.next().ok_or("--defaults needs a path")?.into())
                }
                "--user" => args.user = Some(it.next().ok_or("--user needs a path")?.into()),
                "-h" | "--help" => return Err(USAGE.into()),
                other => return Err(format!("unknown argument {other:?}\n\n{USAGE}")),
            }
        }
        Ok(args)
    }
}

/// The curated defaults: an explicit path, then `defaults.sqlite` next to the
/// executable (how releases ship it), then the copy in this repository.
fn defaults_path(arg: Option<PathBuf>) -> PathBuf {
    if let Some(p) = arg.or_else(|| std::env::var_os("SDSA_DEFAULTS").map(PathBuf::from)) {
        return p;
    }
    let beside_exe = std::env::current_exe()
        .ok()
        .and_then(|exe| Some(exe.parent()?.join("defaults.sqlite")));
    match beside_exe {
        Some(p) if p.exists() => p,
        _ => PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../data/defaults.sqlite"
        )),
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

fn main() -> iced::Result {
    let store = match Args::parse().and_then(|args| {
        let user = user_path(args.user)?;
        let defaults = defaults_path(args.defaults);
        Store::open(&defaults, &user, args.admin)
            .map(|store| (store, defaults, user))
            .map_err(|e| e.to_string())
    }) {
        Ok(opened) => opened,
        Err(msg) => {
            eprintln!("{msg}");
            std::process::exit(2);
        }
    };

    // `boot` must be `Fn`, but it only runs once.
    let store = Cell::new(Some(store));
    iced::application(
        move || {
            let (store, defaults, user) = store.take().expect("application booted twice");
            app::App::new(store, defaults, user)
        },
        app::App::update,
        app::App::view,
    )
    .title(app::App::title)
    .window_size((1100.0, 760.0))
    .run()
}
