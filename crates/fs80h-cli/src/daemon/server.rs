use super::protocol::{ClientCommand, ServerError, ServerStatus};
use super::worker::{HardwareActor, WorkerTask};
use fs80h_driver::MockScenario;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{mpsc, oneshot};

pub struct DaemonConfig {
    pub socket_path: PathBuf,
    pub mock: Option<MockScenario>,
    #[allow(dead_code)]
    pub systemd_activation: bool,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        let default_path = if cfg!(target_os = "linux") {
            PathBuf::from("/run/fs80h/fs80h.sock")
        } else {
            PathBuf::from("/tmp/fs80h.sock")
        };

        Self {
            socket_path: default_path,
            mock: None,
            systemd_activation: false,
        }
    }
}

pub fn default_socket_path() -> PathBuf {
    if let Ok(env_path) = std::env::var("FS80H_SOCKET_PATH") {
        return PathBuf::from(env_path);
    }
    if cfg!(target_os = "linux") {
        PathBuf::from("/run/fs80h/fs80h.sock")
    } else {
        PathBuf::from("/tmp/fs80h.sock")
    }
}

pub async fn run_daemon(
    config: DaemonConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let listener = setup_listener(&config)?;
    tracing::info!(
        "fs80hctl capture daemon listening on {:?}",
        config.socket_path
    );

    let actor = Arc::new(HardwareActor::spawn(config.mock));
    let socket_path = config.socket_path.clone();

    tokio::select! {
        _ = accept_loop(listener, Arc::clone(&actor)) => {},
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("shutdown signal received, terminating daemon gracefully");
        }
    }

    if socket_path.exists() {
        let _ = std::fs::remove_file(&socket_path);
    }

    Ok(())
}

fn setup_listener(
    config: &DaemonConfig,
) -> Result<UnixListener, Box<dyn std::error::Error + Send + Sync>> {
    #[cfg(target_os = "linux")]
    if config.systemd_activation
        && let Ok(fds) = std::env::var("LISTEN_FDS")
        && let Ok(count) = fds.parse::<i32>()
        && count > 0
    {
        use std::os::unix::io::FromRawFd;
        tracing::info!("inheriting socket from systemd activation (fd: 3)");
        let std_listener = unsafe { std::os::unix::net::UnixListener::from_raw_fd(3) };
        std_listener.set_nonblocking(true)?;
        return Ok(UnixListener::from_std(std_listener)?);
    }

    let socket_path = &config.socket_path;
    if let Some(parent) = socket_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty() && !p.exists())
    {
        let _ = std::fs::create_dir_all(parent);
    }

    if socket_path.exists() {
        let _ = std::fs::remove_file(socket_path);
    }

    let listener = UnixListener::bind(socket_path)?;

    // Restrict POSIX socket permissions to 0660
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(socket_path, std::fs::Permissions::from_mode(0o660));
    }

    Ok(listener)
}

async fn accept_loop(listener: UnixListener, actor: Arc<HardwareActor>) {
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let actor_clone = Arc::clone(&actor);
                tokio::spawn(async move {
                    if let Err(err) = handle_client(stream, actor_clone).await {
                        tracing::debug!("client connection closed: {err}");
                    }
                });
            }
            Err(err) => {
                tracing::warn!("accept error: {err}");
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    }
}

pub async fn handle_client(
    mut stream: UnixStream,
    actor: Arc<HardwareActor>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (reader, mut writer) = stream.split();
    let mut lines = BufReader::new(reader).lines();

    while let Some(line) = lines.next_line().await? {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let command: ClientCommand = match serde_json::from_str(trimmed) {
            Ok(cmd) => cmd,
            Err(err) => {
                let err_resp = ServerError {
                    status: "error".into(),
                    message: format!("invalid JSON command: {err}"),
                };
                let mut data = serde_json::to_string(&err_resp)?;
                data.push('\n');
                writer.write_all(data.as_bytes()).await?;
                continue;
            }
        };

        match command {
            ClientCommand::Ping => {
                let status = ServerStatus {
                    status: "pong".into(),
                    device_ready: Some(true),
                    version: Some(env!("CARGO_PKG_VERSION").into()),
                    captures_count: None,
                };
                let mut out = serde_json::to_string(&status)?;
                out.push('\n');
                writer.write_all(out.as_bytes()).await?;
            }

            ClientCommand::Status => {
                let status = ServerStatus {
                    status: "ok".into(),
                    device_ready: Some(true),
                    version: Some(env!("CARGO_PKG_VERSION").into()),
                    captures_count: Some(actor.captures_count()),
                };
                let mut out = serde_json::to_string(&status)?;
                out.push('\n');
                writer.write_all(out.as_bytes()).await?;
            }

            ClientCommand::Cancel => {
                let resp = ServerStatus {
                    status: "ok".into(),
                    device_ready: Some(!actor.is_busy()),
                    version: None,
                    captures_count: None,
                };
                let mut out = serde_json::to_string(&resp)?;
                out.push('\n');
                writer.write_all(out.as_bytes()).await?;
            }

            ClientCommand::Capture {
                timeout_ms,
                min_quality,
            } => {
                if actor.is_busy() {
                    let busy_resp = ServerError {
                        status: "busy".into(),
                        message: "Scanner is busy with another capture".into(),
                    };
                    let mut out = serde_json::to_string(&busy_resp)?;
                    out.push('\n');
                    writer.write_all(out.as_bytes()).await?;
                    continue;
                }

                let (event_tx, mut event_rx) = mpsc::channel(16);
                let (result_tx, result_rx) = oneshot::channel();

                let task = WorkerTask::Capture {
                    timeout: Duration::from_millis(timeout_ms),
                    min_quality,
                    event_tx,
                    result_tx,
                };

                if actor.task_sender().send(task).await.is_err() {
                    let err_resp = ServerError {
                        status: "error".into(),
                        message: "failed to dispatch task to hardware worker".into(),
                    };
                    let mut out = serde_json::to_string(&err_resp)?;
                    out.push('\n');
                    writer.write_all(out.as_bytes()).await?;
                    continue;
                }

                // Stream intermediate events to client socket
                let writer_task = async {
                    while let Some(event) = event_rx.recv().await {
                        let mut msg = serde_json::to_string(&event)?;
                        msg.push('\n');
                        writer.write_all(msg.as_bytes()).await?;
                        writer.flush().await?;
                    }
                    Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
                };

                let (_, final_res) = tokio::join!(writer_task, result_rx);

                match final_res {
                    Ok(Ok(_)) => {}
                    Ok(Err(err)) => {
                        let err_resp = ServerError {
                            status: "error".into(),
                            message: err,
                        };
                        let mut out = serde_json::to_string(&err_resp)?;
                        out.push('\n');
                        let _ = writer.write_all(out.as_bytes()).await;
                    }
                    Err(_) => {
                        let err_resp = ServerError {
                            status: "error".into(),
                            message: "hardware worker dropped result channel".into(),
                        };
                        let mut out = serde_json::to_string(&err_resp)?;
                        out.push('\n');
                        let _ = writer.write_all(out.as_bytes()).await;
                    }
                }
            }
        }
    }

    Ok(())
}
