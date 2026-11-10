//! Protocol packet definitions and safe parsing for Futronic FS80H USB commands.

use thiserror::Error;

/// FS80H USB command opcodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CommandOpcode {
    GetImage = 0x20,
    SetInvert = 0x21,
    GetImageVariableDose = 0x6A,
}

impl CommandOpcode {
    pub fn from_u8(val: u8) -> Option<Self> {
        match val {
            0x20 => Some(Self::GetImage),
            0x21 => Some(Self::SetInvert),
            0x6A => Some(Self::GetImageVariableDose),
            _ => None,
        }
    }
}

/// Structured USB command sent from host to scanner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsbCommand {
    pub opcode: CommandOpcode,
    pub parameter: Option<u8>,
}

impl UsbCommand {
    pub fn new(opcode: CommandOpcode, parameter: Option<u8>) -> Self {
        Self { opcode, parameter }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        match self.parameter {
            Some(p) => vec![self.opcode as u8, p],
            None => vec![self.opcode as u8],
        }
    }
}

/// Decoded USB response packet from device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponsePacket {
    pub status_code: u8,
    pub payload: Vec<u8>,
}

/// Errors occurring during USB protocol parsing.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProtocolError {
    #[error("packet truncated: expected at least {expected} bytes, got {actual}")]
    Truncated { expected: usize, actual: usize },

    #[error("invalid header byte: 0x{0:02X}")]
    InvalidHeader(u8),

    #[error("checksum mismatch: expected 0x{expected:02X}, calculated 0x{calculated:02X}")]
    ChecksumMismatch { expected: u8, calculated: u8 },

    #[error("unknown opcode byte: 0x{0:02X}")]
    UnknownOpcode(u8),
}

/// Parses a raw USB response packet fallibly without panics or unchecked indexing.
pub fn parse_response_packet(buf: &[u8]) -> Result<ResponsePacket, ProtocolError> {
    let header = buf.first().copied().ok_or(ProtocolError::Truncated {
        expected: 1,
        actual: 0,
    })?;

    if header == 0xFF {
        return Err(ProtocolError::InvalidHeader(header));
    }

    if buf.len() < 2 {
        return Err(ProtocolError::Truncated {
            expected: 2,
            actual: buf.len(),
        });
    }

    let status_code = *buf.first().unwrap_or(&0);
    let payload_len = *buf.get(1).unwrap_or(&0) as usize;

    let total_expected = 2 + payload_len + 1; // status + len + payload + checksum
    if buf.len() < total_expected {
        return Err(ProtocolError::Truncated {
            expected: total_expected,
            actual: buf.len(),
        });
    }

    let payload = buf
        .get(2..2 + payload_len)
        .ok_or(ProtocolError::Truncated {
            expected: 2 + payload_len,
            actual: buf.len(),
        })?
        .to_vec();

    let expected_checksum = *buf.get(2 + payload_len).ok_or(ProtocolError::Truncated {
        expected: total_expected,
        actual: buf.len(),
    })?;

    let calculated_checksum = payload
        .iter()
        .fold(status_code ^ (payload_len as u8), |acc, &b| acc ^ b);
    if expected_checksum != calculated_checksum {
        return Err(ProtocolError::ChecksumMismatch {
            expected: expected_checksum,
            calculated: calculated_checksum,
        });
    }

    Ok(ResponsePacket {
        status_code,
        payload,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn test_valid_packet_roundtrip() {
        let status = 0x00;
        let payload = vec![0x10, 0x20, 0x30];
        let len = payload.len() as u8;
        let checksum = payload.iter().fold(status ^ len, |acc, &b| acc ^ b);

        let mut packet_bytes = vec![status, len];
        packet_bytes.extend_from_slice(&payload);
        packet_bytes.push(checksum);

        let parsed = parse_response_packet(&packet_bytes).expect("parse valid packet");
        assert_eq!(parsed.status_code, status);
        assert_eq!(parsed.payload, payload);
    }

    proptest! {
        #[test]
        fn test_arbitrary_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..1024)) {
            // Must return Ok or Err, never panic
            let _ = parse_response_packet(&bytes);
        }
    }
}
