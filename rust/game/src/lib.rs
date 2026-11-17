//! GDExtension entry point. Godot classes defined here wrap the engine-free
//! simulation in `shooter-sim`.

mod acoustics;
mod aim_assist;
mod ambience;
mod audio;
mod body;
mod character;
mod convert;
mod footsteps;
mod gunfire;
mod hitbox_rig;
mod net;
mod penetration;
mod pickup;
mod player;
mod sentry;
mod sights;
mod spring;
mod surfaces;
mod target;
mod viewmodel;
mod weapon_data;
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
    }
}
