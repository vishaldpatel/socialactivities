//! Creates a fresh defaults database populated with the seed data.
//!
//! Usage: build-defaults [--force] [PATH]   (default PATH: data/defaults.sqlite)

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut force = false;
    let mut path = PathBuf::from("data/defaults.sqlite");
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--force" => force = true,
            "-h" | "--help" => {
                println!("usage: build-defaults [--force] [PATH]");
                return ExitCode::SUCCESS;
            }
            _ => path = arg.into(),
        }
    }

    if path.exists() {
        if !force {
            eprintln!(
                "{} already exists and may hold curated edits; pass --force to replace it",
                path.display()
            );
            return ExitCode::FAILURE;
        }
        if let Err(e) = std::fs::remove_file(&path) {
            eprintln!("couldn't remove {}: {e}", path.display());
            return ExitCode::FAILURE;
        }
    }

    // The user database is irrelevant here; admin mode writes to the defaults.
    let scratch = std::env::temp_dir().join(format!("sdsa-seed-{}.sqlite", std::process::id()));
    let result = sdsa_core::Store::open(&path, &scratch, true)
        .and_then(|mut store| sdsa_core::seed::seed(&mut store));
    let _ = std::fs::remove_file(&scratch);

    match result {
        Ok(()) => {
            println!("wrote {}", path.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
