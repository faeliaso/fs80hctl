# Releasing `fs80hctl`

This checklist publishes a release only after the source, documentation, and
hardware-free checks agree on the same workspace version.

## Preflight

From the repository root, confirm the release branch is clean apart from the
intended changes and that `Cargo.toml`, the lockfile, CLI version test, and
manpage use the same version.

```sh
git status --short
cargo check --workspace
cargo test --workspace --verbose
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo doc --workspace --no-deps
./scripts/check-clean-workspace.sh
```

The repository's physical scanner tests are intentionally separate from the
hardware-free suite. Run them only with authorized hardware and outside the
repository:

```sh
./scripts/test-physical.sh
```

For a release that contains template changes, run the image-to-template and
Base64 round-trip commands from `TESTING.md` in the protected output directory.
Do not commit the generated image, FMR, Base64, manifest, or console output.

## Build artifacts

```sh
cargo build --release --workspace
./target/release/fs80hctl --version
shasum -a 256 ./target/release/fs80hctl
```

The displayed version must match the release tag. Archive the binary and its
checksum only through the approved release channel.

## Publish

After review and approval, create a focused conventional commit, annotate a tag,
push the branch and tag, then create the hosted release using the approved
repository workflow:

```sh
git add Cargo.toml Cargo.lock README.md TESTING.md crates/ docs/ man/ scripts/check-clean-workspace.sh tests/
git commit -m "feat: add biometric minutiae templates"
git tag -a v0.2.0 -m "fs80hctl v0.2.0"
git push origin HEAD
git push origin v0.2.0
```

Release notes must state that template parsing/serialization is structural and
that AFIS matching accuracy, PAD/liveness, and external-profile conformance are
not certified by this release.
