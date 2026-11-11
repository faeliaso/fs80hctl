# User Manual — fs80hctl

`fs80hctl` is a command-line utility written in pure Rust for technical control, optical frame acquisition, chain-of-custody tracking, and 1:1 biometric comparison with **Futronic FS80, FS80H, and FS81H** optical fingerprint scanners.

---

## Table of Contents

1. [Installation & Building](#installation--building)
2. [General Syntax](#general-syntax)
3. [Global Options](#global-options)
4. [Available Commands](#available-commands)
   - [doctor (Environment Diagnostics)](#doctor)
   - [devices (Device Discovery)](#devices)
   - [device inspect (Scanner Inspection)](#device-inspect)
   - [capture (Biometric Acquisition & Auto-Trigger)](#capture)
   - [enroll (Multi-Sample Enrollment)](#enroll)
   - [convert (Forensic Transcoding)](#convert)
   - [inspect (Image & Custody Inspection)](#inspect)
   - [verify (1:1 and 1:N Biometric Comparison)](#verify)
   - [forensic (Technical Analysis)](#forensic)
   - [tool (Enhancement, Interchange, and Benchmarks)](#tool)
   - [serve (Daemon over Unix Domain Socket)](#serve)
   - [manual (Built-in Manual)](#manual)
5. [Supported Formats](#supported-formats)
6. [Chain of Custody & Manifests](#chain-of-custody--manifests)
7. [Exit Codes](#exit-codes)

---

## Installation & Building

To build `fs80hctl`:

```bash
# Build release binary
cargo build --release --workspace

# The binary is located at:
./target/release/fs80hctl --version
```

Or run directly via Cargo during development:

```bash
cargo run -p fs80h-cli -- <command>
```

---

## General Syntax

```bash
fs80hctl [GLOBAL_OPTIONS] <COMMAND> [ARGUMENTS...]
```

Examples:

```bash
fs80hctl --help
fs80hctl --version
fs80hctl manual
```

---

## Global Options

| Option | Description |
|---|---|
| `--format <text\|json\|raw>` | Global output format mode: human-readable `text`, structured `json`, or pure `raw` byte stream. Default: `text`. |
| `--json` | Shorthand alias for `--format json`. Output results in structured JSON format. |
| `-v`, `--verbose` | Increase logging verbosity on `stderr` (`-v` for INFO, `-vv` for DEBUG, `-vvv` for TRACE). |
| `--mock <scenario>` | Hardware mock simulation scenario (`match`, `mismatch`, `degraded`, `empty`, `disconnect`) for testing without physical devices. Configurable via `FS80H_MOCK_FIXTURE`. |
| `-h`, `--help` | Print help information and usage syntax. |
| `-V`, `--version` | Print the installed `fs80hctl` version. |

---

## Available Commands

### `doctor`
Validates runtime dependencies and system health:
- `libusb` loading and USB subsystem health.
- Physical detection of compatible Futronic hardware.
- Read/write access permissions on the scanner USB endpoint.
- Filesystem write availability in the current working directory.

```bash
fs80hctl doctor
fs80hctl --json doctor
```

---

### `devices`
Lists all Futronic fingerprint scanners connected to the USB bus, showing bus number (`bus`), address (`addr`), Vendor/Product ID, and model (`fs80`, `fs80h`, `fs81h`).

```bash
fs80hctl devices
fs80hctl --json devices
```

---

### `device inspect`
Opens a test connection to the selected scanner (or first available) to verify hardware readiness and communication.

```bash
# Inspect the first available scanner
fs80hctl device inspect

# Inspect a specific scanner at bus 1, address 4
fs80hctl device inspect --bus 1 --address 4
```

---

### `capture`
Acquires an optical biometric frame from the scanner (320×480 resolution, 8-bit grayscale L8, calibrated to 500 DPI). Writes the image file and an adjacent chain-of-custody manifest (`.meta.json`).

```bash
# Capture standard AFIS PNG (500 DPI pHYs chunk, immediate capture)
fs80hctl capture --output digital.png

# Smart Auto-Trigger: waits for finger presence and contact stabilization
fs80hctl capture --wait --timeout 15 --min-quality 60 --output digital.png

# Direct forensic TIFF Baseline 6.0 with full EXIF tags
fs80hctl capture --format tiff --output forensic.tiff

# Direct Netpbm binary PGM (NIST NBIS)
fs80hctl capture --format pgm --output print.pgm

# Raw optical sensor frame without inversion (light ridges on dark background)
fs80hctl capture --raw --output sensor_raw.png

# Direct variable-dose optical acquisition
fs80hctl capture --variable-dose --dose 180 --output dose.png

# Capture via background daemon (sub-50ms latency)
fs80hctl capture --client --output stream.png
```

---

### `serve`
Runs a background daemon process that keeps the scanner USB context open and listens on a Unix Domain Socket (UDS) for low-latency capture requests and event streaming via JSON Lines.

```bash
# Start daemon listening on default socket (/run/fs80h/fs80h.sock or /tmp/fs80h.sock)
fs80hctl serve

# Start daemon with custom socket path
fs80hctl serve --socket-path /tmp/my_fs80h.sock

# Start daemon with mock transport for testing
fs80hctl --mock match serve --socket-path /tmp/test.sock
```

---

### `enroll`
Acquires multiple biometric samples consecutively, evaluates their local variance and quality scores, and automatically selects and persists the highest-quality frame (best-of-N).

```bash
# Enroll with 3 samples (default) and save the best sample
fs80hctl enroll --output master.png

# Custom sample count, timeout, and format
fs80hctl enroll --samples 5 --timeout 20 --min-quality 65 --format tiff --output master.tiff

# Structured JSON output for integration into enrollment pipelines
fs80hctl --json enroll --samples 3 --output master.png
```

---

### `convert`
Transcodes biometric images between any supported format pair, preserving the root SHA-256 hash and recording transformation lineage in the adjacent `.meta.json` manifest.

```bash
# PNG to TIFF 500 DPI
fs80hctl convert digital.png --to tiff --output forensic.tiff

# TIFF to Netpbm PGM (NIST)
fs80hctl convert forensic.tiff --to pgm --output print.pgm

# PGM to raw binary dump (153,600 bytes)
fs80hctl convert print.pgm --to raw --output frame.raw

# Transcode with inverted pixel polarity (255 - pixel)
fs80hctl convert digital.png --invert --output inverted.png
```

---

### `inspect`
Analyzes biometric image files stored on disk:
- Dimensions and spatial resolution (500 DPI).
- Cryptographic SHA-256 hash of the uncompressed raw pixel buffer.
- Native quality metrics: mean local variance and sensor saturation ratio.
- Status and integrity of the `.meta.json` custody manifest.

```bash
fs80hctl inspect digital.png
fs80hctl --json inspect forensic.tiff
```

---

### `verify`
Performs deterministic spatial comparison between biometric images across any combination of supported formats (e.g., comparing TIFF directly against PNG). Supports both **1:1 pairwise matching** and **1:N gallery identification**.

```bash
# 1:1 Comparison with default threshold (40.0)
fs80hctl verify digital.png reference.tiff

# 1:1 Custom threshold
fs80hctl verify digital.png reference.tiff --threshold 45.0

# 1:1 Exit with code 1 if threshold is not met
fs80hctl verify digital.png reference.tiff --threshold 50.0 --exit-code

# 1:1 Structured JSON output
fs80hctl --json verify digital.png reference.tiff

# 1:N Gallery Identification against a folder of candidate prints
fs80hctl verify probe.png --gallery ./candidates/ --threshold 40.0

# 1:N Gallery Verification with JSON output
fs80hctl --json verify probe.png --gallery ./candidates/
```

---

### `forensic`

Provides technical minutiae analysis for authorized evaluation only. It does not
make an identification decision, issue an expert report, or provide certified
PAD. Inputs and generated annotations, reports, and comparisons are biometric
data and must remain outside the checkout; follow
[`FORENSIC_BIOMETRIC_VALIDATION.md`](FORENSIC_BIOMETRIC_VALIDATION.md).

```bash
# Annotate detected skeleton minutiae on a 320x480 supported image
fs80hctl forensic annotate --input sample.png --output annotated.png

# Write a visual comparison; score requires six corroborated minutiae pairs
fs80hctl forensic compare --probe probe.png --reference reference.png --output comparison.png --threshold 40

# JSON-only technical report and deterministic static texture screen
fs80hctl forensic report sample.png
fs80hctl --json forensic liveness sample.png
```

`forensic compare` exits with code `2` if its threshold or the minimum of six
geometrically consistent pairs is not satisfied. `forensic liveness` exits with
code `4` below score 65; it is a static heuristic, not ISO/IEC 30107-3 or LivDet
validation.

---

### `tool`

Provides local image processing and repeatable synthetic benchmarks. `convert`
within this command supports `raw`, `png`, and `wsq` format names (or infers them
from the file extensions); WSQ bitrate defaults to `0.75` and DPI to `500`.

```bash
fs80hctl tool enhance --input sample.png --output enhanced.png
fs80hctl tool convert --input sample.png --output sample.wsq
fs80hctl --json tool bench matching --templates 50000
fs80hctl --json tool bench io --iterations 100 --input sample.png
```

---

### `manual`
Displays the technical reference manual directly in the terminal standard output.

```bash
fs80hctl manual
fs80hctl --json manual
```

---

## Supported Formats

| Format | Technical Details | Typical Usage |
|---|---|---|
| **PNG** | 8-bit grayscale with calibrated `pHYs` chunk at 500 DPI (19,685 px/m). Standard AFIS visual polarity (dark ridges on light background). | Civil identity, web applications, general storage. |
| **TIFF** | Baseline 6.0 L8 grayscale with full forensic EXIF tags (`Make="Futronic"`, `Model="FS80H"`, `XResolution=500`, `YResolution=500`, `ResolutionUnit=Inch`). | Forensic archiving, expert reports, legal custody. |
| **PGM** | Netpbm binary format P5 (320 480 255). | Reference tools: NIST NBIS (`mindtct`, `bozorth3`). |
| **RAW / DAT** | Uncompressed 153,600-byte contiguous buffer (320×480 in L8). | Neural network training, NumPy, PyTorch. |

---

## Chain of Custody & Manifests

Every image captured with `fs80hctl capture` or transcoded with `fs80hctl convert` generates an adjacent manifest file with the `.meta.json` extension:

```text
sample.tiff
sample.tiff.meta.json
```

The manifest preserves:
- `sha256_raw_frame`: Cryptographic SHA-256 hash computed on the original optical sensor buffer at acquisition.
- `captured_at_utc`: ISO-8601 UTC timestamp of acquisition.
- `scanner_model`: Hardware scanner model used during capture.
- `transformations`: Ordered list of applied operations (e.g., format transcoding, polarity inversion).

---

## Exit Codes

| Code | Description |
|---|---|
| `0` | Success / Biometric verification confirmed (Match). |
| `1` | Operational error: invalid parameters, syntax error, or general unexpected failure. |
| `2` | Biometric mismatch (finger comparison score below threshold when `--exit-code` is passed). |
| `3` | Capture timeout or finger not detected within acquisition window. |
| `4` | Biometric quality insufficient or liveness score below the heuristic threshold. |
| `5` | USB hardware failure (device not found, permission denied on `/dev/bus/usb`, I/O failure). |
