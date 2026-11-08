# futronic-fs80h-rs

[README em português](README.md) · [Commands, examples, and tests](EXAMPLES.md)

Native Rust USB driver for Futronic FS80/FS80H and FS81H optical fingerprint
scanners. It captures grayscale images, processes biometric templates locally,
and can produce a stable C ABI without using or shipping Futronic ScanAPI.

## Status, scope, and limits

- Supported hardware: VID `0x1491`, PID `0x0020` (FS80/FS80H), and PID
  `0x0025` (FS81H).
- Normal capture returns `RawFrame`: 320×480, 8-bit grayscale, 500 DPI. The
  internal raw stream is 384×800 before resampling.
- Transport uses `libusb-1.0` through `rusb`, Bulk OUT `0x01`, and Bulk IN
  `0x82`. The calibrated default dose is `166` (`0xA6`).
- WSQ, ISO/IEC 19794-2, and ANSI INCITS 378 are format implementations; this
  project makes no government, ANSI, ISO, NIST, or partner certification claim.
- Matching, NFIQ2, and LFD research run locally. Consumers remain responsible
  for protecting biometric data.

> LFD research does not implement proprietary Traditional Live Finger Detection
> (TLFD), decide “live/fake”, or provide an anti-spoof control.

## Installation and first use

```toml
[dependencies]
futronic-fs80h-rs = { path = "../futronic-fs80h-rs" }
```

The Rust import is `futronic_fs80h_rs`. Install `libusb-1.0` and `pkg-config`:

```sh
# macOS
brew install libusb pkg-config

# Debian/Ubuntu
sudo apt-get install libusb-1.0-0-dev pkg-config
```

On Linux, grant USB access, for example through a udev rule containing
`SUBSYSTEM=="usb", ATTR{idVendor}=="1491", MODE="0660", GROUP="plugdev"`;
reload rules and reconnect the scanner.

```rust,no_run
use futronic_fs80h_rs::FutronicDevice;

# fn main() -> Result<(), Box<dyn std::error::Error>> {
let mut scanner = FutronicDevice::open_first()?;
let frame = scanner.capture_frame()?;
println!("{}×{} @ {} DPI", frame.width(), frame.height(), frame.dpi());
# Ok(())
# }
```

For multiple devices, use `list_devices`, `DeviceId`, and `open_device_by_id`;
`open_single_device` fails when selection would be ambiguous.

## Capabilities and features

| Area | Primary API | Feature | Notes |
| --- | --- | --- | --- |
| Capture and discovery | `FutronicDevice`, `RawFrame`, `DeviceId` | default | Direct capture, diodes, and bus/address selection. |
| Presence/auto-capture | `evaluate_frame_presence`, `AutoCaptureOptions` | default | Measures contact and stabilizes capture; not liveness. |
| WSQ | `RawFrame::to_wsq`, `RawFrame::from_wsq` | `wsq` | Locally compiled NBIS encoder/decoder. |
| Lossless images | `RawFrame::encode_image` | `image-formats` | PNG and TIFF at 500 DPI; PGM for technical diagnostics. |
| Templates | `IsoTemplate`, `AnsiTemplate`, `TemplateOptions` | `templates`, `ansi-378` | One-finger ISO 2005/2011 and ANSI 378. |
| Matcher | `Matcher` | `matcher`, `matcher-base64` | 1:1 verification, 1:N search, explicit Base64. |
| Biometric core | `extract_minutiae`, Bozorth3 | `biometric-core` | Minutiae extraction, rendering, and local comparison. |
| MCC | `MccTemplate`, `GalleryIndex` | `mcc` | Bit-based MCC matching and Rayon parallel search. |
| Quality | `Nfiq2` | `nfiq2` | Requires external NFIQ2 2.3, OpenCV, and model. |
| LFD research | `lfd` module | `lfd-research` | FS80H-only experimental collection/analysis. |
| Signal gate | `decide_lfd_experimental` | `lfd-experimental-decision` | Dataset-calibrated policy; not TLFD. |
| C ABI | `futronic_fs80h.h` | `native` | Dynamic library for other runtimes. |

Default features include WSQ, image formats, templates, ANSI, matcher, biometric
core, and MCC. `native` enables experimental LFD decision because the ABI exports it.
`nfiq2` requires `NFIQ2_DIR` to identify an installation with `include/` and
`lib/` directories.

## Local biometric flow

`RawFrame` preserves sensor geometry and provides inversion, normalization,
enhancement, resampling, and contact metrics. For automatic capture, use
`wait_for_finger_present` or `wait_for_finger_and_capture`, then wait for
removal with `wait_for_finger_removed` before the next action.

WSQ converts a 153,600-byte frame to FBI WSQ and reconstructs `RawFrame`.
ISO/IEC 19794-2:2011 is default; ISO 2005 + Cor.1:2009 and ANSI
378-2004/2009/2009 AM1 are explicit alternatives. Parsers validate supported
profiles and reject malformed records. Templates, like images, are sensitive
biometric data.

`RawFrame::encode_image` exports PNG, TIFF, and PGM without altering the frame's
current pixels. PNG and TIFF are 8-bit grayscale with 500-DPI metadata; TIFF uses
lossless Deflate compression. PGM (P5) is for technical tools and has no
standardized DPI metadata. These containers do not replace WSQ for biometric interchange.

```rust
use futronic_fs80h_rs::FingerprintImageFormat;

let png = frame.encode_image(FingerprintImageFormat::Png)?;
let tiff = frame.encode_image(FingerprintImageFormat::Tiff)?;
```

`Matcher` compares ISO and ANSI records 1:1, returns score-sorted 1:N
candidates, and supplies format-explicit Base64. Demonstration thresholds are
not universal FAR/FRR values: calibrate per device, population, and scenario.
The biometric core exposes minutiae extraction, rendering, Bozorth3, and MCC.
MCC supplies an in-memory gallery and parallel search, not a database or remote
service.

### NFIQ2 quality

NFIQ2 produces an image-utility score from 0 to 100 and actionable feedback
before template creation. Its default threshold is 50, configurable through
`Nfiq2Options`. It does not measure pressure, detect liveness, or prove a
physical cause of poor capture. NFIQ2 2.3, its model, and OpenCV are external;
the operational command is documented in [EXAMPLES.md](EXAMPLES.md).

## Native C integration

With `--features native`, the crate emits a `cdylib` as well as the Rust
library. The artifact is `libfutronic_fs80h_rs.dylib` on macOS and
`libfutronic_fs80h_rs.so` on Linux; Unix clients link with
`-lfutronic_fs80h_rs`. The contract is in
[`include/futronic_fs80h.h`](include/futronic_fs80h.h), and
[`examples/native_smoke.c`](examples/native_smoke.c) is the minimal smoke test.

ABI v1 uses `fs80h_status`, opaque handles, and synchronous calls. After an
error, read `fs80h_last_error_message()` on the same thread. Free handles with
their matching `*_free` function and each `fs80h_buffer` only with
`fs80h_buffer_free`; never use `free`, `delete`, or a consumer allocator.
Input buffers remain caller-owned, and a handle cannot be used concurrently.

```c
#include "futronic_fs80h.h"

int main(void) {
  fs80h_device *device = NULL;
  if (fs80h_device_open_first(&device) != FS80H_OK) return 1;
  fs80h_device_close(device);
  return 0;
}
```

Use `fs80h_list_devices(NULL, 0, &count)` when multiple readers are present;
allocate `fs80h_device_info[count]`, list again, and open the selected ID.

```sh
cargo build --release --features native,lfd-experimental-decision
./scripts/package-native.sh
```

The `dist/` package contains library, `libusb` runtime, header, READMEs,
`EXAMPLES.md`, and licenses. Keep the library and `libusb` in the same bundle
or loader path: rpath/`LD_LIBRARY_PATH` on Linux, `@loader_path`/rpath on
macOS, or the executable directory on Windows.

## Experimental LFD research

`lfd-research` investigates FS80H on an isolated reader with consented
collection. FS81H is reported as unsupported. Static analysis suggests this
sequence: configure CMOS/backlight, collect three auxiliary frames at `0x2d`
with selectors `0x84`, `0x81`, and `0x01`, subtract a dark frame, calculate
exposure, capture final frames, and restore CMOS. This does not establish
vendor calibration or thresholds.

Static evidence includes the vendor-library symbols `SetOptions`, `GetFrame`,
`CalculationDLFD`, and `IsFakeFinger80H`, plus
`FTR_OPTIONS_DETECT_FAKE_FINGER` activation in the Android demo. It establishes
that the proprietary solution consumes more than the public 320×480 frame, but
does not reveal calibration constants, features, or error rates.

The collector discards two warm-up captures and records 384×800 streams,
timings, control/descriptor responses, and descriptive metrics. Colored-CMOS
uses eight-byte Bulk OUT commands for `0x4A..0x4F` and restores `4A 03`; use it
only behind its explicit flag on a test reader. Firmware query is read-only.
Offline analysis subtracts channel `0x01` and reports mean absolute difference
and correlation; threshold 20 is diagnostic, not liveness.

`lfd-experimental-decision` reads an external versioned policy and returns
`biometric_signal_present` or `insufficient_biometric_signal`. The example
policy was fitted only to genuine captures and four-layer medical gloves;
recalibrate with consented, labelled data in every environment. Never present
it as live-finger, anti-spoof, or TLFD-equivalent classification. Do not commit
RAW streams; protect and dispose of them under applicable policy.

Responsible collection records PID, control/descriptor bytes, dose, timing,
metrics, and streams for every session; keeps ordinary placement variation; and
uses authorized, labelled samples before any classifier. The next technical
milestones are validating geometry tables and register transport against
independent traces, keeping a non-executing sequence representation, and
evaluating error rates on a held-out set before any use beyond research.

## Architecture and quality

Modules are grouped by cohesion: transport/capture, frames, discovery, errors,
presence, templates, matcher, WSQ, biometrics, LFD, NFIQ2, and FFI. `src/lib.rs`
re-exports the public surface; protocol and USB details stay internal. Unit and
integration tests cover frame, WSQ, presence, discovery, biometric core, and MCC.

The release workflow validates formatting, compilation, tests, and Clippy, then
builds/tests the native ABI on configured platforms. Run locally:

```sh
cargo fmt --check
cargo check
cargo test
cargo clippy -- -D warnings
```

Per-suite, feature, example, benchmark, and output commands are in
[EXAMPLES.md](EXAMPLES.md), which is intentionally maintained in Portuguese.

## Project evolution

1. Initial Rust FS80H/FS81H driver and USB capture.
2. WSQ, ISO 2011, ISO 2005, and ANSI 378 templates.
3. Local matcher, optional NFIQ2, and LFD signal research.
4. CMOS transport, minutiae extraction, Bozorth3, and MCC benchmarking.
5. Finger presence, auto-capture, Base64, `DeviceId` discovery, and C ABI.
6. Public crate-name alignment, quality CI, and truncation-safe ISO parser.

## License

Rust code is [MIT](LICENSE). NBIS-derived WSQ code is under
[`c_src/LICENSE-NIST`](c_src/LICENSE-NIST).
