//! Going down and getting back up: Black Ops' last stand.
//!
//! A player whose health would run out in Zombies goes down instead of
//! dying. Downed, they crawl, can only fire the sidearm, cannot be hurt, and
//! zombies ignore them. A teammate who holds the use button beside them for
//! [`REVIVE_SECONDS`] brings them back up with their weapons; otherwise they
//! bleed out after [`BLEED_OUT_SECONDS`] and watch until the next round.
//! The game ends when every player is down or dead, so a lone player going
//! down ends it (Black Ops' solo game without Quick Revive).
//!
//! Numbers from Black Ops III's `_zm_laststand.gsc`, `_zm.gsc`, and
//! `_zm_score.gsc`.

use crate::weapon::CombatState;

/// Seconds a downed player lasts before bleeding out
/// (`player_lastStandBleedoutTime`). The clock stops while they are being
/// revived.
pub const BLEED_OUT_SECONDS: f32 = 45.0;
/// Seconds a teammate holds the use button to revive (`revive_get_revive_time`).
pub const REVIVE_SECONDS: f32 = 3.0;
/// How close (metres) a teammate must be to revive: the revive trigger's
/// 75-unit radius (`revive_trigger_radius`).
pub const REVIVE_RADIUS: f32 = 1.9;
/// Seconds zombies keep ignoring a player just revived.
pub const REVIVED_IGNORE_SECONDS: f32 = 2.0;
/// Share of a player's points lost on going down (`penalty_downed`), which
/// whoever revives them earns.
pub const DOWN_PENALTY: f32 = 0.05;
/// Share of their points every other player loses when one bleeds out
/// (`penalty_no_revive`).
pub const NO_REVIVE_PENALTY: f32 = 0.10;
/// A downed player crawls at this share of the crouch speed (2.6 m/s), about
/// 1 m/s. Black Ops crawls prone; this game has no prone yet, so the body
/// stays crouched.
pub const CRAWL_SPEED_SCALE: f32 = 0.4;
/// Health a downed player is held at (Black Ops sets 1): the screen stays
/// red, and nothing regenerates.
pub const DOWNED_HEALTH: f32 = 1.0;
/// Magazines of reserve the last-stand pistol comes with (`doubleclip`).
pub const PISTOL_RESERVE_MAGAZINES: u16 = 2;

/// Points rounded up to the next ten, as Black Ops rounds penalties
/// (`round_up_to_ten`).
pub fn round_up_to_ten(points: u32) -> u32 {
    points.div_ceil(10) * 10
}

/// Points a player with `points` loses on going down.
pub fn down_penalty(points: u32) -> u32 {
    round_up_to_ten((points as f32 * DOWN_PENALTY) as u32).min(points)
}

/// Points a player with `points` loses when a teammate bleeds out.
pub fn no_revive_penalty(points: u32) -> u32 {
    round_up_to_ten((points as f32 * NO_REVIVE_PENALTY) as u32).min(points)
}

/// What a downed player holds: the starting sidearm, loaded, with two
/// magazines in reserve, and no grenades.
pub fn pistol() -> CombatState {
    let mut combat = super::start_combat().with_lethals(0);
    let active = usize::from(combat.active);
    let slot = &mut combat.slots[active];
    slot.reserve = slot.magazine * PISTOL_RESERVE_MAGAZINES;
    combat
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn penalties_round_up_to_ten() {
        assert_eq!(down_penalty(500), 30, "25 rounds up to 30");
        assert_eq!(down_penalty(2000), 100);
        assert_eq!(down_penalty(0), 0);
        assert_eq!(down_penalty(5), 0, "5% of 5 is nothing");
        assert_eq!(no_revive_penalty(1234), 130);
        assert_eq!(no_revive_penalty(20), 10, "2 rounds up to 10");
    }

    #[test]
    fn the_last_stand_pistol_has_two_magazines_spare_and_no_grenades() {
        let combat = pistol();
        let weapon = combat.weapon();
        assert_eq!(weapon.id.def().slot, crate::weapon::Slot::Secondary);
        assert_eq!(weapon.reserve, weapon.magazine * 2);
        assert_eq!(combat.lethals, 0);
        assert_eq!(combat.tacticals, 0);
    }
}
