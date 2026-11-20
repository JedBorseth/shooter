//! Client-side prediction and reconciliation.
//!
//! The client applies its own input immediately and remembers, per input
//! sequence number, the input and the state it produced. When a snapshot says
//! "after your input N the server had state S", the client compares S with its
//! own record for N. If they agree, nothing happens. If not, it restarts from S
//! and replays every later input, so the correction lands in the present
//! without the player feeling the round trip. (Gambetta, *Fast-Paced
//! Multiplayer*; Overwatch GDC 2017.)

use std::collections::VecDeque;

use glam::Vec3;

use super::protocol::{MAX_INPUTS_PER_PACKET, Predicted};
use crate::movement::MoveState;
use crate::player::PlayerInput;
use crate::weapon::{Action, CombatState};

/// About 2 seconds of inputs; older ones can no longer be acknowledged usefully.
const HISTORY: usize = 2 * super::TICK_RATE as usize;

/// Position error below this is float noise, not a misprediction.
pub const POSITION_TOLERANCE: f32 = 0.01;
const VELOCITY_TOLERANCE: f32 = 0.05;
const TIME_TOLERANCE: f32 = 1e-3;

#[derive(Default)]
pub struct Prediction {
    history: VecDeque<Entry>,
    next_seq: u32,
    acked: u32,
    /// Mispredictions corrected so far.
    pub corrections: u32,
    /// Restarts from server state with no prediction to compare against
    /// (joining, before the server has applied any input).
    pub resyncs: u32,
    /// Distance the predicted position moved at the last correction.
    pub last_error: f32,
}

#[derive(Clone, Copy)]
struct Entry {
    seq: u32,
    input: PlayerInput,
    after: Predicted,
}

/// What a snapshot did to the prediction.
#[derive(Clone, Debug, PartialEq)]
pub enum Reconciled {
    /// Stale or duplicate snapshot; nothing to do.
    Ignored,
    /// The server agrees with the prediction.
    InSync,
    /// The client restarted from the server's state and replayed its inputs.
    /// `previous` is the present-tick position before; `current` the state after.
    /// `current` is boxed: the full simulation state, far larger than the rest.
    Corrected {
        previous: Vec3,
        current: Box<Predicted>,
    },
}

impl Prediction {
    /// Hands out the sequence number for the next input (starting at 1).
    pub fn next_seq(&mut self) -> u32 {
        self.next_seq += 1;
        self.next_seq
    }

    /// Remembers an input and the state it produced.
    pub fn record(&mut self, seq: u32, input: PlayerInput, after: Predicted) {
        self.history.push_back(Entry { seq, input, after });
        while self.history.len() > HISTORY {
            self.history.pop_front();
        }
    }

    /// Last input sequence the server confirmed.
    pub fn acked(&self) -> u32 {
        self.acked
    }

    /// Inputs the server has not confirmed yet, to send (newest last).
    pub fn unacked(&self) -> (u32, Vec<PlayerInput>) {
        let pending: Vec<&Entry> = self.history.iter().filter(|e| e.seq > self.acked).collect();
        let start = pending.len().saturating_sub(MAX_INPUTS_PER_PACKET);
        let first = pending.get(start).map_or(self.next_seq + 1, |e| e.seq);
        (first, pending[start..].iter().map(|e| e.input).collect())
    }

    /// The input numbered `seq`, while it is still remembered.
    pub fn input(&self, seq: u32) -> Option<PlayerInput> {
        self.history.iter().find(|e| e.seq == seq).map(|e| e.input)
    }

    /// Number of inputs predicted ahead of the server.
    pub fn pending(&self) -> usize {
        self.history.iter().filter(|e| e.seq > self.acked).count()
    }

    /// Applies the server's state after input `ack`. `step` must advance a
    /// state by one input exactly as the live game does (including collision).
    pub fn reconcile(
        &mut self,
        ack: u32,
        server: &Predicted,
        mut step: impl FnMut(&mut Predicted, &PlayerInput),
    ) -> Reconciled {
        if ack < self.acked || (ack == self.acked && ack != 0) {
            return Reconciled::Ignored;
        }
        self.acked = ack;
        let predicted = self.history.iter().find(|e| e.seq == ack).map(|e| e.after);
        while self.history.front().is_some_and(|e| e.seq <= ack) {
            self.history.pop_front();
        }
        if predicted.is_some_and(|p| !diverged(&p, server)) {
            return Reconciled::InSync;
        }

        let previous = self
            .history
            .back()
            .map_or(server.position, |e| e.after.position);
        let mut state = *server;
        for entry in &mut self.history {
            step(&mut state, &entry.input);
            entry.after = state;
        }
        if predicted.is_some() {
            self.corrections += 1;
            self.last_error = previous.distance(state.position);
        } else {
            self.resyncs += 1;
        }
        Reconciled::Corrected {
            previous,
            current: Box::new(state),
        }
    }
}

/// True when the server's result differs from the prediction by more than float noise.
pub fn diverged(a: &Predicted, b: &Predicted) -> bool {
    a.on_floor != b.on_floor
        || a.state.reload_held != b.state.reload_held
        || a.state.melee_held != b.state.melee_held
        || a.state.lethal_held != b.state.lethal_held
        || a.state.tactical_held != b.state.tactical_held
        || a.state.airborne != b.state.airborne
        || (a.state.health - b.state.health).abs() > 0.01
        || (a.state.fall_speed - b.state.fall_speed).abs() > VELOCITY_TOLERANCE
        || (a.state.since_damage - b.state.since_damage).abs() > TIME_TOLERANCE
        || a.position.distance(b.position) > POSITION_TOLERANCE
        || move_diverged(&a.state.movement, &b.state.movement)
        || combat_diverged(&a.state.combat, &b.state.combat)
}

fn move_diverged(a: &MoveState, b: &MoveState) -> bool {
    a.stance != b.stance
        || a.sprinting != b.sprinting
        || (a.jump_held, a.crouch_held, a.sprint_held)
            != (b.jump_held, b.crouch_held, b.sprint_held)
        || a.velocity.distance(b.velocity) > VELOCITY_TOLERANCE
        || (a.slide_time - b.slide_time).abs() > TIME_TOLERANCE
        || (a.slide_cooldown - b.slide_cooldown).abs() > TIME_TOLERANCE
        || (a.jump_buffer - b.jump_buffer).abs() > TIME_TOLERANCE
        || (a.coyote - b.coyote).abs() > TIME_TOLERANCE
        || (a.lunge_time - b.lunge_time).abs() > TIME_TOLERANCE
        || a.lunge_velocity.distance(b.lunge_velocity) > VELOCITY_TOLERANCE
}

fn combat_diverged(a: &CombatState, b: &CombatState) -> bool {
    let close = |x: f32, y: f32| (x - y).abs() <= TIME_TOLERANCE;
    let action_diverged = match (a.action, b.action) {
        (Action::Ready, Action::Ready) => false,
        (
            Action::Reloading {
                remaining: ra,
                empty: ea,
            },
            Action::Reloading {
                remaining: rb,
                empty: eb,
            },
        ) => ea != eb || (ra - rb).abs() > TIME_TOLERANCE,
        (
            Action::Lowering {
                remaining: ra,
                to: ta,
            },
            Action::Lowering {
                remaining: rb,
                to: tb,
            },
        ) => ta != tb || (ra - rb).abs() > TIME_TOLERANCE,
        (
            Action::Raising {
                remaining: ra,
                kind: ka,
            },
            Action::Raising {
                remaining: rb,
                kind: kb,
            },
        ) => ka != kb || !close(ra, rb),
        (Action::Cycling { remaining: ra }, Action::Cycling { remaining: rb }) => !close(ra, rb),
        (
            Action::Loading {
                remaining: ra,
                stage: sa,
            },
            Action::Loading {
                remaining: rb,
                stage: sb,
            },
        ) => sa != sb || !close(ra, rb),
        (
            Action::Meleeing {
                remaining: ra,
                strike_in: sa,
                lunge: la,
            },
            Action::Meleeing {
                remaining: rb,
                strike_in: sb,
                lunge: lb,
            },
        ) => la != lb || !close(ra, rb) || !close(sa, sb),
        (
            Action::Throwing {
                kind: ka,
                cooked: ca,
                release_in: la,
            },
            Action::Throwing {
                kind: kb,
                cooked: cb,
                release_in: lb,
            },
        ) => {
            ka != kb
                || !close(ca, cb)
                || match (la, lb) {
                    (None, None) => false,
                    (Some(x), Some(y)) => !close(x, y),
                    _ => true,
                }
        }
        _ => true,
    };
    action_diverged
        || a.slots != b.slots
        || a.active != b.active
        || a.burst != b.burst
        || a.shots_fired != b.shots_fired
        || a.fire_held != b.fire_held
        || (a.ads - b.ads).abs() > TIME_TOLERANCE
        || (a.cooldown - b.cooldown).abs() > TIME_TOLERANCE
        || (a.sprint_out - b.sprint_out).abs() > TIME_TOLERANCE
        || a.recoil.distance(b.recoil) > 1e-4
        || a.holding_breath != b.holding_breath
        || a.reload_queued != b.reload_queued
        || a.lethals != b.lethals
        || (a.tactical, a.tacticals) != (b.tactical, b.tacticals)
        || !close(a.sway_time, b.sway_time)
        || !close(a.breath, b.breath)
        || !close(a.winded, b.winded)
}

/// Hides corrections: the rendered position keeps an offset that starts at
/// the size of the jump and decays, instead of snapping.
#[derive(Clone, Copy, Debug, Default)]
pub struct ErrorSmoothing {
    pub offset: Vec3,
}

impl ErrorSmoothing {
    /// Corrections larger than this (teleports, respawns) snap instead.
    const SNAP_DISTANCE: f32 = 2.0;
    /// Decay rate per second (about 90% gone after 0.15 s).
    const RATE: f32 = 15.0;

    pub fn correct(&mut self, previous: Vec3, current: Vec3) {
        self.offset += previous - current;
        if self.offset.length() > Self::SNAP_DISTANCE {
            self.offset = Vec3::ZERO;
        }
    }

    pub fn update(&mut self, dt: f32) -> Vec3 {
        self.offset *= (-Self::RATE * dt).exp();
        if self.offset.length_squared() < 1e-8 {
            self.offset = Vec3::ZERO;
        }
        self.offset
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::movement::MoveEnv;
    use crate::player::{self, PlayerParams, PlayerState};
    use glam::Vec2;

    const DT: f32 = 1.0 / 60.0;

    /// A stand-in for the engine: integrate velocity on a flat floor at y = 0.
    fn step(p: &mut Predicted, input: &PlayerInput) {
        let env = MoveEnv {
            on_floor: p.on_floor,
            can_stand: true,
            ledge: None,
            ledge_low: false,
            vault: None,
            ladder: None,
            mount: None,
        };
        let mut events = Vec::new();
        let v = player::step(
            &mut p.state,
            input,
            env,
            &PlayerParams::default(),
            DT,
            &mut events,
        );
        p.position += v * DT;
        if p.position.y <= 0.0 {
            p.position.y = 0.0;
            p.state.movement.velocity.y = 0.0;
        }
        p.on_floor = p.position.y <= 0.0;
    }

    fn start() -> Predicted {
        Predicted {
            state: PlayerState::default(),
            position: Vec3::ZERO,
            on_floor: true,
        }
    }

    fn input(i: u32) -> PlayerInput {
        PlayerInput {
            wish: Vec2::new(((i / 20) % 2) as f32 - 0.5, 1.0),
            yaw: i as f32 * 0.02,
            jump: i.is_multiple_of(45),
            ..Default::default()
        }
        .quantized()
    }

    #[test]
    fn matching_server_state_needs_no_correction() {
        let mut client = Prediction::default();
        let mut local = start();
        let mut server = start();
        let mut server_states = Vec::new();
        for i in 0..30 {
            let seq = client.next_seq();
            step(&mut local, &input(i));
            client.record(seq, input(i), local);
            step(&mut server, &input(i));
            server_states.push(server);
        }
        // The server is 10 inputs behind the client.
        let result = client.reconcile(20, &server_states[19], |_, _| panic!("no replay"));
        assert_eq!(result, Reconciled::InSync);
        assert_eq!(client.pending(), 10);
        assert_eq!(client.unacked().0, 21);
        assert_eq!(
            client.reconcile(20, &server_states[19], step),
            Reconciled::Ignored
        );
    }

    #[test]
    fn a_misprediction_replays_to_the_server_result() {
        let mut client = Prediction::default();
        let mut local = start();
        let mut server = start();
        let mut server_states = Vec::new();
        for i in 0..40 {
            let seq = client.next_seq();
            step(&mut local, &input(i));
            client.record(seq, input(i), local);
            step(&mut server, &input(i));
            if i == 9 {
                // Something the client could not know about shoved the player.
                server.position.x += 0.5;
            }
            server_states.push(server);
        }
        let result = client.reconcile(15, &server_states[14], step);
        let Reconciled::Corrected { previous, current } = result else {
            panic!("expected a correction, got {result:?}");
        };
        assert!((previous.distance(current.position) - 0.5).abs() < 1e-3);
        assert_eq!(client.corrections, 1);
        // Replaying the remaining inputs lands exactly where the server will be.
        let later = client.reconcile(40, &server_states[39], |_, _| panic!("no replay"));
        assert_eq!(later, Reconciled::InSync);
    }

    #[test]
    fn unacked_inputs_are_capped_per_packet() {
        let mut client = Prediction::default();
        for i in 0..40 {
            let seq = client.next_seq();
            client.record(seq, input(i), start());
        }
        let (first, inputs) = client.unacked();
        assert_eq!(inputs.len(), MAX_INPUTS_PER_PACKET);
        assert_eq!(first, 41 - MAX_INPUTS_PER_PACKET as u32);
    }

    #[test]
    fn smoothing_hides_small_corrections_and_snaps_teleports() {
        let mut s = ErrorSmoothing::default();
        s.correct(Vec3::new(0.3, 0.0, 0.0), Vec3::ZERO);
        assert!((s.offset.x - 0.3).abs() < 1e-6);
        for _ in 0..30 {
            s.update(DT);
        }
        assert!(s.offset.x < 0.01, "decays within half a second");
        s.correct(Vec3::new(10.0, 0.0, 0.0), Vec3::ZERO);
        assert_eq!(s.offset, Vec3::ZERO);
    }
}
