# Repository Guidelines

## Project Structure & Module Organization

This Rust 2024 workspace contains three crates:

- `crates/fs80h-core/`: hardware-independent domain logic, image codecs, quality metrics, custody hashing, and matching.
- `crates/fs80h-driver/`: Futronic USB-driver integration.
- `crates/fs80h-cli/`: the `fs80hctl` command-line binary and command parsing.

Workspace-level integration tests live in `tests/`; use `tests/fixtures/` only for approved deterministic synthetic images. Supporting shell workflows are in `scripts/`, user documentation is in `docs/`, and the Unix manpage is `man/fs80hctl.1`.

Keep `fs80h-core` free of USB and native hardware dependencies. Put hardware-facing behavior in the driver and user-facing command wiring in the CLI.

## Build, Test, and Development Commands

Run these from the repository root:

```sh
cargo check --workspace                 # Fast type check
cargo test --workspace --verbose        # Hardware-free synthetic test suite
cargo fmt --all -- --check              # Verify Rust formatting
cargo clippy --workspace --all-targets -- -D warnings  # Lint strictly
cargo doc --workspace --no-deps         # Verify documentation builds
cargo run -p fs80h-cli -- --help         # Run the CLI locally
./scripts/check-clean-workspace.sh      # Detect biometric-data leaks
```

Use `cargo build --release --workspace` for release builds. Physical scanner testing is opt-in: consult `TESTING.md` and run `./scripts/test-physical.sh` only with authorized hardware and handling procedures.

## Coding Style & Naming Conventions

Use idiomatic Rust and let `rustfmt` determine formatting (four-space indentation). Follow existing names: `snake_case` for functions, modules, files, and test functions; `PascalCase` for types; `SCREAMING_SNAKE_CASE` for constants. Keep public APIs and errors explicit, avoid hardware logic in core, and add `--json` support for new CLI commands that produce structured output.

## Testing Guidelines

Add deterministic automated coverage for every feature. Name tests descriptively, for example `test_cli_verify_match`. Prefer unit tests near implementation and integration tests in `tests/`. Never use a connected scanner or real biometric data in automated tests; generate fixtures with `tests/fixture_support.rs` and retain only approved synthetic data under `tests/fixtures/`.

## Commit & Pull Request Guidelines

Use the established Conventional Commit style, such as `feat: add doctor command`, `fix: handle empty scanner selection`, or `chore(deps): bump driver`. Keep commits focused. Pull requests should describe behavior and testing, link relevant issues, include CLI output or screenshots when UI/output changes, and confirm the full validation commands above pass. Never include real fingerprint images, custody manifests, or sensitive hardware captures; report vulnerabilities privately as described in `SECURITY.md`.
