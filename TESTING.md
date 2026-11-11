# Testing Guide

## Hardware-Free Testing

All automated tests in the workspace use synthetic deterministic fixtures and do not require any connected USB hardware:

```sh
cargo check --workspace
cargo test --workspace
```

## Physical Scanner Testing

To test with physical Futronic hardware:
- Ensure `libusb-1.0` and `pkg-config` are installed.
- On Linux, configure udev rules or grant user permissions to access Futronic USB devices (Vendor ID `0x1491`).
- Connect the scanner and run:

```sh
./scripts/test-physical.sh
```

When multiple scanners are connected, specify the desired USB bus and address (found using `cargo run -p fs80h-cli -- devices`):

```sh
FS80H_BUS=1 FS80H_ADDRESS=4 ./scripts/test-physical.sh
```

If frame-mode capture times out on your USB host controller, test the variable-dose direct path:

```sh
FS80H_VARIABLE_DOSE=1 ./scripts/test-physical.sh
```

The script outputs the captured image and manifest into a temporary directory `fs80h-physical.*` located in `~/Downloads`, performs an automated local 1:1 verification, and reports the directory path. To choose a custom directory, set `FS80H_OUTPUT_DIR`.

> [!CAUTION]
> Biometric data is sensitive. Always review, protect, and safely delete this test directory after completing your verification.

## Supplied Biometric Dataset Validation

For authorized validation using an externally supplied biometric dataset, follow
[`docs/FORENSIC_BIOMETRIC_VALIDATION.md`](docs/FORENSIC_BIOMETRIC_VALIDATION.md).
That protocol keeps real biometric data outside this repository and defines the
required genuine/impostor and PAD evidence before changing a production threshold.
