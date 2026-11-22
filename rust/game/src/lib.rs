//! GDExtension entry point. Godot classes defined here wrap the engine-free
//! simulation in `shooter-sim`.

mod acoustics;
mod aim_assist;
mod ambience;
mod ambience_areas;
mod audio;
mod body;
mod build_info;
mod bullets;
mod camo_look;
mod character;
mod convert;
mod effects;
mod footsteps;
mod forge;
mod grenade_hold;
mod grenade_view;
mod grenades;
mod gunfire;
mod hitbox_rig;
mod hound;
mod knife;
mod ladder;
mod mystery_crate;
mod net;
mod penetration;
mod pickup;
mod player;
mod player_sounds;
mod profile;
mod progression;
mod sentry;
mod sights;
mod spring;
mod surfaces;
mod target;
mod viewmodel;
mod weapon_clips;
mod weapon_data;
mod weapon_foley;
mod zombie;
mod zombie_director;

/// The only unsafe code in the crate: godot-rust requires the entry point to be
/// an `unsafe impl`, since Godot calls into it through the C ABI.
#[allow(unsafe_code)]
mod entry {
    use godot::init::InitStage;
    use godot::prelude::*;

    struct ShooterExtension;

    #[gdextension]
    unsafe impl ExtensionLibrary for ShooterExtension {
        fn on_stage_init(stage: InitStage) {
            // Weapons must be loaded before any scene creates player state.
            if stage == InitStage::MainLoop {
                crate::weapon_data::load(true);
            }
        }

        fn on_main_loop_frame() {
            crate::weapon_data::poll();
        }

        fn on_stage_deinit(stage: InitStage) {
            if matches!(stage, InitStage::MainLoop | InitStage::Scene) {
                crate::viewmodel::clear_cache();
                crate::sights::clear_cache();
                crate::character::palette::clear_cache();
            }
        }
    }
}
