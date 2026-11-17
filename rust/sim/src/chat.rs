//! Text chat rules: what a message may contain, and how often a player may
//! send one. The server applies them; clients only show what it relays.

/// Longest message, in characters.
pub const MAX_CHARS: usize = 120;

/// Longest message on the wire, in bytes (UTF-8 is at most 4 bytes a char).
pub const MAX_BYTES: usize = MAX_CHARS * 4;

/// A message as the server relays it: printable, trimmed, at most
/// [`MAX_CHARS`] characters. `None` if nothing is left.
pub fn clean_message(text: &str) -> Option<String> {
    let cleaned: String = text
        .chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .filter(|c| !c.is_control())
        .take(MAX_CHARS)
        .collect::<String>()
        .trim()
        .to_owned();
    (!cleaned.is_empty()).then_some(cleaned)
}

/// A token bucket per player: a burst of [`ChatLimiter::BURST`] messages,
/// then one every [`ChatLimiter::INTERVAL`] seconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChatLimiter {
    tokens: f32,
}

impl Default for ChatLimiter {
    fn default() -> Self {
        Self {
            tokens: Self::BURST,
        }
    }
}

impl ChatLimiter {
    pub const BURST: f32 = 4.0;
    pub const INTERVAL: f32 = 1.5;

    /// Time passes.
    pub fn tick(&mut self, dt: f32) {
        self.tokens = (self.tokens + dt / Self::INTERVAL).min(Self::BURST);
    }

    /// Spends a message if one is allowed now.
    pub fn allow(&mut self) -> bool {
        if self.tokens < 1.0 {
            return false;
        }
        self.tokens -= 1.0;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_are_cleaned() {
        assert_eq!(clean_message("  gg  "), Some("gg".to_owned()));
        assert_eq!(clean_message("a\nb\tc\u{7}"), Some("a b c".to_owned()));
        assert_eq!(clean_message(" \n "), None);
        let long = "x".repeat(500);
        assert_eq!(
            clean_message(&long).map(|m| m.chars().count()),
            Some(MAX_CHARS)
        );
    }

    #[test]
    fn a_burst_then_one_every_interval() {
        let mut limiter = ChatLimiter::default();
        for _ in 0..4 {
            assert!(limiter.allow());
        }
        assert!(!limiter.allow(), "the burst is spent");
        limiter.tick(ChatLimiter::INTERVAL * 0.5);
        assert!(!limiter.allow());
        limiter.tick(ChatLimiter::INTERVAL * 0.5);
        assert!(limiter.allow());
        limiter.tick(100.0);
        for _ in 0..4 {
            assert!(limiter.allow());
        }
        assert!(!limiter.allow(), "waiting refills only the burst");
    }
}
