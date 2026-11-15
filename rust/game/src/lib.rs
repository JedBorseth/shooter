//! GDExtension entry point. Godot classes defined here wrap the engine-free
//! simulation in `shooter-sim`.

mod aim_assist;
mod convert;
mod player;
mod target;
mod viewmodel;

/// The only unsafe code in the crate: godot-rust requires the entry point to be
/// an `unsafe impl`, since Godot calls into it through the C ABI.
#[allow(unsafe_code)]
mod entry {
    use godot::prelude::*;

    struct ShooterExtension;

    #[gdextension]
    unsafe impl ExtensionLibrary for ShooterExtension {}
}
