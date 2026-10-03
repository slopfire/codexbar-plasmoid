//! End-to-end HTTP checks for the sync server binary: publish from two
//! devices, merged read, filter, and auth failures across the wire.

use std::io::BufRead;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::channel;
use std::thread;
use std::time::Duration;

const TOKEN: &str = "integration-test-token";

struct ServerGuard(Child);

impl Drop for ServerGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Spawn the sync server on an ephemeral port and read its "listening on"
/// line from stderr so the test knows the assigned port.
fn spawn_server() -> (ServerGuard, String, String) {
    let data_dir = std::env::temp_dir().join(format!(
        "codexbar-sync-test-{}",
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::create_dir_all(&data_dir).expect("create temp data dir");
    let mut child = Command::new(env!("CARGO_BIN_EXE_codexbar-plasmoid"))
        .args([
            "sync-server",
            "--bind",
            "127.0.0.1",
            "--port",
            "0",
            "--token",
            TOKEN,
            "--data-dir",
            data_dir.to_str().expect("utf8 data dir"),
        ])
        .stderr(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .expect("spawn sync server");

    let stderr = child.stderr.take().expect("server stderr piped");
    let rx = {
        let (tx, rx) = channel();
        thread::spawn(move || {
            let reader = std::io::BufReader::new(stderr);
            for line in reader.lines().flatten() {
                // Line looks like "sync-server listening on 127.0.0.1:PORT, data dir …".
                if let Some(port_socket) = line
                    .strip_prefix("sync-server listening on ")
                    .map(|rest| rest.split(',').next().unwrap_or("").to_string())
                {
                    let _ = tx.send(port_socket.clone());
                }
            }
        });
        rx
    };
    let socket = rx
        .recv_timeout(Duration::from_secs(10))
        .expect("sync server did not report its port in time");
    (
        ServerGuard(child),
        socket,
        data_dir.to_string_lossy().to_string(),
    )
}

fn client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .expect("build test client")
}

fn device_payload(generated_at: &str, total_tokens: i64, cost: f64) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "generatedAt": generated_at,
        "requestedProvider": "all",
        "entries": [
            {
                "provider": "opencode",
                "account": "unit@example.com",
                "source": "native",
                "updatedAt": generated_at,
                "rows": [{"id": "session", "percentLeft": 50}],
                "dailyUsage": [
                    {"dayKey": "2026-09-29", "totalTokens": 1000, "costUSD": 1.0, "models": []},
                    {"dayKey": "2026-09-30", "totalTokens": total_tokens, "costUSD": cost, "models": []}
                ]
            }
        ],
        "costError": null
    })
}

#[test]
fn publish_then_merged_read_over_http() {
    let (_guard, socket, data_dir) = spawn_server();
    let base = format!("http://{socket}");
    let http = client();

    // No token -> 401; wrong token -> 401.
    let no_auth = http.get(format!("{base}/v1/usage")).send().unwrap();
    assert_eq!(no_auth.status().as_u16(), 401);
    let bad_auth = http
        .get(format!("{base}/v1/usage"))
        .header("Authorization", "Bearer nope")
        .send()
        .unwrap();
    assert_eq!(bad_auth.status().as_u16(), 401);

    // Health needs no token.
    let health = http.get(format!("{base}/health")).send().unwrap();
    assert_eq!(health.status().as_u16(), 200);

    let exposures = [
        ("unit-a", "2026-09-30T09:00:00Z", 500i64, 2.0f64),
        ("unit-b", "2026-09-30T10:00:00Z", 300i64, 1.5f64),
    ];
    for (device, stamp, tokens, cost) in exposures {
        let put = http
            .put(format!("{base}/v1/snapshot/{device}"))
            .header("Authorization", format!("Bearer {TOKEN}"))
            .header("Content-Type", "application/json")
            .body(device_payload(stamp, tokens, cost).to_string())
            .send()
            .unwrap();
        assert_eq!(
            put.status().as_u16(),
            200,
            "PUT body: {}",
            put.text().unwrap()
        );
    }

    let merged: serde_json::Value = http
        .get(format!("{base}/v1/usage"))
        .header("Authorization", format!("Bearer {TOKEN}"))
        .send()
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(merged["entries"].as_array().unwrap().len(), 1);
    let entry = &merged["entries"][0];
    // Freshest device's rows win (unit-b's session row); history sums across.
    assert_eq!(entry["rows"][0]["percentLeft"], 50);
    let day30 = entry["dailyUsage"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["dayKey"] == "2026-09-30")
        .unwrap();
    assert_eq!(day30["totalTokens"], 800);
    assert_eq!(day30["costUSD"], 3.5);
    let devices = entry["remoteDevices"].as_array().unwrap();
    assert_eq!(devices.len(), 2);

    // Provider filter normalizes casing, so unknown providers return empty.
    let filtered: serde_json::Value = http
        .get(format!("{base}/v1/usage?provider=ANTIGRAVITY"))
        .header("Authorization", format!("Bearer {TOKEN}"))
        .send()
        .unwrap()
        .json()
        .unwrap();
    assert!(filtered["entries"].as_array().unwrap().is_empty());

    // Devices listing.
    let listing: serde_json::Value = http
        .get(format!("{base}/v1/devices"))
        .header("Authorization", format!("Bearer {TOKEN}"))
        .send()
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(listing["devices"].as_array().unwrap().len(), 2);

    // Error snapshots are refused so a failing fetch cannot clobber data.
    let refused = http
        .put(format!("{base}/v1/snapshot/unit-a"))
        .header("Authorization", format!("Bearer {TOKEN}"))
        .header("Content-Type", "application/json")
        .body(r#"{"ok": false, "entries": []}"#)
        .send()
        .unwrap();
    assert_eq!(refused.status().as_u16(), 422);

    // Invalid device ids are refused.
    let bad_device = http
        .put(format!("{base}/v1/snapshot/some%2Fbad%2Eid"))
        .header("Authorization", format!("Bearer {TOKEN}"))
        .header("Content-Type", "application/json")
        .body(device_payload("2026-09-30T09:00:00Z", 500, 2.0).to_string())
        .send()
        .unwrap();
    assert_eq!(bad_device.status().as_u16(), 422);

    // All asserted — remove the temp data dir.
    std::fs::remove_dir_all(&data_dir).expect("clean test data dir");
}
