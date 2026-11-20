//! `ZombieDirector`: a Zombies map's game as this client knows it. The
//! dedicated server runs the game (`net::horde`, rules in
//! `shooter_sim::zombies`); `NetClient` passes what it says on through
//! `HordeView` (`net_round`, `net_points`, `net_drop`, ...), and the director
//! holds it for the HUD (`scripts/zombies_hud.gd`) and emits the round,
//! points, and power-up signals on the changes. `NetClient` draws the zombies
//! as its children.
//!
//! It is also this player's wallet (group `zombies_wallet`): wall weapons,
//! doors, and machines ask it for the points, and only say whether a
//! purchase is possible; the server takes the points.

use godot::classes::{INode3D, Node3D, PackedScene};
use godot::prelude::*;
use shooter_sim::zombies::{self, Phase, PowerUp, Rounds};

use crate::zombie::Zombie;

#[derive(GodotClass)]
#[class(init, base = Node3D)]
pub struct ZombieDirector {
    base: Base<Node3D>,

    /// The zombie clients draw for the server's (`net::horde_view`).
    #[export]
    zombie_scene: OnEditor<Gd<PackedScene>>,
    /// Dropped for a power-up; its script's `setup(kind)` and
    /// `set_time_left(seconds)` are called (`scripts/zombies_power_up.gd`).
    #[export]
    power_up_scene: Option<Gd<PackedScene>>,

    /// The round and its phase, as the server last said.
    rounds: Option<Rounds>,
    points: i64,
    kills: i64,
    headshots: i64,
    drops: Vec<Drop>,
    /// Engine seconds the break ends, and zombies left.
    net_break_ends: f64,
    net_remaining: u32,
    /// Engine seconds Insta-Kill, Double Points, and the Fire Sale end.
    net_insta_kill_ends: f64,
    net_double_points_ends: f64,
    net_fire_sale_ends: f64,
    /// This round is a hound round.
    net_hounds: bool,
    /// This player's re-packed weapons (keys) and their alternate ammo's
    /// name.
    net_alt_ammo: Vec<(String, &'static str)>,
    /// Engine seconds this player bleeds out, while down.
    net_bleeds_out: Option<f64>,
    /// A revive under way that involves this player: the other player's
    /// name ("" for getting back up by ourselves), whether we are the one
    /// reviving, the engine seconds it is done, and how long it takes in all.
    net_revive: Option<(String, bool, f64, f64)>,
    /// The teammate close enough to revive, if any.
    net_revive_prompt: String,
    /// The power is on (drink machines, the upgrade machine, traps).
    net_power: bool,
}

struct Drop {
    kind: PowerUp,
    node: Gd<Node3D>,
    left: f32,
    /// The server's id for it.
    id: u8,
}

#[godot_api]
impl INode3D for ZombieDirector {
    fn ready(&mut self) {
        self.base_mut().add_to_group("zombies_wallet");
    }

    fn physics_process(&mut self, delta: f64) {
        let _profile = crate::profile::scope("zombie director");
        // Tokens blink in their last seconds; the server removes them.
        for drop in &mut self.drops {
            drop.left -= delta as f32;
            drop.node.call("set_time_left", &[drop.left.to_variant()]);
        }
    }
}

#[godot_api]
impl ZombieDirector {
    /// A new round began.
    #[signal]
    fn round_started(round: i64);

    /// Every zombie of the round is dead.
    #[signal]
    fn round_cleared(round: i64);

    /// Points were earned: `points` this time, `total` now.
    #[signal]
    fn points_awarded(points: i64, total: i64);

    /// Every player is down: the game ended in round `round`.
    #[signal]
    fn game_over(round: i64);

    /// A kill dropped a power-up (`kind` is its key, such as "insta_kill").
    #[signal]
    fn power_up_dropped(kind: GString);

    /// A player picked up a power-up; `label` is what to announce.
    #[signal]
    fn power_up_collected(kind: GString, label: GString);

    /// Points were spent on something: `points` this time, `total` left.
    #[signal]
    fn points_spent(points: i64, total: i64);

    /// Whether this player has `amount` points for a purchase. The server
    /// takes them and says what is left.
    #[func]
    fn spend(&mut self, amount: i64) -> bool {
        amount >= 0 && self.points >= amount
    }

    /// The game is the server's (a client joined): forget any points held.
    #[func]
    pub fn set_networked(&mut self) {
        self.points = 0;
        self.kills = 0;
        self.headshots = 0;
    }

    /// Always: the dedicated server runs every Zombies game.
    #[func]
    pub fn is_networked(&self) -> bool {
        true
    }

    /// Seconds of Insta-Kill left.
    #[func]
    fn insta_kill_left(&self) -> f32 {
        (self.net_insta_kill_ends - now_seconds()).max(0.0) as f32
    }

    /// Seconds of Double Points left.
    #[func]
    fn double_points_left(&self) -> f32 {
        (self.net_double_points_ends - now_seconds()).max(0.0) as f32
    }

    /// Seconds of Fire Sale left.
    #[func]
    fn fire_sale_left(&self) -> f32 {
        (self.net_fire_sale_ends - now_seconds()).max(0.0) as f32
    }

    /// The name of the alternate ammo on this player's weapon `key` (an
    /// upgraded one), or "".
    #[func]
    fn alt_ammo_label(&self, key: GString) -> GString {
        let key = key.to_string();
        self.net_alt_ammo
            .iter()
            .find(|(k, _)| *k == key)
            .map_or(GString::new(), |(_, label)| GString::from(*label))
    }

    /// Whether this round is a hound round.
    #[func]
    fn hound_round(&self) -> bool {
        self.net_hounds
    }

    /// Gives points outright, as if the server had paid them (captures).
    #[func]
    fn add_points(&mut self, amount: i64) {
        let total = (self.points + amount.max(0)).clamp(0, i64::from(u32::MAX));
        self.net_points(total as u32, amount.max(0) as i32);
    }

    /// Whether the power is on.
    #[func]
    fn power_on(&self) -> bool {
        self.net_power
    }

    /// Last stand for the HUD: `downed` and `bleed_left` (seconds) while
    /// this player is down; `revive_left` (seconds, -1 for none) and
    /// `revive_seconds` (its whole length), `revive_name` (the other
    /// player), and `reviving` (we are the one holding) during a revive;
    /// and `prompt` (the teammate close enough to revive, or "").
    #[func]
    fn last_stand_info(&self) -> VarDictionary {
        let now = now_seconds();
        let mut d = VarDictionary::new();
        d.set("downed", self.net_bleeds_out.is_some());
        d.set(
            "bleed_left",
            self.net_bleeds_out.map_or(0.0, |end| (end - now).max(0.0)),
        );
        let (name, reviving, left, total) = match &self.net_revive {
            Some((name, reviving, end, total)) => {
                (name.clone(), *reviving, (end - now).max(0.0), *total)
            }
            None => (
                String::new(),
                false,
                -1.0,
                f64::from(shooter_sim::zombies::laststand::REVIVE_SECONDS),
            ),
        };
        d.set("revive_left", left);
        d.set("revive_seconds", total);
        d.set("revive_name", &GString::from(&name));
        d.set("reviving", reviving);
        d.set("prompt", &GString::from(&self.net_revive_prompt));
        d
    }

    #[func]
    fn round(&self) -> i64 {
        self.rounds.map(|r| i64::from(r.round)).unwrap_or(0)
    }

    /// True while zombies are coming (not in a break, not over).
    #[func]
    fn is_fighting(&self) -> bool {
        self.rounds.is_some_and(|r| r.phase == Phase::Fighting)
    }

    #[func]
    fn is_over(&self) -> bool {
        self.rounds.is_some_and(|r| r.phase == Phase::Over)
    }

    /// Seconds left before the next round, or 0 during one.
    #[func]
    fn break_left(&self) -> f32 {
        if !self.rounds.is_some_and(|r| r.phase == Phase::Break) {
            return 0.0;
        }
        (self.net_break_ends - now_seconds()).max(0.0) as f32
    }

    /// Zombies still to kill this round, spawned or not.
    #[func]
    fn remaining(&self) -> i64 {
        i64::from(self.net_remaining)
    }

    #[func]
    fn points(&self) -> i64 {
        self.points
    }

    #[func]
    fn kills(&self) -> i64 {
        self.kills
    }

    #[func]
    fn headshots(&self) -> i64 {
        self.headshots
    }

    /// The server's round. `phase` 0 is a break ending in `break_left`
    /// seconds, 1 the fight with `remaining` zombies left (`hounds`: a hound
    /// round), 2 the end. Emits the round signals on the changes (also how
    /// captures set the HUD up).
    #[func]
    pub fn net_round(
        &mut self,
        round: u32,
        phase: u8,
        break_left: f64,
        remaining: u32,
        hounds: bool,
    ) {
        self.net_hounds = hounds;
        let phase = match phase {
            0 => Phase::Break,
            1 => Phase::Fighting,
            _ => Phase::Over,
        };
        let before = self.rounds;
        let mut rounds = Rounds::new(0.0);
        rounds.round = round;
        rounds.phase = phase;
        self.rounds = Some(rounds);
        self.net_remaining = remaining;
        self.net_break_ends = now_seconds() + break_left.max(0.0);
        let changed = before.is_none_or(|b| b.round != round || b.phase != phase);
        if !changed {
            return;
        }
        match phase {
            Phase::Fighting => {
                self.signals().round_started().emit(i64::from(round));
            }
            Phase::Break => {
                if before.is_some_and(|b| b.phase == Phase::Fighting) {
                    let cleared = i64::from(round.saturating_sub(1));
                    self.signals().round_cleared().emit(cleared);
                }
            }
            Phase::Over => {
                self.signals().game_over().emit(i64::from(round));
            }
        }
    }

    /// The zombies drawn here, alive or dead (tests).
    #[func]
    pub fn zombies(&self) -> Array<Gd<Zombie>> {
        let mut out = Array::new();
        for child in self.base().get_children().iter_shared() {
            if let Ok(zombie) = child.try_cast::<Zombie>() {
                out.push(&zombie);
            }
        }
        out
    }
}

impl ZombieDirector {
    /// The zombie the client draws for each of the server's.
    pub fn zombie_scene(&self) -> Gd<PackedScene> {
        (*self.zombie_scene).clone()
    }

    /// This player's points, which just changed by `change`.
    pub fn net_points(&mut self, total: u32, change: i32) {
        self.points = i64::from(total);
        let total = self.points;
        if change > 0 {
            self.signals()
                .points_awarded()
                .emit(i64::from(change), total);
        } else if change < 0 {
            self.signals()
                .points_spent()
                .emit(i64::from(-change), total);
        }
    }

    /// A kill dropped power-up `id` of `kind` at `at`.
    pub fn net_drop(&mut self, id: u8, kind: PowerUp, at: Vector3) {
        let Some(node) = self
            .power_up_scene
            .as_ref()
            .and_then(|s| s.instantiate())
            .and_then(|n| n.try_cast::<Node3D>().ok())
        else {
            return;
        };
        let mut node = node;
        self.base_mut().add_child(&node);
        node.set_global_position(at + Vector3::UP * 0.9);
        node.call("setup", &[GString::from(kind.key()).to_variant()]);
        self.drops.push(Drop {
            kind,
            node,
            left: zombies::POWER_UP_LIFETIME,
            id,
        });
        self.signals()
            .power_up_dropped()
            .emit(&GString::from(kind.key()));
    }

    /// Power-up `id` is gone; `taken` if a player picked it up, which
    /// everyone hears announced.
    pub fn net_drop_gone(&mut self, id: u8, taken: bool) {
        let Some(index) = self.drops.iter().position(|d| d.id == id) else {
            return;
        };
        let mut drop = self.drops.remove(index);
        drop.node.queue_free();
        if taken {
            self.signals().power_up_collected().emit(
                &GString::from(drop.kind.key()),
                &GString::from(drop.kind.label()),
            );
        }
    }

    /// This player's weapon `key` has alternate ammo `label` now (none: no
    /// longer).
    pub fn net_alt_ammo(&mut self, key: &str, label: Option<&'static str>) {
        self.net_alt_ammo.retain(|(k, _)| k != key);
        if let Some(label) = label {
            self.net_alt_ammo.push((key.to_string(), label));
        }
    }

    /// Seconds of Insta-Kill, Double Points, and Fire Sale left. Held as
    /// the engine times they end, so the HUD counts them down.
    pub fn net_timers(&mut self, insta_kill: f64, double_points: f64, fire_sale: f64) {
        let now = now_seconds();
        self.net_insta_kill_ends = now + insta_kill.max(0.0);
        self.net_double_points_ends = now + double_points.max(0.0);
        self.net_fire_sale_ends = now + fire_sale.max(0.0);
    }

    /// This player went down and bleeds out in `seconds`, or (None) is no
    /// longer down.
    pub fn net_downed(&mut self, seconds: Option<f64>) {
        self.net_bleeds_out = seconds.map(|s| now_seconds() + s.max(0.0));
        if seconds.is_none() {
            self.net_revive = None;
        }
    }

    /// A revive involving this player started (done in `seconds`), with
    /// `other` reviving us or being revived by us (`reviving`), or stopped
    /// (None).
    pub fn net_revive(&mut self, revive: Option<(String, bool, f64)>) {
        self.net_revive = revive.map(|(other, reviving, seconds)| {
            let seconds = seconds.max(0.0);
            (other, reviving, now_seconds() + seconds, seconds.max(0.01))
        });
    }

    /// The power went on (or off for a new game).
    pub fn net_power(&mut self, on: bool) {
        self.net_power = on;
    }

    /// The teammate close enough to revive ("" for none).
    pub fn net_revive_prompt(&mut self, name: &str) {
        name.clone_into(&mut self.net_revive_prompt);
    }

    /// This player's kills and headshots, from the scoreboard.
    pub fn net_tallies(&mut self, kills: u32, headshots: u32) {
        self.kills = i64::from(kills);
        self.headshots = i64::from(headshots);
    }
}

/// Engine time in seconds.
fn now_seconds() -> f64 {
    godot::classes::Time::singleton().get_ticks_usec() as f64 / 1e6
}
