//! Transcendental functions that give the same bits on every platform.
//!
//! `f32::sin`, `exp`, `atan2`, and friends call the platform's math library,
//! and those differ in the last bits between Linux, Windows, and macOS. A
//! Windows client predicting against a Linux server would then drift and
//! mispredict. Everything the client and server both simulate uses these
//! instead: `libm` is a pure-Rust port of musl's math library, so results are
//! identical everywhere. (`sqrt` and basic arithmetic are exact under IEEE 754
//! and need no wrapper.)

pub fn sin(x: f32) -> f32 {
    libm::sinf(x)
}

pub fn cos(x: f32) -> f32 {
    libm::cosf(x)
}

pub fn sin_cos(x: f32) -> (f32, f32) {
    libm::sincosf(x)
}

pub fn exp(x: f32) -> f32 {
    libm::expf(x)
}

pub fn powf(x: f32, y: f32) -> f32 {
    libm::powf(x, y)
}

/// Four-quadrant arctangent of `y / x`.
pub fn atan2(y: f32, x: f32) -> f32 {
    libm::atan2f(y, x)
}
