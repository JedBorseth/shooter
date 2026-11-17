//! Which build this is, for the menus and the dedicated server's log. The
//! values come from `build.rs`.

use godot::prelude::*;
use shooter_sim::net::protocol::PROTOCOL_VERSION;

/// A release tag such as `v0.3.0`, or the short commit hash that also names
/// the CI build zips; `dev` outside a git checkout.
pub const BUILD: &str = env!("SHOOTER_BUILD");
/// The commit's date, `YYYY-MM-DD`, or empty when unknown.
pub const BUILD_DATE: &str = env!("SHOOTER_BUILD_DATE");

/// The build and its date, as menus show it: `abc1234 (2026-11-16)`.
pub fn label() -> String {
    if BUILD_DATE.is_empty() {
        BUILD.to_owned()
    } else {
        format!("{BUILD} ({BUILD_DATE})")
    }
}

/// Static build facts for GDScript: `GameBuild.label()`.
#[derive(GodotClass)]
#[class(no_init, base = Object)]
pub struct GameBuild;

#[godot_api]
impl GameBuild {
    /// The tag or short commit hash.
    #[func]
    fn build() -> GString {
        BUILD.into()
    }

    /// The build and its commit date, such as `abc1234 (2026-11-16)`.
    #[func]
    fn label() -> GString {
        label().as_str().into()
    }

    /// The network protocol version this build speaks.
    #[func]
    fn protocol() -> i32 {
        i32::from(PROTOCOL_VERSION)
    }
}
