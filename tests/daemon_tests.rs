use std::process::{Child, Command, Stdio};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

fn bin() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_fs80hctl"));
    command.arg("--no-config");
    command
}

struct DaemonHarness {
    child: Child,
    socket_path: std::path::PathBuf,
    _temp_dir: tempfile::TempDir,
}

impl DaemonHarness {
    fn start(scenario: &str) -> Self {
        Self::start_with_config(scenario, false)
    }

    fn start_with_config(scenario: &str, configured: bool) -> Self {
        let temp_dir = tempfile::tempdir().expect("create temp dir");
        let socket_path = temp_dir.path().join("test_fs80h.sock");

        let mut command = if configured {
            let config = temp_dir.path().join("config.toml");
            std::fs::write(
                &config,
                format!(
                    "schema_version=1\n[driver]\npoll_interval_ms=7\n[daemon]\nsocket_path='{}'\n",
                    socket_path.display()
                ),
            )
            .unwrap();
            let mut command = Command::new(env!("CARGO_BIN_EXE_fs80hctl"));
            command
                .arg("--config")
                .arg(config)
                .env_remove("FS80H_SOCKET_PATH");
            command
        } else {
            let mut command = bin();
            command
                .args(["--mock", scenario, "serve", "--socket-path"])
                .arg(&socket_path);
            command
        };
        if configured {
            command.args(["--mock", scenario, "serve"]);
        }
        let mut child = command
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn daemon process");

        // Wait until socket file exists and is ready
        let start = std::time::Instant::now();
        while start.elapsed() < Duration::from_secs(3) {
            if let Some(status) = child.try_wait().expect("check daemon status") {
                use std::io::Read;
                let mut stderr = String::new();
                child
                    .stderr
                    .take()
                    .unwrap()
                    .read_to_string(&mut stderr)
                    .unwrap();
                panic!("daemon exited before becoming ready ({status}): {stderr}");
            }
            if socket_path.exists() {
                // Try connecting
                if std::os::unix::net::UnixStream::connect(&socket_path).is_ok() {
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(50));
        }

        Self {
            child,
            socket_path,
            _temp_dir: temp_dir,
        }
    }
}

impl Drop for DaemonHarness {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if self.socket_path.exists() {
            let _ = std::fs::remove_file(&self.socket_path);
        }
    }
}

#[tokio::test]
async fn test_daemon_ping_and_status() {
    let daemon = DaemonHarness::start("match");

    let mut stream = UnixStream::connect(&daemon.socket_path)
        .await
        .expect("connect to daemon");

    // 1. Send Ping
    stream
        .write_all(b"{\"command\":\"ping\"}\n")
        .await
        .expect("write ping");

    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    let line = lines
        .next_line()
        .await
        .expect("read response")
        .expect("line exists");
    let ping_parsed: serde_json::Value =
        serde_json::from_str(&line).expect("valid ping json response");
    assert_eq!(ping_parsed["status"], "pong");
    assert_eq!(ping_parsed["device_ready"], true);

    // 2. Send Status
    writer
        .write_all(b"{\"command\":\"status\"}\n")
        .await
        .expect("write status");

    let line = lines
        .next_line()
        .await
        .expect("read status response")
        .expect("line exists");
    let status_parsed: serde_json::Value =
        serde_json::from_str(&line).expect("valid status json response");
    assert_eq!(status_parsed["status"], "ok");
    assert_eq!(status_parsed["device_ready"], true);
    assert!(status_parsed.get("version").is_some());
}

#[tokio::test]
async fn test_daemon_capture_streaming_and_completion() {
    let daemon = DaemonHarness::start("match");

    let mut stream = UnixStream::connect(&daemon.socket_path)
        .await
        .expect("connect to daemon");

    stream
        .write_all(b"{\"command\":\"capture\",\"timeout_ms\":2500,\"min_quality\":50}\n")
        .await
        .expect("send capture command");

    let (reader, _) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    let mut received_waiting = false;
    let mut completed_event = None;

    while let Some(line) = lines.next_line().await.expect("read line") {
        let parsed: serde_json::Value = serde_json::from_str(&line).expect("valid json event");
        if let Some(event) = parsed.get("event").and_then(|e| e.as_str()) {
            match event {
                "waiting_finger" => {
                    received_waiting = true;
                }
                "completed" => {
                    completed_event = Some(parsed);
                    break;
                }
                _ => {}
            }
        }
    }

    assert!(received_waiting, "should receive waiting_finger event");
    assert!(completed_event.is_some(), "should receive completed event");

    let completed = completed_event.unwrap();
    assert_eq!(completed["width"], 320);
    assert_eq!(completed["height"], 480);
    assert!(completed["quality"].as_u64().unwrap() >= 50);

    let b64 = completed["image_base64"]
        .as_str()
        .expect("image_base64 string");
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
    let decoded = BASE64_STANDARD
        .decode(b64.as_bytes())
        .expect("decode base64");
    assert_eq!(decoded.len(), 153_600);
}

#[tokio::test]
async fn test_daemon_concurrency_busy_rejection() {
    let daemon = DaemonHarness::start("empty"); // empty scenario will wait until timeout

    // Client 1 sends capture
    let mut stream1 = UnixStream::connect(&daemon.socket_path)
        .await
        .expect("client 1 connect");
    stream1
        .write_all(b"{\"command\":\"capture\",\"timeout_ms\":2000,\"min_quality\":50}\n")
        .await
        .expect("client 1 send capture");

    let (reader1, _) = stream1.into_split();
    let mut lines1 = BufReader::new(reader1).lines();

    // Read first event from client 1
    let first_line = lines1.next_line().await.unwrap().unwrap();
    assert!(first_line.contains("waiting_finger"));

    // Client 2 connects concurrently and attempts capture
    let mut stream2 = UnixStream::connect(&daemon.socket_path)
        .await
        .expect("client 2 connect");
    stream2
        .write_all(b"{\"command\":\"capture\",\"timeout_ms\":1000,\"min_quality\":50}\n")
        .await
        .expect("client 2 send capture");

    let (reader2, _) = stream2.into_split();
    let mut lines2 = BufReader::new(reader2).lines();

    let busy_line = lines2.next_line().await.unwrap().unwrap();
    let busy_parsed: serde_json::Value = serde_json::from_str(&busy_line).expect("valid json busy");
    assert_eq!(busy_parsed["status"], "busy");
    assert!(
        busy_parsed["message"]
            .as_str()
            .unwrap()
            .contains("Scanner is busy with another capture")
    );
}

#[test]
fn test_cli_client_capture_command() {
    let daemon = DaemonHarness::start("match");
    let tmp_dir = tempfile::tempdir().expect("create temp dir");
    let capture_out = tmp_dir.path().join("daemon_client_capture.png");

    let out = bin()
        .args([
            "capture",
            "--client",
            "--socket",
            daemon.socket_path.to_str().unwrap(),
            "--output",
            capture_out.to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .expect("run capture --client");

    assert_eq!(out.status.code(), Some(0));
    assert!(capture_out.exists());
    assert!(
        tmp_dir
            .path()
            .join("daemon_client_capture.png.meta.json")
            .exists()
    );

    let parsed: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("valid client capture json");
    assert_eq!(parsed["format"], "png");
    assert_eq!(parsed["daemon"], true);
}

#[test]
fn configured_daemon_and_environment_socket_capture() {
    let daemon = DaemonHarness::start_with_config("match", true);
    let output_path = daemon._temp_dir.path().join("configured.png");
    let output = bin()
        .env("FS80H_SOCKET_PATH", &daemon.socket_path)
        .args(["capture", "--client", "--json", "--output"])
        .arg(&output_path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output_path.exists());
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["daemon"], true);
}
