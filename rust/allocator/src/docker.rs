//! A Docker Engine API client over the Docker socket: list, create, start,
//! inspect, and remove the allocator's containers.

use std::io::Read;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::http;
use crate::{AllocateError, LABEL};

pub struct Docker {
    pub socket: PathBuf,
}

impl Docker {
    fn call(&self, start: &str, body: &[u8]) -> Result<(u16, Value), AllocateError> {
        let fail = |e: std::io::Error| AllocateError::Docker(e.to_string());
        let mut stream = UnixStream::connect(&self.socket).map_err(fail)?;
        stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .map_err(fail)?;
        http::write_message(&mut stream, start, body).map_err(fail)?;
        let message = http::read_message(&stream, true).map_err(fail)?;
        let value = if message.body.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&message.body).unwrap_or(Value::Null)
        };
        Ok((message.status(), value))
    }

    fn expect(&self, start: &str, body: &[u8], ok: &[u16]) -> Result<Value, AllocateError> {
        let (status, value) = self.call(start, body)?;
        if ok.contains(&status) {
            Ok(value)
        } else {
            let why = value["message"].as_str().unwrap_or("no message");
            Err(AllocateError::Docker(format!("{start}: {status} {why}")))
        }
    }

    /// The allocator's running containers.
    pub fn list(&self) -> Result<Value, AllocateError> {
        // filters={"label":["shooter.allocator"]}, URL-encoded.
        let path = format!(
            "GET /containers/json?filters=%7B%22label%22%3A%5B%22{}%22%5D%7D",
            LABEL
        );
        self.expect(&path, b"", &[200])
    }

    /// Whether `image` is on this machine.
    pub fn has_image(&self, image: &str) -> Result<bool, AllocateError> {
        let (status, _) = self.call(&format!("GET /images/{image}/json"), b"")?;
        Ok(status == 200)
    }

    /// Pulls `image` ("repo:tag"), waiting for the download to finish.
    pub fn pull(&self, image: &str) -> Result<(), AllocateError> {
        let fail = |e: std::io::Error| AllocateError::Docker(e.to_string());
        let mut stream = UnixStream::connect(&self.socket).map_err(fail)?;
        stream
            .set_read_timeout(Some(Duration::from_secs(600)))
            .map_err(fail)?;
        http::write_message(
            &mut stream,
            &format!("POST /images/create?fromImage={image}"),
            b"",
        )
        .map_err(fail)?;
        // The answer streams progress until the pull is done; only the end
        // matters.
        let mut progress = Vec::new();
        stream.read_to_end(&mut progress).map_err(fail)?;
        let text = String::from_utf8_lossy(&progress);
        if !text.starts_with("HTTP/1.1 200") || text.contains("\"error\"") {
            let line = text
                .lines()
                .rev()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("");
            return Err(AllocateError::Docker(format!("pull {image}: {line}")));
        }
        Ok(())
    }

    /// Creates and starts a container from `spec`; its id.
    pub fn run(&self, name: &str, spec: &Value) -> Result<String, AllocateError> {
        let body = serde_json::to_vec(spec).map_err(|e| AllocateError::Docker(e.to_string()))?;
        let created = self.expect(
            &format!("POST /containers/create?name={name}"),
            &body,
            &[201],
        )?;
        let id = created["Id"]
            .as_str()
            .ok_or_else(|| AllocateError::Docker("no container id".into()))?
            .to_owned();
        if let Err(error) = self.expect(&format!("POST /containers/{id}/start"), b"", &[204, 304]) {
            self.remove(&id);
            return Err(error);
        }
        Ok(id)
    }

    /// Waits for the container's health check to pass.
    pub fn wait_healthy(&self, id: &str, timeout: Duration) -> Result<(), AllocateError> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            let state = self.expect(&format!("GET /containers/{id}/json"), b"", &[200])?;
            match state["State"]["Health"]["Status"].as_str() {
                Some("healthy") => return Ok(()),
                Some("unhealthy") => break,
                _ => {}
            }
            if state["State"]["Running"] == Value::Bool(false) {
                break;
            }
            thread::sleep(Duration::from_millis(500));
        }
        self.remove(id);
        Err(AllocateError::Unhealthy)
    }

    /// Stops and removes a container, ignoring failures.
    pub fn remove(&self, id: &str) {
        let _ = self.call(&format!("DELETE /containers/{id}?force=true"), b"");
    }
}
