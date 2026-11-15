//! Stateless, deterministic randomness.
//!
//! Spread and recoil jitter must come out the same on the client (prediction)
//! and the server (authority). Instead of a stateful generator, values are
//! hashed from inputs both sides already agree on, such as a shot counter.

/// Mixes two words into a well-distributed 32-bit hash (lowbias32 finaliser).
pub fn hash(a: u32, b: u32) -> u32 {
    let mut x = a.wrapping_mul(0x9E37_79B1) ^ b.wrapping_mul(0x85EB_CA77).rotate_left(15);
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^= x >> 16;
    x
}

/// Uniform float in `[0, 1)`.
pub fn unit(a: u32, b: u32) -> f32 {
    (hash(a, b) >> 8) as f32 / (1u32 << 24) as f32
}

/// Uniform float in `[-1, 1)`.
pub fn signed(a: u32, b: u32) -> f32 {
    unit(a, b) * 2.0 - 1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_is_in_range_and_spread_out() {
        let mut sum = 0.0;
        for i in 0..10_000 {
            let v = unit(7, i);
            assert!((0.0..1.0).contains(&v));
            sum += v;
        }
        let mean = sum / 10_000.0;
        assert!((mean - 0.5).abs() < 0.02, "mean {mean}");
    }

    #[test]
    fn different_streams_differ() {
        assert_ne!(hash(1, 2), hash(2, 1));
        assert_ne!(hash(0, 0), hash(0, 1));
    }
}
