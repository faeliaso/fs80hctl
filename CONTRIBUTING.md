# Contributing Guide — fs80hctl

Thank you for your interest in contributing to `fs80hctl`!

## Fundamental Rule: Prohibition of Real Biometrics

> [!CAUTION]
> **Never commit or submit real fingerprint biometric data in Pull Requests, Issues, or test files.**
> All automated tests must use programmatically generated synthetic images or the deterministic fixtures approved in `tests/fixtures/`.

To generate new synthetic fixtures, use the generator functions in `tests/fixture_support.rs`.

## Development Environment & Tooling

- **Rust**: 2024 Edition / Latest stable toolchain.
- **Formatting and Linters**:
  ```sh
  cargo fmt --all -- --check
  cargo clippy --workspace --all-targets -- -D warnings
  ```
- **Hardware-Free Automated Tests**:
  ```sh
  cargo test --workspace
  ```
- **Workspace Cleanliness & Leak Prevention**:
  ```sh
  ./scripts/check-clean-workspace.sh
  ```

## Workspace Architecture

- [`crates/fs80h-core`](crates/fs80h-core): Pure domain logic, image codecs (PNG, TIFF, PGM, RAW), SHA-256 custody hashing, quality metrics, and deterministic 1:1 matcher. Zero dependency on USB hardware.
- [`crates/fs80h-driver`](crates/fs80h-driver): Hardware integration layer wrapping `futronic-fs80h-rs`.
- [`crates/fs80h-cli`](crates/fs80h-cli): Command-line interface (`fs80hctl`).

## Code Standards

- Maintain strict architectural separation: `fs80h-core` must never depend on USB or native hardware drivers.
- Every new feature must include automated synthetic tests.
- All CLI commands providing structured output must support the global `--json` flag.
