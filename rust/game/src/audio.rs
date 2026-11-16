//! Small helpers for building sound variations from numbered files.

use godot::classes::{AudioStream, AudioStreamRandomizer, ResourceLoader};
use godot::prelude::*;

/// Loads `<base>.ogg`, or `<base>_1.ogg`, `<base>_2.ogg`, ... into a randomizer
/// that never repeats the same file twice in a row. `.wav` files work too; short
/// transient sounds use them because Vorbis smears sharp attacks. Returns `None`
/// when no file exists, so missing optional sounds are skipped silently.
pub fn variations(base: &str, pitch: f32) -> Option<Gd<AudioStreamRandomizer>> {
    let mut loader = ResourceLoader::singleton();
    let mut paths = Vec::new();
    paths.extend(find(&mut loader, base));
    for i in 1..=8 {
        let Some(path) = find(&mut loader, &format!("{base}_{i}")) else {
            break;
        };
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

/// The path of `<stem>.ogg` or `<stem>.wav`, whichever exists.
fn find(loader: &mut Gd<ResourceLoader>, stem: &str) -> Option<String> {
    ["ogg", "wav"]
        .into_iter()
        .map(|ext| format!("{stem}.{ext}"))
        .find(|path| loader.exists(path))
}
