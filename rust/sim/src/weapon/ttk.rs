//! Time to kill: how long each weapon takes to down a full-health player at a
//! range, for balance tests and the tables in `docs/weapons.md`.
//!
//! Every shot is assumed to land on the same zone, and the trigger is pulled
//! as fast as the weapon allows (bursts and bolt or pump cycles included).
//! Shotgun pellets hit in proportion to how much of a torso-sized disc their
//! cone covers at that range.

use super::def::{FireMode, WeaponClass, WeaponDef};
use crate::hitbox::HitZone;

/// What a weapon class's numbers must stay within, so each new weapon keeps
/// its class's shape (Black Ops 2's: rifles kill in 200 to 400 ms at every
/// range, SMGs faster up close and slower past 20 m, LMGs steady to long
/// range and slow to handle, shotguns one or two shots up close and nothing
/// at range, marksman rifles two shots anywhere, snipers one, pistols a
/// sidearm's few hits). Checked against every weapon file by
/// `every_weapon_lands_in_its_class_band`, and listed in `docs/weapons.md`.
#[derive(Clone, Copy, Debug)]
pub struct Band {
    /// Chest time to kill on a 100-health player at each range: (metres,
    /// fastest, slowest) in seconds. `f32::INFINITY` as the slowest lets the
    /// weapon fail to kill there.
    pub ttk: &'static [(f32, f32, f32)],
    /// Aim-down-sights time, seconds.
    pub ads: (f32, f32),
    /// Sprint-out time, seconds.
    pub sprint_out: (f32, f32),
    /// Movement speed scale while it is in hand.
    pub move_scale: (f32, f32),
}

/// The band `class` is held to, or `None` for one with none (test weapons).
pub fn band(class: WeaponClass) -> Option<Band> {
    const INF: f32 = f32::INFINITY;
    Some(match class {
        WeaponClass::AssaultRifle => Band {
            ttk: &[
                (5.0, 0.12, 0.32),
                (10.0, 0.18, 0.40),
                (20.0, 0.18, 0.40),
                (30.0, 0.18, 0.40),
                (40.0, 0.18, 0.42),
                (50.0, 0.22, 0.45),
                (70.0, 0.22, 0.45),
            ],
            ads: (0.22, 0.32),
            sprint_out: (0.20, 0.30),
            move_scale: (0.93, 1.0),
        },
        WeaponClass::Smg => Band {
            ttk: &[
                (5.0, 0.12, 0.22),
                (10.0, 0.12, 0.25),
                (15.0, 0.12, 0.30),
                (20.0, 0.15, 0.35),
                (30.0, 0.30, 0.60),
                (50.0, 0.30, 0.80),
            ],
            ads: (0.15, 0.22),
            sprint_out: (0.12, 0.20),
            move_scale: (1.0, 1.05),
        },
        WeaponClass::Lmg => Band {
            ttk: &[
                (5.0, 0.22, 0.40),
                (20.0, 0.22, 0.40),
                (40.0, 0.22, 0.40),
                (50.0, 0.25, 0.45),
                (70.0, 0.25, 0.45),
            ],
            ads: (0.36, 0.50),
            sprint_out: (0.32, 0.45),
            move_scale: (0.82, 0.92),
        },
        WeaponClass::Shotgun => Band {
            ttk: &[
                (3.0, 0.0, 0.15),
                (5.0, 0.0, 0.15),
                (10.0, 0.0, 0.60),
                (20.0, 0.5, INF),
            ],
            ads: (0.20, 0.30),
            sprint_out: (0.18, 0.26),
            move_scale: (0.95, 1.0),
        },
        WeaponClass::Marksman => Band {
            // Never one shot to the chest (that is the sniper's).
            ttk: &[
                (5.0, 0.05, 0.30),
                (20.0, 0.05, 0.30),
                (50.0, 0.05, 0.40),
                (70.0, 0.05, 0.40),
            ],
            ads: (0.30, 0.40),
            sprint_out: (0.26, 0.36),
            move_scale: (0.92, 0.98),
        },
        WeaponClass::Sniper => Band {
            ttk: &[
                (5.0, 0.0, 0.0),
                (20.0, 0.0, 0.0),
                (50.0, 0.0, 0.0),
                (100.0, 0.0, 0.0),
            ],
            ads: (0.38, 0.50),
            sprint_out: (0.32, 0.42),
            move_scale: (0.88, 0.95),
        },
        WeaponClass::Pistol => Band {
            ttk: &[(5.0, 0.08, 0.35), (15.0, 0.10, 0.45), (30.0, 0.25, 0.90)],
            ads: (0.12, 0.20),
            sprint_out: (0.08, 0.15),
            move_scale: (1.0, 1.1),
        },
        // Explosives: no bullet time to kill; handling only.
        WeaponClass::Launcher => Band {
            ttk: &[],
            ads: (0.35, 0.60),
            sprint_out: (0.30, 0.50),
            move_scale: (0.85, 0.95),
        },
        WeaponClass::Special => return None,
    })
}

/// Every way `def` falls outside its class's band, as readable lines.
pub fn band_misses(def: &WeaponDef) -> Vec<String> {
    let Some(band) = band(def.class) else {
        return Vec::new();
    };
    let mut misses = Vec::new();
    for &(range, fastest, slowest) in band.ttk {
        let t = ttk(def, range, HitZone::Chest, 100.0).unwrap_or(f32::INFINITY);
        // A millisecond of tolerance for rounding in the fire interval.
        if t < fastest - 1e-3 || t > slowest + 1e-3 {
            misses.push(format!(
                "{} at {range} m kills in {:.0} ms, outside {:.0}-{:.0} ms",
                def.key,
                t * 1000.0,
                fastest * 1000.0,
                slowest * 1000.0
            ));
        }
    }
    let mut check = |what: &str, value: f32, (low, high): (f32, f32)| {
        if value < low - 1e-4 || value > high + 1e-4 {
            misses.push(format!("{} {what} {value} outside {low}-{high}", def.key));
        }
    };
    check("ads_time", def.ads_time, band.ads);
    check("sprint_out_time", def.sprint_out_time, band.sprint_out);
    check("move_scale", def.move_scale, band.move_scale);
    misses
}

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
        // The CQ-10 out-kills the PDW-9 to about 12 m and loses from 13 to
        // 20 m; past that its kick, not its damage, loses it the fight.
        for d in [5.0, 10.0] {
            assert!(at("cq10", d) < at("pdw9", d), "{d} m");
        }
        for d in [15.0, 20.0] {
            assert!(at("cq10", d) > at("pdw9", d), "{d} m");
        }
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
        // The battle rifle kills in three to about 42 m: slower than the AK
        // point-blank, faster than either rifle from 10 m out.
        assert_eq!(
            shots_to_kill(def("kr12"), 40.0, HitZone::Chest, 100.0),
            Some(3)
        );
        assert!(at("ak47", 5.0) < at("kr12", 5.0));
        for d in [10.0, 20.0, 40.0] {
            assert!(at("kr12", d) < at("m4", d) && at("kr12", d) < at("ak47", d));
        }
        // The burst pistol kills in one burst to about 11 m, and out-kills
        // the M1911 everywhere it reaches but is a sidearm past 20 m.
        assert_eq!(
            shots_to_kill(def("bp9"), 9.0, HitZone::Chest, 100.0),
            Some(3)
        );
        assert!(at("bp9", 5.0) < at("m1911", 5.0));
        assert!(at("bp9", 30.0) > at("m4", 30.0));
        // The semi-automatic shotgun one-shots to about 7.5 m, and its
        // second shell lands 133 ms later out to about 10 m, where the pump
        // takes 550 ms.
        assert_eq!(at("sg12", 7.0), 0.0);
        assert!(at("sg12", 9.0) > 0.0);
        assert!(at("sg12", 10.0) < 0.15 && at("sg12", 10.0) < at("m870", 10.0));
        // The marksman rifle takes two to the chest everywhere, never one,
        // and a headshot kills outright up close.
        for d in [5.0, 30.0, 70.0, 100.0] {
            assert_eq!(
                shots_to_kill(def("dm7"), d, HitZone::Chest, 100.0),
                Some(2),
                "{d} m"
            );
        }
        assert_eq!(
            shots_to_kill(def("dm7"), 30.0, HitZone::Head, 100.0),
            Some(1)
        );
        // The sniper one-shots the chest anywhere.
        assert_eq!(at("m40", 100.0), 0.0);
    }

    /// The headless balance check: every weapon file's time to kill at each
    /// range and its handling land in its class's band.
    #[test]
    fn every_weapon_lands_in_its_class_band() {
        let mut misses = Vec::new();
        for id in WeaponId::all() {
            let d = id.def();
            if d.plain() {
                misses.extend(band_misses(d));
            }
        }
        assert!(misses.is_empty(), "{}", misses.join("\n"));
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
