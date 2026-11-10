# Fuzz Testing Guide — fs80hctl

This project integrates property-based testing (`proptest`) and coverage-guided fuzz testing via `cargo-fuzz` and `libFuzzer` to guarantee memory safety, panic-freedom, and resilient input parsing across all protocol and image decoders.

---

## 1. Property-Based Testing (proptest)

Property tests execute automatically as part of the standard test suite:

```bash
cargo test --workspace
```

They continuously verify:
- **`fs80h-core`**: Invariant validation for `QualityScore`, `MatchScore`, `GrayscaleImage`, and panic-freedom for `RawFrame::from_pgm_bytes` and `RawFrame::from_png_bytes` on random byte streams.
- **`fs80h-driver`**: Arbitrary packet parsing for `parse_response_packet` and arbitrary chunk feeds into `FrameAssembler`.

---

## 2. Coverage-Guided Fuzzing with `cargo-fuzz`

### Prerequisites

Ensure you have a nightly Rust toolchain and `cargo-fuzz` installed:

```bash
rustup install nightly
cargo +nightly install cargo-fuzz
```

### Available Fuzz Targets

1. **`fuzz_target_frame`**: Fuzzes raw frame chunk assembly, packet header verification, and checksum validation in `fs80h-driver`.
2. **`fuzz_target_template`**: Fuzzes image decoders (PGM, PNG), minutiae extraction, and quality metric calculations in `fs80h-core`.

### Running Fuzz Targets

From the repository root:

```bash
# Fuzz USB frame chunks & packet parser
cargo +nightly fuzz run fuzz_target_frame

# Fuzz image decoders & biometric algorithms
cargo +nightly fuzz run fuzz_target_template
```

### Running with AddressSanitizer (ASan)

`cargo-fuzz` enables AddressSanitizer by default to catch any out-of-bounds accesses or memory corruption.

To run for a specific duration or iteration count:

```bash
# Run for 60 seconds
cargo +nightly fuzz run fuzz_target_frame -- -max_total_time=60

# Run with 4 parallel worker threads
cargo +nightly fuzz run fuzz_target_template -- -jobs=4 -workers=4
```
