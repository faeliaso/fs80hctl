use super::{TransportError, UsbTransport};
use fs80h_core::{FRAME_LEN, RawFrame};
use serde::{Deserialize, Serialize};
use std::time::Duration;

const FIXTURE_MATCH: &[u8] = include_bytes!("../../../../tests/fixtures/reference_match.png");
const FIXTURE_MISMATCH: &[u8] = include_bytes!("../../../../tests/fixtures/reference_mismatch.png");
const FIXTURE_DEGRADED: &[u8] = include_bytes!("../../../../tests/fixtures/degraded_noisy.png");

/// Simulated hardware operational scenarios for CI and headless testing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum, Default)]
#[serde(rename_all = "lowercase")]
pub enum MockScenario {
    /// Finger firmly present, matching reference template (high biometric quality).
    #[default]
    Match,

    /// Finger present, but distinct print producing biometric mismatch.
    Mismatch,

    /// Finger present, but sensor surface degraded/noisy (insufficient quality score).
    Degraded,

    /// No finger placed on sensor (empty/white frame).
    Empty,

    /// Abrupt USB hardware disconnection.
    Disconnect,
}

impl std::str::FromStr for MockScenario {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "match" => Ok(Self::Match),
            "mismatch" => Ok(Self::Mismatch),
            "degraded" => Ok(Self::Degraded),
            "empty" => Ok(Self::Empty),
            "disconnect" => Ok(Self::Disconnect),
            other => Err(format!(
                "unknown mock scenario '{other}'. Valid: match, mismatch, degraded, empty, disconnect"
            )),
        }
    }
}

/// Simulated Futronic FS80H USB transport for deterministic headless execution.
pub struct MockTransport {
    scenario: MockScenario,
    pending_buffer: Vec<u8>,
    buffer_offset: usize,
}

impl MockTransport {
    /// Instantiates a new mock transport with the designated scenario.
    pub fn new(scenario: MockScenario) -> Self {
        let mut transport = Self {
            scenario,
            pending_buffer: Vec::new(),
            buffer_offset: 0,
        };
        transport.prepare_frame();
        transport
    }

    /// Access current scenario.
    pub fn scenario(&self) -> MockScenario {
        self.scenario
    }

    /// Update scenario dynamically.
    pub fn set_scenario(&mut self, scenario: MockScenario) {
        self.scenario = scenario;
        self.prepare_frame();
    }

    fn prepare_frame(&mut self) {
        self.buffer_offset = 0;
        match self.scenario {
            MockScenario::Match => {
                if let Ok(frame) = RawFrame::from_png_bytes(FIXTURE_MATCH) {
                    self.pending_buffer = frame.pixels().to_vec();
                } else {
                    self.pending_buffer = vec![128u8; FRAME_LEN];
                }
            }
            MockScenario::Mismatch => {
                if let Ok(frame) = RawFrame::from_png_bytes(FIXTURE_MISMATCH) {
                    self.pending_buffer = frame.pixels().to_vec();
                } else {
                    self.pending_buffer = vec![100u8; FRAME_LEN];
                }
            }
            MockScenario::Degraded => {
                if let Ok(frame) = RawFrame::from_png_bytes(FIXTURE_DEGRADED) {
                    let mut pixels = frame.pixels().to_vec();
                    // Inject saturation noise so saturated_ratio exceeds tolerated limit
                    for (i, p) in pixels.iter_mut().enumerate() {
                        if i % 3 == 0 {
                            *p = 0;
                        }
                    }
                    self.pending_buffer = pixels;
                } else {
                    self.pending_buffer = vec![0u8; FRAME_LEN];
                }
            }
            MockScenario::Empty => {
                // Sensor saturated white (zero ridge contact, presence rejected)
                self.pending_buffer = vec![255u8; FRAME_LEN];
            }
            MockScenario::Disconnect => {
                self.pending_buffer.clear();
            }
        }
    }
}

impl UsbTransport for MockTransport {
    fn write_bulk(
        &mut self,
        _endpoint: u8,
        data: &[u8],
        _timeout: Duration,
    ) -> Result<usize, TransportError> {
        if self.scenario == MockScenario::Disconnect {
            return Err(TransportError::DeviceDisconnected);
        }

        self.prepare_frame();
        Ok(data.len())
    }

    fn read_bulk(
        &mut self,
        _endpoint: u8,
        buffer: &mut [u8],
        _timeout: Duration,
    ) -> Result<usize, TransportError> {
        if self.scenario == MockScenario::Disconnect {
            return Err(TransportError::DeviceDisconnected);
        }

        if self.buffer_offset >= self.pending_buffer.len() {
            return Ok(0);
        }

        let remaining = self.pending_buffer.len() - self.buffer_offset;
        let to_copy = remaining.min(buffer.len());
        buffer[..to_copy].copy_from_slice(
            &self.pending_buffer[self.buffer_offset..self.buffer_offset + to_copy],
        );
        self.buffer_offset += to_copy;
        Ok(to_copy)
    }

    fn control_transfer(
        &mut self,
        _request_type: u8,
        _request: u8,
        _value: u16,
        _index: u16,
        _data: &mut [u8],
        _timeout: Duration,
    ) -> Result<usize, TransportError> {
        if self.scenario == MockScenario::Disconnect {
            return Err(TransportError::DeviceDisconnected);
        }
        Ok(0)
    }
}
