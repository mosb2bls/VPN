//! Minimal client for sing-box's Clash-compatible API on 127.0.0.1 (random
//! port, random per-session secret). Never uses any proxy.

use std::time::{Duration, Instant};

use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use serde_json::Value;

#[derive(Clone)]
pub struct ClashApi {
    base: String,
    secret: String,
    agent: ureq::Agent,
}

impl ClashApi {
    pub fn new(port: u16, secret: &str) -> Self {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .proxy(None)
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(15)))
            .build()
            .into();
        Self { base: format!("http://127.0.0.1:{port}"), secret: secret.into(), agent }
    }

    fn get(&self, path: &str) -> Result<Value, String> {
        let mut r = self
            .agent
            .get(format!("{}{}", self.base, path))
            .header("Authorization", format!("Bearer {}", self.secret))
            .call()
            .map_err(|e| e.to_string())?;
        let status = r.status().as_u16();
        let body = r.body_mut().read_to_string().map_err(|e| e.to_string())?;
        if !(200..300).contains(&status) {
            return Err(format!("HTTP {status}: {}", body.chars().take(200).collect::<String>()));
        }
        serde_json::from_str(&body).map_err(|e| e.to_string())
    }

    pub fn version(&self) -> Result<String, String> {
        Ok(self.get("/version")?["version"].as_str().unwrap_or_default().to_string())
    }

    /// Poll `/version` until it answers or `timeout` passes.
    pub fn wait_ready(&self, timeout: Duration) -> Result<(), String> {
        let start = Instant::now();
        loop {
            match self.version() {
                Ok(_) => return Ok(()),
                Err(e) if start.elapsed() >= timeout => return Err(e),
                Err(_) => std::thread::sleep(Duration::from_millis(100)),
            }
        }
    }

    /// Switch selector `group` to outbound `name`.
    pub fn select(&self, group: &str, name: &str) -> Result<(), String> {
        let enc = |s: &str| utf8_percent_encode(s, NON_ALPHANUMERIC).to_string();
        let mut r = self
            .agent
            .put(format!("{}/proxies/{}", self.base, enc(group)))
            .header("Authorization", format!("Bearer {}", self.secret))
            .send_json(serde_json::json!({ "name": name }))
            .map_err(|e| e.to_string())?;
        let status = r.status().as_u16();
        if (200..300).contains(&status) {
            Ok(())
        } else {
            Err(format!("HTTP {status}: {}", r.body_mut().read_to_string().unwrap_or_default()))
        }
    }

    /// Real HTTP request through outbound `name` to `url`; returns latency in ms.
    pub fn delay(&self, name: &str, url: &str, timeout_ms: u32) -> Result<u32, String> {
        let enc = |s: &str| utf8_percent_encode(s, NON_ALPHANUMERIC).to_string();
        let v = self.get(&format!("/proxies/{}/delay?url={}&timeout={timeout_ms}", enc(name), enc(url)))?;
        v["delay"].as_u64().map(|d| d as u32).ok_or_else(|| format!("no delay in response: {v}"))
    }

    /// (upload total, download total, open connections).
    pub fn totals(&self) -> Result<(f64, f64, u32), String> {
        let v = self.get("/connections")?;
        Ok((
            v["uploadTotal"].as_f64().unwrap_or(0.0),
            v["downloadTotal"].as_f64().unwrap_or(0.0),
            v["connections"].as_array().map(|a| a.len() as u32).unwrap_or(0),
        ))
    }
}

/// A free TCP port on 127.0.0.1 (the OS picks; tiny race until sing-box binds).
pub fn free_port() -> std::io::Result<u16> {
    Ok(std::net::TcpListener::bind("127.0.0.1:0")?.local_addr()?.port())
}

/// Random 32-hex-character secret.
pub fn random_secret() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}
