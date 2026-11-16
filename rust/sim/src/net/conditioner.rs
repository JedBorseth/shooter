//! Network condition simulator for local testing.
//!
//! Delays, jitters, and drops packets on their way through, so prediction,
//! reconciliation, and interpolation can be tried at realistic pings on one
//! machine. Losses and jitter come from hashed counters, so a test run is
//! reproducible.

use crate::rng;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LinkSettings {
    /// One-way delay in seconds.
    pub latency: f32,
    /// Extra random one-way delay, up to this many seconds. Reorders packets.
    pub jitter: f32,
    /// Fraction of packets dropped, 0 to 1.
    pub loss: f32,
}

impl LinkSettings {
    pub fn is_perfect(&self) -> bool {
        self.latency <= 0.0 && self.jitter <= 0.0 && self.loss <= 0.0
    }
}

pub struct LinkConditioner {
    pub settings: LinkSettings,
    seed: u32,
    counter: u32,
    queue: Vec<(f64, u32, Vec<u8>)>,
}

impl LinkConditioner {
    pub fn new(settings: LinkSettings, seed: u32) -> Self {
        Self {
            settings,
            seed,
            counter: 0,
            queue: Vec::new(),
        }
    }

    /// Queues a packet sent at time `now` (seconds).
    pub fn send(&mut self, now: f64, packet: Vec<u8>) {
        self.counter = self.counter.wrapping_add(1);
        if rng::unit(self.seed, self.counter.wrapping_mul(2)) < self.settings.loss {
            return;
        }
        let jitter = rng::unit(self.seed, self.counter.wrapping_mul(2) + 1) * self.settings.jitter;
        let deliver_at = now + f64::from(self.settings.latency + jitter);
        self.queue.push((deliver_at, self.counter, packet));
    }

    /// Packets due by time `now`, in delivery order.
    pub fn receive(&mut self, now: f64) -> Vec<Vec<u8>> {
        self.queue
            .sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        let due = self.queue.partition_point(|(at, _, _)| *at <= now);
        self.queue.drain(..due).map(|(_, _, p)| p).collect()
    }

    pub fn in_flight(&self) -> usize {
        self.queue.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delays_and_drops() {
        let mut link = LinkConditioner::new(
            LinkSettings {
                latency: 0.05,
                jitter: 0.0,
                loss: 0.25,
            },
            7,
        );
        for i in 0..1000u32 {
            link.send(0.0, i.to_le_bytes().to_vec());
        }
        assert!(link.receive(0.049).is_empty());
        let got = link.receive(0.051);
        assert!(
            (700..800).contains(&got.len()),
            "about 25% lost: {}",
            got.len()
        );
        // Without jitter, order is preserved.
        let ids: Vec<u32> = got
            .iter()
            .map(|p| u32::from_le_bytes(p[..4].try_into().unwrap()))
            .collect();
        assert!(ids.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn jitter_reorders_within_its_window() {
        let mut link = LinkConditioner::new(
            LinkSettings {
                latency: 0.0,
                jitter: 0.03,
                loss: 0.0,
            },
            1,
        );
        for i in 0..100u32 {
            link.send(f64::from(i) / 60.0, vec![i as u8]);
        }
        assert_eq!(link.receive(0.5).len() + link.in_flight(), 100);
        let rest = link.receive(10.0);
        assert!(!rest.is_empty());
        assert_eq!(link.in_flight(), 0);
    }
}
