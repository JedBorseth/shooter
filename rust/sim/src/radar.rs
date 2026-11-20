//! Enemies on the minimap: what each player's radar shows.
//!
//! Black Ops 2's rule, kept by every Call of Duty since: firing a weapon
//! without a suppressor puts the shooter on every enemy's minimap as a red
//! dot where they fired, for a few seconds; a suppressed weapon keeps them
//! off it. The dot stays where the shot was fired, it does not follow the
//! shooter, and a new shot moves it. Knives and grenades show nothing.
//!
//! The server decides who is revealed ([`Radar`]) and sends each player only
//! the dots their own radar shows (`protocol::RadarView`), so a client never
//! learns where an enemy is from the radar unless the game shows it.

use glam::Vec3;

use crate::TICK_RATE;
use crate::net::Tick;
use crate::weapon::{Attachment, WeaponId};

/// Seconds a red dot from firing stays up. Call of Duty gives no number;
/// players measure "a few seconds" (an estimate, between Modern Warfare 2's
/// and Black Ops 2's feel). It fades over its last second on the client.
pub const FIRE_SECONDS: f32 = 3.0;

/// Most dots a snapshot carries.
pub const MAX_BLIPS: usize = 32;

/// What put an enemy on the radar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BlipKind {
    /// They fired an unsuppressed weapon.
    #[default]
    Fire,
}

impl BlipKind {
    /// Ticks the dot lasts.
    pub fn life(self) -> Tick {
        match self {
            BlipKind::Fire => (FIRE_SECONDS * TICK_RATE as f32).round() as Tick,
        }
    }
}

/// An enemy shown on a radar: where, since when, and why.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Blip {
    pub id: u8,
    pub at: Vec3,
    /// The tick it was revealed.
    pub tick: Tick,
    pub kind: BlipKind,
}

/// Whether firing `weapon` shows the shooter on enemy minimaps.
pub fn fire_reveals(weapon: WeaponId) -> bool {
    !weapon.def().fitted.has(Attachment::Suppressor)
}

/// The server's radar: the newest unsuppressed shot of each player.
#[derive(Clone, Debug, Default)]
pub struct Radar {
    fire: Vec<Blip>,
}

impl Radar {
    /// Player `id` fired `weapon` from `at` on `tick`.
    pub fn fired(&mut self, id: u8, at: Vec3, weapon: WeaponId, tick: Tick) {
        if !fire_reveals(weapon) {
            return;
        }
        let blip = Blip {
            id,
            at,
            tick,
            kind: BlipKind::Fire,
        };
        match self.fire.iter_mut().find(|b| b.id == id) {
            Some(old) => *old = blip,
            None => self.fire.push(blip),
        }
    }

    /// Player `id` died or left: their dot goes.
    pub fn forget(&mut self, id: u8) {
        self.fire.retain(|b| b.id != id);
    }

    /// Drops dots that have run out by `now`.
    pub fn expire(&mut self, now: Tick) {
        self.fire
            .retain(|b| now.saturating_sub(b.tick) < b.kind.life());
    }

    /// The dots a player sees at `now`: those of players `hostile` says are
    /// their enemies.
    pub fn blips_for(&self, now: Tick, hostile: impl Fn(u8) -> bool) -> Vec<Blip> {
        self.fire
            .iter()
            .filter(|b| now.saturating_sub(b.tick) < b.kind.life() && hostile(b.id))
            .take(MAX_BLIPS)
            .copied()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m4() -> WeaponId {
        WeaponId::expect("m4")
    }

    #[test]
    fn unsuppressed_fire_shows_where_the_shot_was_fired() {
        let mut radar = Radar::default();
        radar.fired(3, Vec3::new(1.0, 0.0, 2.0), m4(), 100);
        let blips = radar.blips_for(110, |_| true);
        assert_eq!(blips.len(), 1);
        assert_eq!(blips[0].id, 3);
        assert_eq!(blips[0].at, Vec3::new(1.0, 0.0, 2.0));
        // A new shot moves the dot rather than adding one.
        radar.fired(3, Vec3::new(5.0, 0.0, 2.0), m4(), 120);
        let blips = radar.blips_for(130, |_| true);
        assert_eq!(blips.len(), 1);
        assert_eq!(blips[0].at.x, 5.0);
        assert_eq!(blips[0].tick, 120);
    }

    #[test]
    fn a_suppressor_keeps_the_shooter_off_the_radar() {
        let mut radar = Radar::default();
        radar.fired(3, Vec3::ZERO, WeaponId::expect("m4+suppressor"), 100);
        assert!(radar.blips_for(101, |_| true).is_empty());
        assert!(!fire_reveals(WeaponId::expect("m4+suppressor")));
        assert!(fire_reveals(WeaponId::expect("m4+grip")));
    }

    #[test]
    fn dots_show_only_to_enemies_and_run_out() {
        let mut radar = Radar::default();
        radar.fired(3, Vec3::ZERO, m4(), 100);
        radar.fired(4, Vec3::ZERO, m4(), 100);
        let enemies = radar.blips_for(101, |id| id == 4);
        assert_eq!(enemies.len(), 1);
        assert_eq!(enemies[0].id, 4);
        let life = BlipKind::Fire.life();
        assert_eq!(life, 360);
        assert_eq!(radar.blips_for(100 + life - 1, |_| true).len(), 2);
        assert!(radar.blips_for(100 + life, |_| true).is_empty());
        radar.expire(100 + life);
        assert!(radar.fire.is_empty());
    }

    #[test]
    fn a_dead_players_dot_goes() {
        let mut radar = Radar::default();
        radar.fired(3, Vec3::ZERO, m4(), 100);
        radar.forget(3);
        assert!(radar.blips_for(101, |_| true).is_empty());
    }
}
