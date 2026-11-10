use super::protocol::ServerEvent;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use fs80h_core::{HEIGHT, WIDTH, evaluate_quality};
use fs80h_driver::{FutronicScanner, MockScenario, MockTransport};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

#[allow(dead_code)]
pub struct CompletedCapture {
    pub quality: u8,
    pub width: u32,
    pub height: u32,
    pub image_base64: String,
    pub nfiq2: Option<u8>,
    pub sha256_raw_frame: String,
}

pub enum WorkerTask {
    #[allow(dead_code)]
    Ping { resp_tx: oneshot::Sender<bool> },
    #[allow(dead_code)]
    Status {
        resp_tx: oneshot::Sender<WorkerTelemetry>,
    },
    Capture {
        timeout: Duration,
        min_quality: u8,
        event_tx: mpsc::Sender<ServerEvent>,
        result_tx: oneshot::Sender<Result<CompletedCapture, String>>,
    },
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct WorkerTelemetry {
    pub device_ready: bool,
    pub captures_count: usize,
    pub version: String,
}

pub struct HardwareActor {
    task_tx: mpsc::Sender<WorkerTask>,
    is_busy: Arc<AtomicBool>,
    captures_count: Arc<AtomicUsize>,
}

impl HardwareActor {
    pub fn spawn(mock: Option<MockScenario>) -> Self {
        let (task_tx, mut task_rx) = mpsc::channel::<WorkerTask>(16);
        let is_busy = Arc::new(AtomicBool::new(false));
        let captures_count = Arc::new(AtomicUsize::new(0));

        let is_busy_clone = Arc::clone(&is_busy);
        let captures_count_clone = Arc::clone(&captures_count);

        std::thread::Builder::new()
            .name("fs80h-hardware-worker".into())
            .spawn(move || {
                let mut scanner = match mock {
                    Some(scenario) => FutronicScanner::new_mock(MockTransport::new(scenario)),
                    None => FutronicScanner::open_first().ok().unwrap_or_else(|| {
                        FutronicScanner::new_mock(MockTransport::new(MockScenario::Disconnect))
                    }),
                };

                while let Some(task) = task_rx.blocking_recv() {
                    match task {
                        WorkerTask::Ping { resp_tx } => {
                            let _ = resp_tx.send(true);
                        }
                        WorkerTask::Status { resp_tx } => {
                            let telemetry = WorkerTelemetry {
                                device_ready: true,
                                captures_count: captures_count_clone.load(Ordering::Relaxed),
                                version: env!("CARGO_PKG_VERSION").into(),
                            };
                            let _ = resp_tx.send(telemetry);
                        }
                        WorkerTask::Capture {
                            timeout,
                            min_quality,
                            event_tx,
                            result_tx,
                        } => {
                            is_busy_clone.store(true, Ordering::SeqCst);
                            let _ = event_tx.blocking_send(ServerEvent::WaitingFinger);

                            let event_tx_clone = event_tx.clone();
                            let capture_result = scanner.wait_for_finger(
                                timeout,
                                min_quality,
                                None,
                                |presence, _elapsed| {
                                    let coverage = (presence.contact_ratio * 100.0).round() as u8;
                                    let _ =
                                        event_tx_clone.blocking_send(ServerEvent::FingerDetected {
                                            area_coverage: coverage,
                                            score: Some(presence.quality.score),
                                        });
                                },
                            );

                            match capture_result {
                                Ok(raw_frame) => {
                                    let quality = evaluate_quality(&raw_frame);
                                    let sha256_raw_frame = raw_frame.sha256();
                                    let image_base64 = BASE64_STANDARD.encode(raw_frame.pixels());

                                    captures_count_clone.fetch_add(1, Ordering::Relaxed);

                                    let completed = CompletedCapture {
                                        quality: quality.score,
                                        width: WIDTH,
                                        height: HEIGHT,
                                        image_base64: image_base64.clone(),
                                        nfiq2: None,
                                        sha256_raw_frame: sha256_raw_frame.clone(),
                                    };

                                    let _ = event_tx.blocking_send(ServerEvent::Completed {
                                        quality: quality.score,
                                        width: WIDTH,
                                        height: HEIGHT,
                                        image_base64,
                                        nfiq2: None,
                                        sha256_raw_frame,
                                    });

                                    let _ = result_tx.send(Ok(completed));
                                }
                                Err(err) => {
                                    let _ = result_tx.send(Err(err.to_string()));
                                }
                            }

                            is_busy_clone.store(false, Ordering::SeqCst);
                        }
                    }
                }
            })
            .expect("failed to spawn hardware worker thread");

        Self {
            task_tx,
            is_busy,
            captures_count,
        }
    }

    pub fn is_busy(&self) -> bool {
        self.is_busy.load(Ordering::SeqCst)
    }

    pub fn task_sender(&self) -> mpsc::Sender<WorkerTask> {
        self.task_tx.clone()
    }

    pub fn captures_count(&self) -> usize {
        self.captures_count.load(Ordering::Relaxed)
    }
}
