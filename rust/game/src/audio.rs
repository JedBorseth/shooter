//! Small helpers for building sound variations from numbered files.

use godot::classes::{AudioStream, AudioStreamRandomizer, ResourceLoader};
use godot::prelude::*;

/// Loads `<base>.ogg`, or `<base>_1.ogg`, `<base>_2.ogg`, ... into a randomizer
/// that never repeats the same file twice in a row. Returns `None` when no file
/// exists, so missing optional sounds are skipped silently.
pub fn variations(base: &str, pitch: f32) -> Option<Gd<AudioStreamRandomizer>> {
    let mut loader = ResourceLoader::singleton();
    let mut paths = Vec::new();
    let single = format!("{base}.ogg");
    if loader.exists(&single) {
        paths.push(single);
    }
    for i in 1..=8 {
        let path = format!("{base}_{i}.ogg");
        if !loader.exists(&path) {
            break;
        }
        paths.push(path);
    }
    if paths.is_empty() {
        return None;
    }
    let mut randomizer = AudioStreamRandomizer::new_gd();
    for path in &paths {
        if let Some(stream) = loader
            .load(path)
            .and_then(|r| r.try_cast::<AudioStream>().ok())
        {
            randomizer.add_stream(-1, &stream);
        }
    }
    randomizer.set_random_pitch(pitch);
    randomizer.set_random_volume_offset_db(1.5);
    Some(randomizer)
}
