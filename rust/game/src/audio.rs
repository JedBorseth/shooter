//! Small helpers for building sound variations from numbered files.

use godot::classes::{AudioStream, AudioStreamRandomizer, ResourceLoader};
use godot::prelude::*;

/// Voices for a gunshot whose snap layer is the whole shot (room and echo
/// included, about 1.2 s): enough that full auto at 600 rounds a minute never
/// cuts a ringing shot short.
pub const WHOLE_SHOT_VOICES: i32 = 14;

/// Loads `<base>.ogg`, or `<base>_1.ogg`, `<base>_2.ogg`, ... into a randomizer
/// that never repeats the same file twice in a row. `.wav` files work too; short
/// transient sounds use them because Vorbis smears sharp attacks. Returns `None`
/// when no file exists, so missing optional sounds are skipped silently.
/// Whose shot a weapon plays (its key under `assets/audio/weapons/`): its
/// own once it has one (`snap` takes), or else the weapon it borrows from
/// (`lender`, the view's `sound_from`; empty for none). A new weapon ships
/// on a close one's shot, and its own takes over as soon as its files land.
pub fn shot_source(own: &str, lender: &str) -> String {
    let has_own = variations(&format!("res://assets/audio/weapons/{own}/snap"), 1.0).is_some();
    if has_own || lender.is_empty() {
        own.to_owned()
    } else {
        lender.to_owned()
    }
}

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

/// A positional player under `node` on the Effects bus, for the sounds
/// [`play_on`] gives it.
pub fn attach_3d(
    node: &mut Gd<godot::classes::Node3D>,
    name: &str,
    unit_size: f32,
    max_distance: f32,
) -> Gd<godot::classes::AudioStreamPlayer3D> {
    let mut player = godot::classes::AudioStreamPlayer3D::new_alloc();
    player.set_name(name);
    player.set_bus("Effects");
    player.set_unit_size(unit_size);
    player.set_max_distance(max_distance);
    player.set_max_polyphony(2);
    node.add_child(&player);
    player
}

/// Plays one take of `<base>` (see [`variations`]) on `player`, at
/// `volume_db`; nothing when the files are missing.
pub fn play_on(player: &mut Gd<godot::classes::AudioStreamPlayer3D>, base: &str, volume_db: f32) {
    if let Some(stream) = variations(base, 1.02) {
        player.set_stream(&stream);
        player.set_volume_db(volume_db);
        player.play();
    }
}
