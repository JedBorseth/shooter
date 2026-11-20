//! The server allocator: starts a dedicated server container for each match
//! the matchmaker makes, and says where players should connect.
//!
//! Nakama's match handler (`docker/online/nakama/modules/matches.lua`) calls
//! `POST /allocate` with the match's mode and map. The allocator picks a free
//! UDP port pair from its range, starts the server image through the Docker
//! Engine API (the Docker socket), waits for the image's health check (the
//! server answering its query port), and answers with the public address. The
//! server is ranked (it checks accounts and reports XP) and stops by itself
//! once its players have gone (`--exit-when-empty`); Docker removes the
//! stopped container.
//!
//! Standard library and `serde_json` only. The HTTP server and the Docker
//! client speak just enough HTTP/1.1 for this: one request per connection.
//! See docs/online.md.

// The Docker socket is a Unix socket; the allocator runs on Linux.
#[cfg(unix)]
pub mod docker;
pub mod http;

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Modes a match may ask for, as the server's `--mode` takes them.
pub const MODES: [&str; 7] = ["tdm", "ctf", "hardpoint", "dom", "gungame", "ffa", "snd"];
/// Maps a match may ask for, as the server's `--map` takes them.
pub const MAPS: [&str; 5] = ["slamcity", "nuketown", "warehouse", "breakwater", "arena"];
/// Every container the allocator starts carries this label.
pub const LABEL: &str = "shooter.allocator";
/// The game port of a started container, as a label.
pub const PORT_LABEL: &str = "shooter.port";

#[derive(Clone, Debug)]
pub struct Config {
    /// The dedicated server image.
    pub image: String,
    /// The host name or address players connect to.
    pub public_host: String,
    /// Game ports to hand out (even numbers; each server also takes the
    /// next port for queries).
    pub first_port: u16,
    pub last_port: u16,
    /// At most this many servers at once.
    pub max_servers: usize,
    /// The Docker network the servers join, so they reach the account
    /// server by name; empty for Docker's default bridge.
    pub network: String,
    /// The account server as the servers reach it, and its HTTP key: the
    /// servers are ranked when set.
    pub accounts_url: String,
    pub accounts_key: String,
    /// Bot difficulty that fills each match.
    pub bot_skill: String,
    /// Seconds a server waits with nobody connected before stopping.
    pub exit_when_empty: u32,
    /// Memory limit for each server, in bytes (0: none).
    pub memory_bytes: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            image: "ghcr.io/jedborseth/shooter-server:nightly".into(),
            public_host: "127.0.0.1".into(),
            first_port: 27100,
            last_port: 27198,
            max_servers: 8,
            network: String::new(),
            accounts_url: String::new(),
            accounts_key: String::new(),
            bot_skill: "normal".into(),
            exit_when_empty: 120,
            memory_bytes: 1 << 30,
        }
    }
}

/// What the match handler asks for.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct AllocateRequest {
    pub mode: String,
    pub map: String,
    /// Players the match was made for; bots fill the rest of the teams.
    #[serde(default)]
    pub players: u8,
    /// The match handler's match id, for the server's name and logs.
    #[serde(default)]
    pub match_id: String,
}

/// Where players connect.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Allocation {
    pub host: String,
    pub port: u16,
    pub container: String,
}

#[derive(Debug, PartialEq)]
pub enum AllocateError {
    BadRequest(String),
    Full,
    Docker(String),
    Unhealthy,
}

impl AllocateError {
    pub fn status(&self) -> u16 {
        match self {
            AllocateError::BadRequest(_) => 400,
            AllocateError::Full => 503,
            AllocateError::Docker(_) | AllocateError::Unhealthy => 502,
        }
    }

    pub fn message(&self) -> String {
        match self {
            AllocateError::BadRequest(why) => why.clone(),
            AllocateError::Full => "every server slot is in use".into(),
            AllocateError::Docker(why) => format!("docker: {why}"),
            AllocateError::Unhealthy => "the server did not come up".into(),
        }
    }
}

impl AllocateRequest {
    pub fn validate(&self) -> Result<(), AllocateError> {
        if !MODES.contains(&self.mode.as_str()) {
            return Err(AllocateError::BadRequest(format!(
                "unknown mode {:?}",
                self.mode
            )));
        }
        if !MAPS.contains(&self.map.as_str()) {
            return Err(AllocateError::BadRequest(format!(
                "unknown map {:?}",
                self.map
            )));
        }
        if self.mode == "snd" && !matches!(self.map.as_str(), "slamcity" | "breakwater") {
            return Err(AllocateError::BadRequest(
                "Search and Destroy needs Slamcity or Breakwater".into(),
            ));
        }
        Ok(())
    }
}

/// The first free game port in the range, or `None` when the range or the
/// server cap is used up.
pub fn pick_port(config: &Config, used: &BTreeSet<u16>) -> Option<u16> {
    if used.len() >= config.max_servers {
        return None;
    }
    (config.first_port..=config.last_port)
        .step_by(2)
        .find(|port| !used.contains(port) && !used.contains(&port.wrapping_sub(1)))
}

/// Bots a matchmade server starts with: with one player that makes twelve
/// (6v6, or twelve for everyone in free-for-all modes), and each player who
/// joins takes a bot's place.
const BOTS: u8 = 11;

/// The Docker Engine API body that creates a server container for `request`
/// on `port`.
pub fn container_spec(config: &Config, request: &AllocateRequest, port: u16) -> Value {
    let query = port + 1;
    let name = if request.match_id.is_empty() {
        format!("Matchmade {port}")
    } else {
        format!(
            "Match {}",
            request.match_id.chars().take(8).collect::<String>()
        )
    };
    let mut env = vec![
        format!("SHOOTER_PORT={port}"),
        format!("SHOOTER_QUERY_PORT={query}"),
        format!("SHOOTER_MODE={}", request.mode),
        format!("SHOOTER_MAP={}", request.map),
        format!("SHOOTER_NAME={name}"),
        format!("SHOOTER_BOTS={BOTS}"),
        format!("SHOOTER_BOT_SKILL={}", config.bot_skill),
        "SHOOTER_AUTO_START=1".to_owned(),
        format!("SHOOTER_EXIT_WHEN_EMPTY={}", config.exit_when_empty),
    ];
    if !config.accounts_url.is_empty() {
        env.push(format!("SHOOTER_ACCOUNTS={}", config.accounts_url));
        env.push(format!("SHOOTER_ACCOUNTS_KEY={}", config.accounts_key));
    }
    let game = format!("{port}/udp");
    let query = format!("{query}/udp");
    let mut host_config = json!({
        "AutoRemove": true,
        "PortBindings": {
            game.clone(): [{"HostPort": port.to_string()}],
            query.clone(): [{"HostPort": (port + 1).to_string()}],
        },
    });
    if config.memory_bytes > 0 {
        host_config["Memory"] = json!(config.memory_bytes);
    }
    if !config.network.is_empty() {
        host_config["NetworkMode"] = json!(config.network);
    }
    json!({
        "Image": config.image,
        "Env": env,
        "ExposedPorts": { game: {}, query: {} },
        "Labels": { LABEL: "1", PORT_LABEL: port.to_string() },
        "HostConfig": host_config,
    })
}

/// Game ports of the allocator's containers in a `GET /containers/json`
/// answer.
pub fn used_ports(containers: &Value) -> BTreeSet<u16> {
    containers
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| c["Labels"][PORT_LABEL].as_str()?.parse().ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(mode: &str, map: &str) -> AllocateRequest {
        AllocateRequest {
            mode: mode.into(),
            map: map.into(),
            players: 2,
            match_id: "0c1e5a77-1234".into(),
        }
    }

    #[test]
    fn only_known_modes_and_maps_are_allocated() {
        assert_eq!(request("tdm", "slamcity").validate(), Ok(()));
        assert!(request("tdm; rm -rf /", "slamcity").validate().is_err());
        assert!(request("tdm", "../etc").validate().is_err());
        assert!(request("snd", "nuketown").validate().is_err());
        assert_eq!(request("snd", "breakwater").validate(), Ok(()));
    }

    #[test]
    fn ports_go_in_pairs_and_stop_at_the_cap() {
        let config = Config {
            first_port: 27100,
            last_port: 27106,
            max_servers: 3,
            ..Config::default()
        };
        let mut used = BTreeSet::new();
        assert_eq!(pick_port(&config, &used), Some(27100));
        used.insert(27100);
        assert_eq!(pick_port(&config, &used), Some(27102));
        used.insert(27104);
        assert_eq!(pick_port(&config, &used), Some(27102));
        used.insert(27102);
        assert_eq!(pick_port(&config, &used), None, "three servers is the cap");
        let roomy = Config {
            max_servers: 10,
            ..config
        };
        assert_eq!(pick_port(&roomy, &used), Some(27106));
        used.insert(27106);
        assert_eq!(pick_port(&roomy, &used), None, "the range is used up");
    }

    #[test]
    fn the_container_publishes_both_ports_on_the_same_numbers() {
        let config = Config {
            network: "shooter-online_default".into(),
            accounts_url: "http://nakama:7350".into(),
            accounts_key: "secret".into(),
            ..Config::default()
        };
        let spec = container_spec(&config, &request("dom", "warehouse"), 27104);
        let env: Vec<&str> = spec["Env"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert!(env.contains(&"SHOOTER_PORT=27104"));
        assert!(env.contains(&"SHOOTER_QUERY_PORT=27105"));
        assert!(env.contains(&"SHOOTER_MODE=dom"));
        assert!(env.contains(&"SHOOTER_MAP=warehouse"));
        assert!(env.contains(&"SHOOTER_ACCOUNTS=http://nakama:7350"));
        assert!(env.contains(&"SHOOTER_AUTO_START=1"));
        let bindings = &spec["HostConfig"]["PortBindings"];
        assert_eq!(bindings["27104/udp"][0]["HostPort"], "27104");
        assert_eq!(bindings["27105/udp"][0]["HostPort"], "27105");
        assert_eq!(spec["HostConfig"]["NetworkMode"], "shooter-online_default");
        assert_eq!(spec["HostConfig"]["AutoRemove"], true);
        assert_eq!(spec["Labels"][PORT_LABEL], "27104");
    }

    #[test]
    fn used_ports_come_from_the_labels() {
        let listed = json!([
            {"Labels": {LABEL: "1", PORT_LABEL: "27100"}},
            {"Labels": {LABEL: "1", PORT_LABEL: "27108"}},
            {"Labels": {"other": "x"}},
        ]);
        assert_eq!(used_ports(&listed), BTreeSet::from([27100, 27108]));
    }
}
