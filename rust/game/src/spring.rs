//! Small motion helpers shared by the camera and the viewmodel.

use std::ops::{Add, AddAssign, Mul, Neg, Sub};

use godot::prelude::Vector3;

/// A damped spring, integrated semi-implicitly for stability at any frame rate.
/// `T` is `f32` for a single value or `Vector3` for three independent axes.
#[derive(Clone, Copy, Debug, Default)]
pub struct Spring<T = Vector3> {
    pub value: T,
    pub velocity: T,
}

impl<T> Spring<T>
where
    T: Copy
        + Add<Output = T>
        + AddAssign
        + Sub<Output = T>
        + Mul<f32, Output = T>
        + Neg<Output = T>,
{
    pub fn step(&mut self, stiffness: f32, damping: f32, dt: f32) {
        // Sub-step at about 1 kHz: stable on long frames, and close enough to the
        // exact solution that `impulse_for_peak` lands within about 5% of its target.
        let steps = (dt * 1000.0).ceil().max(1.0);
        let h = dt / steps;
        for _ in 0..steps as u32 {
            self.velocity += (-self.value * stiffness - self.velocity * damping) * h;
            self.value += self.velocity * h;
        }
    }
}

/// The velocity impulse that makes an underdamped spring at rest overshoot to
/// `peak` before settling. Lets kicks be tuned as angles instead of velocities.
pub fn impulse_for_peak(peak: f32, stiffness: f32, damping: f32) -> f32 {
    let omega = stiffness.sqrt();
    let zeta = (damping / (2.0 * omega)).min(0.999);
    let root = (1.0 - zeta * zeta).sqrt();
    let peak_per_velocity = (-zeta / root * root.atan2(zeta)).exp() / omega;
    peak / peak_per_velocity
}

/// Frame-rate independent exponential smoothing factor.
pub fn smoothing(rate: f32, dt: f32) -> f32 {
    1.0 - (-rate * dt).exp()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn impulse_reaches_requested_peak() {
        // The camera punch spring in `player.rs`.
        let (stiffness, damping) = (900.0, 48.0);
        let mut spring = Spring::<f32> {
            value: 0.0,
            velocity: impulse_for_peak(0.01, stiffness, damping),
        };
        let mut peak: f32 = 0.0;
        // Step at 144 Hz render frames, as on a typical high-refresh display.
        for _ in 0..144 {
            spring.step(stiffness, damping, 1.0 / 144.0);
            peak = peak.max(spring.value);
        }
        assert!((peak - 0.01).abs() < 0.0005, "peak {peak}");
        assert!(spring.value.abs() < 0.0005, "settles: {}", spring.value);
    }

    #[test]
    fn vector_spring_moves_each_axis() {
        let mut spring = Spring {
            value: Vector3::ZERO,
            velocity: Vector3::new(1.0, 0.0, -1.0),
        };
        spring.step(260.0, 22.0, 1.0 / 60.0);
        assert!(spring.value.x > 0.0 && spring.value.z < 0.0);
        assert_eq!(spring.value.y, 0.0);
    }
}
