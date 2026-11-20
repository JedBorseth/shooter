//! Time to kill: how long each weapon takes to down a full-health player at a
//! range, for balance tests and the tables in `docs/weapons.md`.
//!
//! Every shot is assumed to land on the same zone, and the trigger is pulled
//! as fast as the weapon allows (bursts and bolt or pump cycles included).
//! Shotgun pellets hit in proportion to how much of a torso-sized disc their
//! cone covers at that range.

use super::def::{FireMode, WeaponDef};
use crate::hitbox::HitZone;

/// Radius of the disc a torso presents to a shotgun's pellet cone, in metres.
const TORSO_RADIUS: f32 = 0.25;

/// Damage one trigger pull deals at `distance` to `zone`.
pub fn shot_damage(def: &WeaponDef, distance: f32, zone: HitZone) -> f32 {
    // Rounds stop at the weapon's range (a shotgun's pellets at 60 m).
    if distance > def.range {
        return 0.0;
    }
    let per_pellet = def.damage_at(distance) * def.zone_scale(zone);
    if def.pellets <= 1 {
        return per_pellet;
    }
    let cone = distance * def.pellet_spread.to_radians().tan();
    let landed = if cone <= TORSO_RADIUS {
        1.0
    } else {
        (TORSO_RADIUS / cone).powi(2)
    };
    per_pellet * f32::from(def.pellets) * landed
}

/// Trigger pulls needed to deal `health`, or `None` if the weapon cannot.
pub fn shots_to_kill(def: &WeaponDef, distance: f32, zone: HitZone, health: f32) -> Option<u32> {
    let damage = shot_damage(def, distance, zone);
    if damage <= 0.0 {
        return None;
    }
    // A hair of tolerance so 4 x 25 counts as 100.
    Some(((health - 1e-3) / damage).ceil().max(1.0) as u32)
}

/// Seconds between the first shot and the `n`th (the first shot is at 0).
pub fn time_for_shots(def: &WeaponDef, n: u32) -> f32 {
    if n <= 1 {
        return 0.0;
    }
    let gaps = n - 1;
    let interval = def.fire_interval().max(def.cycle_time);
    match def.fire_mode {
        FireMode::Burst { rounds, delay } => {
            let rounds = u32::from(rounds.max(1));
            // Each completed burst adds its delay before the next one starts.
            let pauses = gaps / rounds;
            gaps as f32 * interval + pauses as f32 * delay
        }
        FireMode::Auto | FireMode::Semi => gaps as f32 * interval,
    }
}

/// Time to kill in seconds, or `None` if the weapon cannot kill at that range.
pub fn ttk(def: &WeaponDef, distance: f32, zone: HitZone, health: f32) -> Option<f32> {
    shots_to_kill(def, distance, zone, health).map(|n| time_for_shots(def, n))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::weapon::WeaponId;

    fn def(key: &str) -> &'static WeaponDef {
        WeaponId::expect(key).def()
    }

    #[test]
    fn auto_ttk_counts_gaps_between_shots() {
        let m4 = def("m4");
        let n = shots_to_kill(m4, 5.0, HitZone::Chest, 100.0).unwrap();
        let expected = (n - 1) as f32 * m4.fire_interval();
        assert!((ttk(m4, 5.0, HitZone::Chest, 100.0).unwrap() - expected).abs() < 1e-5);
    }

    #[test]
    fn bursts_pay_their_delay() {
        let br = def("br3");
        let FireMode::Burst { rounds, delay } = br.fire_mode else {
            panic!("the BR-3 fires bursts");
        };
        let n = u32::from(rounds) + 1;
        let t = time_for_shots(br, n);
        assert!((t - (rounds as f32 * br.fire_interval() + delay)).abs() < 1e-5);
    }

    #[test]
    fn shotgun_pellets_thin_out_with_range() {
        let shotgun = def("m870");
        let near = shot_damage(shotgun, 3.0, HitZone::Chest);
        let far = shot_damage(shotgun, 20.0, HitZone::Chest);
        assert!(near > far * 4.0, "near {near}, far {far}");
    }

    /// Each class keeps its shape against the reference band.
    #[test]
    fn classes_keep_their_identity() {
        let at = |key: &str, d: f32| ttk(def(key), d, HitZone::Chest, 100.0).unwrap();
        // Rifles: 200-400 ms everywhere. The AK wins up close, the M4 at
        // mid and long range.
        for key in ["m4", "ak47"] {
            for d in [5.0, 20.0, 50.0] {
                let t = at(key, d);
                assert!((0.18..=0.41).contains(&t), "{key} at {d} m: {t}");
            }
        }
        assert!(at("ak47", 8.0) < at("m4", 8.0));
        assert!(at("m4", 20.0) < at("ak47", 20.0));
        assert!(at("m4", 50.0) < at("ak47", 50.0));
        // The burst rifle kills in one burst only point-blank...
        assert!(at("br3", 5.0) < 0.16);
        // ...and nothing beats the SMG from there to 15 m.
        for d in [7.0, 10.0, 15.0] {
            for key in ["br3", "ak47", "m4", "lsw6"] {
                assert!(at("pdw9", d) < at(key, d), "{key} at {d} m");
            }
        }
        assert!(at("br3", 25.0) > 0.3);
        // The LMG kills in four to about 49 m, five beyond.
        assert_eq!(at("lsw6", 5.0), at("lsw6", 45.0));
        assert!(at("lsw6", 55.0) > at("lsw6", 45.0));
        // The SMG wins to 15 m and loses at range.
        assert!(at("pdw9", 5.0) < at("m4", 5.0));
        assert!(at("pdw9", 15.0) < at("m4", 15.0));
        assert!(at("pdw9", 30.0) > at("m4", 30.0));
        // The shotgun one-shots to about 9 m and two-shots to about 12 m.
        assert_eq!(at("m870", 9.0), 0.0);
        assert!(at("m870", 10.0) > 0.0);
        assert_eq!(
            shots_to_kill(def("m870"), 12.0, HitZone::Chest, 100.0),
            Some(2)
        );
        // The sniper one-shots the chest anywhere.
        assert_eq!(at("m40", 100.0), 0.0);
    }

    /// Prints the table (`cargo test -p shooter-sim ttk_table -- --nocapture`).
    #[test]
    fn ttk_table() {
        let ranges = [5.0, 10.0, 15.0, 20.0, 30.0, 40.0, 50.0, 70.0];
        let mut header = String::from("| weapon |");
        for r in ranges {
            header += &format!(" {r} m |");
        }
        println!("{header}");
        for id in WeaponId::all() {
            let d = id.def();
            if !d.plain() || d.beam {
                continue;
            }
            let mut row = format!("| {} |", d.key);
            for r in ranges {
                let n = shots_to_kill(d, r, HitZone::Chest, 100.0);
                let t = ttk(d, r, HitZone::Chest, 100.0);
                match (n, t) {
                    (Some(n), Some(t)) => row += &format!(" {:.0} ms ({n}) |", t * 1000.0),
                    _ => row += " - |",
                }
            }
            println!("{row}");
        }
    }
}
