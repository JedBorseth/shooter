//! One player's part of the current match on the dedicated server: when
//! they started playing it and their stats (`shooter_sim::stats`), which the
//! server sends to their game.

use shooter_sim::net::{TICK_RATE, Tick};
use shooter_sim::stats::Recorder;

#[derive(Clone, Debug, Default)]
pub struct MatchShare {
    pub playing_since: Option<Tick>,
    pub stats: Recorder,
    /// When the stats were last sent to the player.
    pub stats_sent: Tick,
}

impl MatchShare {
    pub fn start(&mut self, tick: Tick) {
        self.playing_since = Some(tick);
        self.stats.clear();
        self.stats_sent = tick;
    }

    /// Whole seconds played since the match started for them.
    pub fn seconds(&self, now: Tick) -> u32 {
        self.playing_since
            .map_or(0, |since| now.saturating_sub(since) / TICK_RATE)
    }
}
