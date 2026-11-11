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

To exercise the template path with the same physical capture, create an ISO 2011 template, validate it, and test explicit Base64 round-trip in that protected directory:

```sh
fs80hctl template create "$OUTPUT_DIR/capture.png" --template-format iso2011 \
  --finger right-index --impression live-plain --output "$OUTPUT_DIR/right-index.fmr"
fs80hctl template validate "$OUTPUT_DIR/right-index.fmr" --template-format iso2011
fs80hctl template base64 encode "$OUTPUT_DIR/right-index.fmr" --template-format iso2011 > "$OUTPUT_DIR/right-index.b64"
fs80hctl template base64 decode "$OUTPUT_DIR/right-index.b64" --template-format iso2011 --output "$OUTPUT_DIR/right-index-decoded.fmr"
cmp "$OUTPUT_DIR/right-index.fmr" "$OUTPUT_DIR/right-index-decoded.fmr"
```

Replace `OUTPUT_DIR` with the physical-test directory printed by the script. This tests local extraction and serialization, not matching accuracy or external-profile conformance.

> [!CAUTION]
> Biometric data is sensitive. Always review, protect, and safely delete this test directory after completing your verification.

## Supplied Biometric Dataset Validation

For authorized validation using an externally supplied biometric dataset, follow
[`docs/FORENSIC_BIOMETRIC_VALIDATION.md`](docs/FORENSIC_BIOMETRIC_VALIDATION.md).
That protocol keeps real biometric data outside this repository and defines the
required genuine/impostor and PAD evidence before changing a production threshold.
