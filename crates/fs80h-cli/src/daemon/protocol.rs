use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "lowercase")]
pub enum ClientCommand {
    Ping,
    Status,
    Capture {
        #[serde(default = "default_timeout_ms")]
        timeout_ms: u64,
        #[serde(default = "default_min_quality")]
        min_quality: u8,
    },
    Cancel,
}

fn default_timeout_ms() -> u64 {
    5000
}

fn default_min_quality() -> u8 {
    50
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum ServerEvent {
    WaitingFinger,
    FingerDetected {
        area_coverage: u8,
        #[serde(skip_serializing_if = "Option::is_none")]
        score: Option<u8>,
    },
    Completed {
        quality: u8,
        width: u32,
        height: u32,
        image_base64: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        nfiq2: Option<u8>,
        sha256_raw_frame: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerStatus {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_ready: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub captures_count: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerError {
    pub status: String,
    pub message: String,
}
