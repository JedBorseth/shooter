//! Loads weapon definitions from `res://data/weapons/*.ron` into the simulation's
//! weapon table, and reloads them when a file changes (debug builds, run from
//! the editor or a source checkout).
//!
//! Exported builds must include `*.ron` in the export preset's non-resource
//! filter, since Godot doesn't pack plain data files by default.

use std::cell::RefCell;

use godot::classes::{DirAccess, FileAccess, Os};
use godot::prelude::*;
use shooter_sim::weapon;

const DIR: &str = "res://data/weapons";
/// Main-loop frames between checks for changed files.
const POLL_FRAMES: u32 = 30;

#[derive(Default)]
struct Watcher {
    frame: u32,
    stamps: Vec<(String, u64)>,
}

thread_local! {
    static WATCHER: RefCell<Watcher> = RefCell::default();
}

fn files() -> Vec<String> {
    let mut names: Vec<String> = DirAccess::get_files_at(DIR)
        .as_slice()
        .iter()
        .map(|name| name.to_string())
        .filter(|name| name.ends_with(".ron"))
        .collect();
    names.sort();
    names
}

fn stamps(names: &[String]) -> Vec<(String, u64)> {
    names
        .iter()
        .map(|name| {
            let path = format!("{DIR}/{name}");
            (name.clone(), FileAccess::get_modified_time(&path))
        })
        .collect()
}

/// Reads every weapon file and installs the table. At startup (`initial`) the
/// set of weapons may differ from the compiled-in one; later reloads may only
/// change numbers.
pub fn load(initial: bool) {
    let names = files();
    if names.is_empty() {
        godot_warn!("no weapon files in {DIR}; using the built-in table");
        return;
    }
    let sources: Vec<(String, String)> = names
        .iter()
        .map(|name| {
            let key = name.trim_end_matches(".ron").to_owned();
            let text = FileAccess::get_file_as_string(&format!("{DIR}/{name}")).to_string();
            (key, text)
        })
        .collect();
    let parsed = weapon::parse_all(sources.iter().map(|(k, s)| (k.as_str(), s.as_str())));
    match parsed.and_then(|defs| weapon::install(defs, initial)) {
        Ok(()) if initial => (),
        Ok(()) => {
            godot_print!("reloaded {} weapon files", names.len());
        }
        Err(error) => {
            godot_error!("{error}");
        }
    }
    WATCHER.with_borrow_mut(|w| w.stamps = stamps(&names));
}

/// Called every main-loop frame; reloads changed files in debug builds.
pub fn poll() {
    if !cfg!(debug_assertions) || Os::singleton().has_feature("template") {
        return;
    }
    let due = WATCHER.with_borrow_mut(|w| {
        w.frame = w.frame.wrapping_add(1);
        w.frame.is_multiple_of(POLL_FRAMES)
    });
    if !due {
        return;
    }
    let current = stamps(&files());
    let changed = WATCHER.with_borrow(|w| w.stamps != current);
    if changed {
        load(false);
    }
}
