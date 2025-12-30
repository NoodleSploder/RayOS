use crate::types::GazePoint;
use anyhow::Result;
use serde::Deserialize;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::net::UdpSocket;

#[derive(Debug, Clone, Deserialize)]
struct JsonGazeMsg {
    x: f32,
    y: f32,
    #[serde(default)]
    confidence: Option<f32>,
    #[serde(default)]
    timestamp: Option<u64>,
}

pub fn udp_gaze_addr_from_env() -> Option<SocketAddr> {
    let raw = std::env::var("RAYOS_GAZE_UDP_ADDR").ok()?;
    raw.parse::<SocketAddr>().ok()
}

pub fn parse_gaze_message(msg: &str) -> Option<GazePoint> {
    let msg = msg.trim();
    if msg.is_empty() {
        return None;
    }

    // JSON: {"x":0.5,"y":0.5,"confidence":1.0,"timestamp":123}
    if msg.starts_with('{') {
        if let Ok(j) = serde_json::from_str::<JsonGazeMsg>(msg) {
            return Some(GazePoint {
                screen_x: j.x.clamp(0.0, 1.0),
                screen_y: j.y.clamp(0.0, 1.0),
                confidence: j.confidence.unwrap_or(1.0).clamp(0.0, 1.0),
                timestamp: j.timestamp.unwrap_or_else(now_ms),
            });
        }
    }

    // k=v tokens: x=0.5 y=0.5 conf=0.9 ts=123
    let mut x: Option<f32> = None;
    let mut y: Option<f32> = None;
    let mut confidence: Option<f32> = None;
    let mut timestamp: Option<u64> = None;

    for tok in msg.split_whitespace() {
        let (k, v) = tok.split_once('=')?;
        match k {
            "x" => x = v.parse().ok(),
            "y" => y = v.parse().ok(),
            "conf" | "confidence" => confidence = v.parse().ok(),
            "ts" | "timestamp" => timestamp = v.parse().ok(),
            _ => {}
        }
    }

    let x = x?;
    let y = y?;

    Some(GazePoint {
        screen_x: x.clamp(0.0, 1.0),
        screen_y: y.clamp(0.0, 1.0),
        confidence: confidence.unwrap_or(1.0).clamp(0.0, 1.0),
        timestamp: timestamp.unwrap_or_else(now_ms),
    })
}

pub async fn spawn_udp_gaze_task(
    bind_addr: SocketAddr,
    gaze_storage: Arc<Mutex<Option<GazePoint>>>,
) -> Result<()> {
    let sock = UdpSocket::bind(bind_addr).await?;
    log::info!("UDP gaze listener bound on {bind_addr}");

    tokio::spawn(async move {
        let mut buf = [0u8; 2048];
        loop {
            let (len, _src) = match sock.recv_from(&mut buf).await {
                Ok(v) => v,
                Err(e) => {
                    log::warn!("UDP gaze recv error: {e}");
                    continue;
                }
            };

            if let Ok(s) = std::str::from_utf8(&buf[..len]) {
                if let Some(g) = parse_gaze_message(s) {
                    *gaze_storage.lock().unwrap() = Some(g);
                }
            }
        }
    });

    Ok(())
}

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_json_gaze() {
        let g = parse_gaze_message(r#"{"x":0.25,"y":0.75,"confidence":0.9,"timestamp":123}"#)
            .expect("parse");
        assert!((g.screen_x - 0.25).abs() < 1e-6);
        assert!((g.screen_y - 0.75).abs() < 1e-6);
        assert!((g.confidence - 0.9).abs() < 1e-6);
        assert_eq!(g.timestamp, 123);
    }

    #[test]
    fn parse_kv_gaze() {
        let g = parse_gaze_message("x=0.1 y=0.2 conf=0.3 ts=42").expect("parse");
        assert!((g.screen_x - 0.1).abs() < 1e-6);
        assert!((g.screen_y - 0.2).abs() < 1e-6);
        assert!((g.confidence - 0.3).abs() < 1e-6);
        assert_eq!(g.timestamp, 42);
    }
}
