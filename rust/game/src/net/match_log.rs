//! What happened in a match, for the bot bench (`godot/tools/bot_bench.gd`):
//! each Search and Destroy round's result, and each Hardpoint zone's arrivals
//! and hold time. Kills, deaths, and captures are already on each player's
//! score; this keeps what the score does not, so changes to the bots can be
//! compared by objective outcome as well as by kills.

use godot::prelude::*;
use shooter_sim::net::Tick;
use shooter_sim::snd::RoundEnd;
use shooter_sim::tdm::{TEAM_COUNT, Team};

/// One Search and Destroy round.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoundLog {
    pub number: u32,
    pub winner: Team,
    pub attackers: Team,
    pub reason: RoundEnd,
    /// The site the charge was planted at, if it was.
    pub planted: Option<u8>,
    /// The site the attackers' bots were sent to first, and whether they
    /// switched to another during the round.
    pub target: Option<u8>,
    pub switched: bool,
}

/// One Hardpoint zone's time live.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZoneLog {
    pub zone: usize,
    pub started: Tick,
    /// Ticks from going live until each team first had someone inside.
    pub first_in: [Option<Tick>; TEAM_COUNT],
    /// Ticks each team held it alone.
    pub held: [Tick; TEAM_COUNT],
}

#[derive(Default)]
pub struct MatchLog {
    pub rounds: Vec<RoundLog>,
    pub zones: Vec<ZoneLog>,
}

impl MatchLog {
    /// Hardpoint, every tick: `zone` is live with `presence` players of each
    /// team in it, and `holder` holds it alone.
    pub fn zone_tick(
        &mut self,
        tick: Tick,
        zone: usize,
        presence: [u32; TEAM_COUNT],
        holder: Option<Team>,
    ) {
        if self.zones.last().is_none_or(|z| z.zone != zone) {
            self.zones.push(ZoneLog {
                zone,
                started: tick,
                first_in: [None; TEAM_COUNT],
                held: [0; TEAM_COUNT],
            });
        }
        let Some(log) = self.zones.last_mut() else {
            return;
        };
        for (team, count) in presence.iter().enumerate() {
            if *count > 0 && log.first_in[team].is_none() {
                log.first_in[team] = Some(tick.saturating_sub(log.started));
            }
        }
        if let Some(team) = holder {
            log.held[usize::from(team.min(1))] += 1;
        }
    }

    /// The log as a dictionary: `rounds` and `zones`, arrays of
    /// dictionaries, times in seconds at `tick_rate`.
    pub fn to_dictionary(&self, tick_rate: f32) -> VarDictionary {
        let seconds = |t: Tick| t as f32 / tick_rate;
        let mut rounds: Array<VarDictionary> = Array::new();
        for r in &self.rounds {
            let mut d = VarDictionary::new();
            d.set("number", r.number);
            d.set("winner", i32::from(r.winner));
            d.set("attackers", i32::from(r.attackers));
            d.set("reason", end_key(r.reason));
            d.set("planted", r.planted.map_or(-1, i32::from));
            d.set("target", r.target.map_or(-1, i32::from));
            d.set("switched", r.switched);
            rounds.push(&d);
        }
        let mut zones: Array<VarDictionary> = Array::new();
        for z in &self.zones {
            let mut d = VarDictionary::new();
            d.set("zone", z.zone as i64);
            let first: Array<f32> = z.first_in.iter().map(|t| t.map_or(-1.0, seconds)).collect();
            let held: Array<f32> = z.held.iter().map(|t| seconds(*t)).collect();
            d.set("first_in", &first);
            d.set("held", &held);
            zones.push(&d);
        }
        let mut d = VarDictionary::new();
        d.set("rounds", &rounds);
        d.set("zones", &zones);
        d
    }
}

fn end_key(reason: RoundEnd) -> &'static str {
    match reason {
        RoundEnd::Eliminated => "eliminated",
        RoundEnd::Time => "time",
        RoundEnd::Detonated => "detonated",
        RoundEnd::Defused => "defused",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zone_arrivals_and_holds_are_counted_per_zone() {
        let mut log = MatchLog::default();
        log.zone_tick(100, 0, [0, 0], None);
        log.zone_tick(101, 0, [1, 0], Some(0));
        log.zone_tick(102, 0, [1, 2], None);
        log.zone_tick(200, 1, [0, 1], Some(1));
        assert_eq!(log.zones.len(), 2);
        assert_eq!(log.zones[0].first_in, [Some(1), Some(2)]);
        assert_eq!(log.zones[0].held, [1, 0]);
        assert_eq!(log.zones[1].first_in, [None, Some(0)]);
        assert_eq!(log.zones[1].held, [0, 1]);
    }
}
