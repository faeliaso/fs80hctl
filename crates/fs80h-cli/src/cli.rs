use clap::{Args, Parser, Subcommand, ValueEnum};
use fs80h_core::ImageFormat;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    #[default]
    Text,
    Json,
    Raw,
}

impl std::fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Text => write!(f, "text"),
            Self::Json => write!(f, "json"),
            Self::Raw => write!(f, "raw"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CliExitCode {
    Success = 0,
    OperationalError = 1,
    BiometricMismatch = 2,
    CaptureTimeout = 3,
    QualityInsufficient = 4,
    HardwareFailure = 5,
}

impl CliExitCode {
    pub fn as_u8(self) -> u8 {
        self as u8
    }
}

impl From<CliExitCode> for std::process::ExitCode {
    fn from(code: CliExitCode) -> Self {
        std::process::ExitCode::from(code.as_u8())
    }
}

impl std::process::Termination for CliExitCode {
    fn report(self) -> std::process::ExitCode {
        self.into()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error("{0}")]
    Operational(String),
    #[error("biometric mismatch: score {score:.1} is below threshold {threshold:.1}")]
    Mismatch { score: f32, threshold: f32 },
    #[error("capture timed out: {0}")]
    Timeout(String),
    #[allow(dead_code)]
    #[error("insufficient biometric quality: score {score} (threshold {min_quality})")]
    QualityRejected { score: u8, min_quality: u8 },
    #[error("hardware error: {0}")]
    Hardware(String),
}

impl CliError {
    pub fn exit_code(&self) -> CliExitCode {
        match self {
            Self::Operational(_) => CliExitCode::OperationalError,
            Self::Mismatch { .. } => CliExitCode::BiometricMismatch,
            Self::Timeout(_) => CliExitCode::CaptureTimeout,
            Self::QualityRejected { .. } => CliExitCode::QualityInsufficient,
            Self::Hardware(_) => CliExitCode::HardwareFailure,
        }
    }
}

#[derive(Parser, Debug)]
#[command(
    name = "fs80hctl",
    version,
    about = "Technical control tool for Futronic FS80/FS80H fingerprint scanners",
    long_about = "Technical control CLI for Futronic FS80/FS80H optical fingerprint scanners, supporting frame acquisition, presence detection, multi-sample enrollment, SHA-256 chain of custody, forensic transcoding (PNG, TIFF 500 DPI, NIST PGM, RAW), image inspection, and 1:1/1:N biometric verification."
)]
pub struct Cli {
    /// Global output format: text (human-readable), json (structured payload), or raw (pure byte stream)
    #[arg(long, global = true, value_enum, default_value = "text")]
    pub format: OutputFormat,

    /// Output results in structured JSON format (shorthand for --format json)
    #[arg(long, global = true)]
    pub json: bool,

    /// Increase logging verbosity (-v for INFO, -vv for DEBUG, -vvv for TRACE)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,

    #[command(subcommand)]
    pub command: Command,
}

impl Cli {
    pub fn resolved_format(&self) -> OutputFormat {
        if self.json {
            OutputFormat::Json
        } else {
            self.format
        }
    }
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// List connected Futronic fingerprint scanners via USB
    Devices,
    /// Device inspection and low-level diagnostic operations
    Device {
        #[command(subcommand)]
        command: DeviceCommand,
    },
    /// Capture an optical biometric frame from the scanner
    Capture(CaptureArgs),
    /// Acquire multiple samples and elect the highest quality frame (best-of-N)
    Enroll(EnrollArgs),
    /// Transcode biometric images across forensic formats (PNG, TIFF, PGM, RAW)
    Convert(ConvertArgs),
    /// Inspect image dimensions, DPI, SHA-256 integrity, quality metrics, and custody manifest
    Inspect(InspectArgs),
    /// Compare biometric images (1:1 with reference file or 1:N with --gallery directory)
    Verify(VerifyArgs),
    /// Run environment diagnostics (USB subsystem, device permissions, udev rules, groups, NFIQ2)
    Doctor,
    /// Display technical manual and quick reference guide
    Manual,
}

#[derive(Subcommand, Debug)]
pub enum DeviceCommand {
    /// Inspect USB communication with the selected scanner
    Inspect(Selection),
}

#[derive(Args, Clone, Debug)]
pub struct Selection {
    /// USB bus number (e.g., 1)
    #[arg(long)]
    pub bus: Option<u8>,
    /// USB device address (e.g., 2)
    #[arg(long)]
    pub address: Option<u8>,
}

#[derive(Args, Debug)]
pub struct CaptureArgs {
    #[command(flatten)]
    pub selection: Selection,
    /// Output file path (default: capture.png)
    #[arg(short, long, default_value = "capture.png")]
    pub output: PathBuf,
    /// Target image encoding format (png, tiff, pgm, raw). Inferred from output extension if omitted.
    #[arg(long = "image-format", visible_alias = "img-format")]
    pub image_format: Option<ImageFormat>,
    /// Save raw optical sensor frame without ridge inversion (light ridges on dark background)
    #[arg(long)]
    pub raw: bool,
    /// Wait for finger presence and contact stabilization before acquiring
    #[arg(long)]
    pub wait: bool,
    /// Timeout in seconds when waiting for finger presence (default: 10)
    #[arg(long, default_value_t = 10)]
    pub timeout: u64,
    /// Minimum quality score threshold to trigger capture when waiting (0 to 100, default: 50)
    #[arg(long, default_value_t = 50)]
    pub min_quality: u8,
    /// Use direct variable-dose acquisition path instead of default frame mode
    #[arg(long)]
    pub variable_dose: bool,
    /// Exposure dose used with --variable-dose (default: 166)
    #[arg(long, default_value_t = 166)]
    pub dose: u8,
}

#[derive(Args, Debug)]
pub struct EnrollArgs {
    #[command(flatten)]
    pub selection: Selection,
    /// Output file path (default: enroll.png)
    #[arg(short, long, default_value = "enroll.png")]
    pub output: PathBuf,
    /// Target image encoding format (png, tiff, pgm, raw). Inferred from output extension if omitted.
    #[arg(long = "image-format", visible_alias = "img-format")]
    pub image_format: Option<ImageFormat>,
    /// Number of biometric samples to acquire and evaluate (default: 3)
    #[arg(long, default_value_t = 3)]
    pub samples: usize,
    /// Timeout in seconds per sample when waiting for finger (default: 10)
    #[arg(long, default_value_t = 10)]
    pub timeout: u64,
    /// Minimum quality score threshold per sample (0 to 100, default: 50)
    #[arg(long, default_value_t = 50)]
    pub min_quality: u8,
    /// Save raw optical sensor frame without ridge inversion (light ridges on dark background)
    #[arg(long)]
    pub raw: bool,
    /// Use direct variable-dose acquisition path instead of default frame mode
    #[arg(long)]
    pub variable_dose: bool,
    /// Exposure dose used with --variable-dose (default: 166)
    #[arg(long, default_value_t = 166)]
    pub dose: u8,
}

#[derive(Args, Debug)]
pub struct ConvertArgs {
    /// Input image file (PNG, TIFF, PGM, RAW)
    pub input: PathBuf,
    /// Target format (png, tiff, pgm, raw). Inferred from output extension if omitted.
    #[arg(short = 't', long)]
    pub to: Option<ImageFormat>,
    /// Output file path
    #[arg(short, long)]
    pub output: Option<PathBuf>,
    /// Invert pixel polarity (255 - pixel)
    #[arg(long)]
    pub invert: bool,
}

#[derive(Args, Debug)]
pub struct InspectArgs {
    /// Biometric image file to inspect (PNG, TIFF, PGM, RAW)
    pub image: PathBuf,
}

#[derive(Args, Debug)]
pub struct VerifyArgs {
    /// Probe / sample image file (PNG, TIFF, PGM, RAW)
    pub probe: PathBuf,
    /// Reference / template image file for 1:1 verification
    pub reference: Option<PathBuf>,
    /// Directory containing gallery of reference images for 1:N comparison
    #[arg(long)]
    pub gallery: Option<PathBuf>,
    /// Minimum similarity threshold for verification (0 to 100)
    #[arg(long, default_value_t = 40.0)]
    pub threshold: f32,
    /// Exit with code 2 if threshold is not met (biometric mismatch)
    #[arg(long)]
    pub exit_code: bool,
}

#[derive(Serialize, Debug)]
pub struct VerifyOutput {
    pub deterministic: fs80h_core::MatchReport,
    pub threshold: f32,
    pub threshold_met: bool,
    pub biometric_score: Option<f32>,
}

pub const MANUAL_TEXT: &str = r#"fs80hctl(1) - Futronic FS80/FS80H Technical Control Manual

NAME
    fs80hctl - Technical acquisition, custody, and processing utility for Futronic FS80/FS80H scanners

SYNOPSIS
    fs80hctl [GLOBAL_OPTIONS] <COMMAND> [ARGUMENTS...]

DESCRIPTION
    fs80hctl is a pure-Rust command-line tool for technical and forensic operations on
    Futronic optical fingerprint scanners (FS80, FS80H, and FS81H). It provides direct optical
    sensor acquisition, biometric standard compliance (500 DPI), image quality metrics
    (local variance and saturation), format transcoding with immutable chain of custody
    (SHA-256 manifests), and deterministic 1:1 biometric comparison.

GLOBAL OPTIONS
    --format <text|json|raw>
        Output format: text (human-readable), json (structured JSON payload), or raw (pure byte stream).
        Default: text.

    --json
        Shorthand alias for --format json. Output results in structured JSON format.

    -v, --verbose
        Increase logging verbosity on stderr (-v for INFO, -vv for DEBUG, -vvv for TRACE).
        Also configurable via RUST_LOG environment variable.

    -h, --help
        Print quick help message or subcommand syntax.

    -V, --version
        Print installed fs80hctl version.

SUBCOMMANDS
    doctor
        Verify runtime environment: USB subsystem (libusb), scanner access permissions,
        udev rules (Linux), user groups, external NFIQ2 availability, and working directory write.
        Examples:
            fs80hctl doctor
            fs80hctl --format json doctor

    devices
        List all connected Futronic FS80/FS80H/FS81H scanners on the USB bus.
        Examples:
            fs80hctl devices
            fs80hctl --format json devices

    device inspect [--bus <N>] [--address <N>]
        Open specified device (or first available) and inspect USB communication.
        Examples:
            fs80hctl device inspect
            fs80hctl device inspect --bus 1 --address 4

    capture [OPTIONS]
        Acquire a biometric frame from optical sensor (320x480 pixels @ 500 DPI, L8 grayscale).
        Writes image file and adjacent chain-of-custody manifest (.meta.json).
        With --format raw, streams raw 153,600 sensor bytes directly to stdout for Unix pipes.
        Capture options:
            -o, --output <FILE>          Output path (default: capture.png)
            --image-format <FORMAT>      Image format: png, tiff, pgm, raw
            --raw                        Save raw optical sensor frame without ridge inversion
            --wait                       Wait for finger presence and contact stabilization before acquiring
            --timeout <SECS>             Timeout in seconds when waiting for finger presence (default: 10)
            --min-quality <0-100>        Minimum quality score threshold to trigger capture (default: 50)
            --variable-dose              Direct variable-dose acquisition path
            --dose <VALUE>               Exposure dose (default: 166)
            --bus <N> / --address <N>    Select specific USB device
        Examples:
            fs80hctl capture --output digital.png
            fs80hctl capture --wait --timeout 15 --output digital.png
            fs80hctl capture --format raw | imagemagick -
            fs80hctl capture --image-format tiff --output digital.tiff

    enroll [OPTIONS]
        Acquire multiple biometric samples consecutively, evaluate quality metrics, and
        automatically elect and persist the highest quality frame (best-of-N).
        Enroll options:
            -o, --output <FILE>          Output path (default: enroll.png)
            --image-format <FORMAT>      Image format: png, tiff, pgm, raw
            --samples <N>                Number of samples to acquire and evaluate (default: 3)
            --timeout <SECS>             Timeout in seconds per sample (default: 10)
            --min-quality <0-100>        Minimum quality score threshold per sample (default: 50)
            --raw                        Save raw optical sensor frame without ridge inversion
            --bus <N> / --address <N>    Select specific USB device
        Examples:
            fs80hctl enroll --output master.png
            fs80hctl enroll --samples 3 --timeout 15 --image-format tiff --output master.tiff

    convert <INPUT> [OPTIONS]
        Transcode biometric images between formats, preserving root custody SHA-256 in the
        adjacent .meta.json manifest.
        Conversion options:
            -t, --to <FORMAT>            Target format: png, tiff, pgm, raw
            -o, --output <FILE>          Output file path
            --invert                     Invert pixel polarity (255 - pixel)
        Examples:
            fs80hctl convert capture.png --to tiff --output forensic.tiff
            fs80hctl convert forensic.tiff --to pgm --output print.pgm
            fs80hctl convert forensic.tiff --to raw --output frame.raw

    inspect <IMAGE>
        Inspect biometric image metadata: dimensions, DPI, raw buffer SHA-256 hash,
        quality metrics (local variance and saturation), and custody manifest status.
        Examples:
            fs80hctl inspect capture.png
            fs80hctl --format json inspect forensic.tiff

    verify <PROBE> [<REFERENCE>] [--gallery <DIR>] [OPTIONS]
        Compare biometric images using deterministic spatial feature extraction and alignment.
        Supports 1:1 comparison against a single reference file or 1:N comparison against a
        directory containing a gallery of candidate images.
        Options:
            --gallery <DIR>              Directory containing reference images for 1:N comparison
            --threshold <0-100>          Minimum verification score threshold (default: 40.0)
            --exit-code                  Exit with code 2 if threshold is not met (mismatch)
        Examples:
            fs80hctl verify capture.png reference.tiff
            fs80hctl verify capture.png reference.tiff --threshold 45 --exit-code
            fs80hctl --format json verify probe.pgm ref.png
            fs80hctl verify probe.png --gallery ./enrolled_users/

    manual
        Display this technical reference manual on standard output.

SUPPORTED FORMATS
    PNG     Standard AFIS presentation (dark ridges, light background), 500 DPI pHYs chunk.
    TIFF    Baseline 6.0 L8 grayscale, full forensic EXIF tags (Make, Model, Software, 500 DPI).
    PGM     Netpbm binary P5, native format for NIST NBIS tools (mindtct, bozorth3).
    RAW/DAT Uncompressed 153,600-byte L8 buffer for NumPy, PyTorch, and ML pipelines.

CHAIN OF CUSTODY
    Every saved or converted image preserves an adjacent <IMAGE>.meta.json manifest.
    The manifest records the SHA-256 of the raw optical frame at acquisition, UTC timestamp,
    scanner model, and transformation lineage.

EXIT CODES
    0   Success / Biometric verification confirmed (Match).
    1   Operational error of syntax, invalid parameters, or unexpected failure.
    2   Biometric mismatch (comparison score below threshold).
    3   Timeout or finger not detected in capture window.
    4   Biometric quality insufficient (rejected by quality evaluation).
    5   USB hardware failure (device not found, permission denied, I/O error).
"#;
