# fs80hctl

Technical Rust CLI for the Futronic FS80/FS80H fingerprint scanner, designed for optical frame acquisition, forensic image processing, standard biometric formats (TIFF 500 DPI, NIST PGM, RAW, PNG), chain-of-custody tracking, and 1:1 biometric comparison.

> Results provided by this tool are technical similarity indicators. They do not
> constitute civil identification, forensic expert reports, or certified liveness/
> anti-spoofing results. The static liveness screen is an engineering heuristic,
> not a presentation-attack-detection (PAD) claim.

## Features

- **USB Scanner Discovery**: Automatic detection of Futronic FS80, FS80H, and FS81H devices.
- **Optical Frame Capture with Presence Detection (`capture`)**: 320×480 resolution, 8-bit grayscale (L8) @ 500 DPI with optional `--wait` auto-trigger and quality threshold gating.
- **Multi-Sample Assisted Enrollment (`enroll`)**: Guided best-of-N frame acquisition electing the highest quality sample automatically.
- **Forensic & Biometric Image Formats**:
  - **PNG**: 8-bit grayscale with `pHYs` chunk calibrated to 500 DPI (19,685 px/m).
  - **TIFF Baseline 6.0**: 500 DPI with full forensic EXIF tags (`ResolutionUnit=Inch`, `XResolution=500`, `YResolution=500`, `Software`, `Make="Futronic"`, `Model="FS80H"`).
  - **PGM (Netpbm P5 binary)**: Direct binary input format for NIST NBIS reference tools (`mindtct`, `bozorth3`).
  - **RAW / DAT**: Uncompressed 153,600-byte L8 dump for NumPy, PyTorch, and machine learning pipelines.
- **Forensic Transcoder (`convert`)**: Bidirectional conversion across all supported formats with custody preservation (`.meta.json`).
- **Technical Image Inspection (`inspect`)**: Inspect detected format, dimensions, SHA-256 buffer hash, local quality metrics, and custody history.
- **Immutable Chain of Custody**: SHA-256 computed on the raw optical sensor buffer and preserved in an adjacent `.meta.json` manifest.
- **Native Quality Assessment**: Mean local variance across 16×16 blocks and sensor saturation ratio analysis.
- **Biometric Verification (`verify`)**: Deterministic spatial minutiae comparison supporting 1:1 pairwise matching and 1:N gallery identification against directories.
- **Forensic Analysis (`forensic`)**: Minutiae annotation, corroborated-pair comparison, JSON reporting, and static liveness screening for authorized technical evaluation.
- **Image Tools (`tool`)**: Gabor enhancement, RAW/PNG/WSQ interchange conversion, and deterministic synthetic benchmarks.
- **Environment Diagnostics (`doctor`)**: Verifies USB subsystem, scanner permissions, and filesystem write access.
- **100% Pure Rust Ecosystem**: Zero C bridges, vendored ABI bundles, or dynamic library dependencies (`libtiff`/`libpng`).

## Requirements

- Rust/Cargo compatible with the 2024 edition.
- macOS or Linux.
- For physical capture: Futronic FS80/FS80H device connected via USB.

The workspace uses the pure-Rust [`futronic-fs80h-rs`](https://crates.io/crates/futronic-fs80h-rs) driver published on crates.io.
No proprietary vendor SDKs or dynamic libraries need to be installed.

## Building and Testing

```sh
cd /Volumes/MAIN/Workspace/fs80hctl
cargo check --workspace
cargo test --workspace
```

To validate with a connected physical scanner, refer to [`TESTING.md`](TESTING.md).

Automated tests run on synthetic fixtures and pass 100% without any connected hardware.

## CLI Usage

Execute the CLI directly via Cargo or installed binary:

```sh
cargo run -p fs80h-cli -- <command>
```

For the comprehensive technical manual and flag specification, see [`docs/MANUAL.md`](docs/MANUAL.md) or execute the built-in manual:

```sh
cargo run -p fs80h-cli -- manual
cargo run -p fs80h-cli -- --help
cargo run -p fs80h-cli -- --version
```

---

### Environment Diagnostics (`doctor`)

Inspects USB subsystem health, device access permissions, and write availability:

```sh
cargo run -p fs80h-cli -- doctor
cargo run -p fs80h-cli -- --json doctor
```

---

### Device Discovery & Inspection

```sh
# List connected fingerprint scanners
cargo run -p fs80h-cli -- devices

# Inspect USB communication with the first available device
cargo run -p fs80h-cli -- device inspect

# Select a specific device by USB bus and address
cargo run -p fs80h-cli -- device inspect --bus 1 --address 1
```

---

### Capture (`capture`)

Acquire a fingerprint directly into your desired format:

```sh
# Standard AFIS PNG (immediate capture)
cargo run -p fs80h-cli -- capture --output capture.png

# Smart Auto-Trigger: waits for finger presence and contact stabilization
cargo run -p fs80h-cli -- capture --wait --timeout 15 --min-quality 60 --output capture.png

# Direct forensic TIFF (500 DPI)
cargo run -p fs80h-cli -- capture --format tiff --output forensic.tiff

# Direct NIST Netpbm PGM
cargo run -p fs80h-cli -- capture --format pgm --output print.pgm

# Raw optical sensor signal (light ridges on dark background, without inversion)
cargo run -p fs80h-cli -- capture --raw --output capture_raw.png
```

Every capture generates the image file and its associated custody manifest:
```text
forensic.tiff
forensic.tiff.meta.json
```

---

### Multi-Sample Enrollment (`enroll`)

Acquire multiple samples sequentially, evaluate each frame's quality, and elect the best-quality sample:

```sh
# Enroll with 3 samples (default: 3 samples, elects highest quality)
cargo run -p fs80h-cli -- enroll --output master.png

# Enroll with 5 samples in forensic TIFF format with custom quality threshold
cargo run -p fs80h-cli -- enroll --samples 5 --timeout 20 --min-quality 65 --format tiff --output master.tiff

# Output structured enrollment JSON
cargo run -p fs80h-cli -- --json enroll --samples 3 --output master.png
```

---

### Format Transcoding (`convert`)

Convert bidirectionally between any pair of formats while propagating root custody:

```sh
# PNG to Forensic TIFF (500 DPI)
cargo run -p fs80h-cli -- convert capture.png --to tiff --output digital.tiff

# TIFF to Netpbm PGM
cargo run -p fs80h-cli -- convert digital.tiff --to pgm --output digital.pgm

# PGM to raw binary dump (153,600 bytes)
cargo run -p fs80h-cli -- convert digital.pgm --to raw --output digital.raw
```

---

### Technical Inspection (`inspect`)

Inspect dimensions, detected format, DPI, SHA-256 buffer hash, and quality metrics:

```sh
cargo run -p fs80h-cli -- inspect forensic.tiff
cargo run -p fs80h-cli -- --json inspect digital.pgm
```

---

### Biometric Verification (`verify`)

Compare fingerprint images across different formats:

```sh
# 1:1 Pairwise Verification (TIFF vs PNG)
cargo run -p fs80h-cli -- verify forensic.tiff capture.png --threshold 40

# 1:N Gallery Identification against a directory of enrolled prints
cargo run -p fs80h-cli -- verify probe.png --gallery ./enrolled_users/ --threshold 40

# Structured JSON output (for pipelines)
cargo run -p fs80h-cli -- --json verify probe.png --gallery ./enrolled_users/

# Fail with non-zero exit code (1) if threshold is not met
cargo run -p fs80h-cli -- verify forensic.tiff capture.png --threshold 40 --exit-code
```

---

### Forensic Analysis and Image Tools

The `forensic` commands are technical aids, not identification or certified PAD
workflows. Keep real biometric inputs and all derived artifacts outside the
repository. The complete authorized-dataset procedure is in
[`docs/FORENSIC_BIOMETRIC_VALIDATION.md`](docs/FORENSIC_BIOMETRIC_VALIDATION.md).

```sh
# Draw detected skeleton minutiae over a supported 320x480 image
cargo run -p fs80h-cli -- forensic annotate --input capture.png --output annotated.png

# Compare two images; exit code 2 means score or corroborated-pair support is insufficient
cargo run -p fs80h-cli -- forensic compare --probe probe.png --reference reference.png --output comparison.png

# Emit the JSON-only forensic report, or run the static liveness heuristic
cargo run -p fs80h-cli -- forensic report capture.png
cargo run -p fs80h-cli -- --json forensic liveness capture.png

# Enhance a frame, convert among RAW/PNG/WSQ, or benchmark synthetic work
cargo run -p fs80h-cli -- tool enhance --input capture.png --output enhanced.png
cargo run -p fs80h-cli -- tool convert --input capture.png --output capture.wsq
cargo run -p fs80h-cli -- --json tool bench matching --templates 50000
```

`forensic compare` requires at least six geometrically consistent minutiae pairs
before it can report a non-zero comparison score. `forensic liveness` exits with
code 4 below its current heuristic threshold; it is not validated to ISO/IEC
30107-3 or LivDet.

---

## External Integration with NIST NFIQ 2

To evaluate images generated by `fs80hctl` against the international standard **NIST NFIQ 2** (ISO/IEC 29794-4:2017):

1. **Direct Terminal Usage**:
   ```bash
   # Evaluate TIFF file generated by fs80hctl
   /usr/local/bin/nfiq2 -a -i forensic.tiff
   ```
2. **Automated Quality Gate Script**:
   ```bash
   # Captures and validates whether NFIQ2 score is >= 50
   ./scripts/capture_with_nfiq2.sh --output sample.tiff --min-score 50
   ```

For detailed specifications, see [`docs/NFIQ2_INTEGRATION.md`](docs/NFIQ2_INTEGRATION.md).

---

## Governance and Security

- [LICENSE](LICENSE): MIT License.
- [SECURITY.md](SECURITY.md): Biometric data policy, minimization, and vulnerability reporting.
- [CONTRIBUTING.md](CONTRIBUTING.md): Contribution guidelines and strict prohibition of real biometrics in the repository.
