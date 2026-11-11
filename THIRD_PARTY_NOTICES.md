# Third-Party Notices

`fs80hctl` is licensed under the MIT License; see [`LICENSE`](LICENSE).
This document identifies the principal third-party runtime components used by
the release binary. The exact resolved dependency graph is recorded in
[`Cargo.lock`](Cargo.lock).

## Rust components

| Component | Version | Declared license |
| --- | --- | --- |
| `anyhow` | 1.0.104 | MIT OR Apache-2.0 |
| `base64` | 0.22.1 | MIT OR Apache-2.0 |
| `blake3` | 1.8.7 | CC0-1.0 OR Apache-2.0 OR Apache-2.0 WITH LLVM-exception |
| `chrono` | 0.4.45 | MIT OR Apache-2.0 |
| `clap` | 4.6.7 | MIT OR Apache-2.0 |
| `futronic-fs80h-rs` | 0.1.1 | MIT |
| `image` | 0.25.10 | MIT OR Apache-2.0 |
| `moxcms` | 0.8.1 | BSD-3-Clause OR Apache-2.0 |
| `openjpeg2-pure-rs` | 0.1.1 | BSD-2-Clause |
| `png` | 0.17.16 | MIT OR Apache-2.0 |
| `rayon` | 1.12.0 | MIT OR Apache-2.0 |
| `rusb` | 0.9.4 | MIT |
| `rustfft` | 6.4.1 | MIT OR Apache-2.0 |
| `serde` / `serde_json` | 1.0.229 / 1.0.154 | MIT OR Apache-2.0 |
| `sha2` | 0.10.9 | MIT OR Apache-2.0 |
| `tiff` | 0.9.1 | MIT |
| `tokio` | 1.54.1 | MIT |
| `tracing` / `tracing-subscriber` | 0.1.44 / 0.3.23 | MIT |
| `wsq-codec` | 0.2.0 | MIT |

Their transitive dependencies are declared under permissive SPDX expressions
in the lockfile, including MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause,
Zlib, CC0-1.0, Unlicense, and Unicode-3.0.

## libusb notice

`rusb` is built with the `vendored` feature in this project. That feature can
compile and statically link the bundled libusb implementation through
`libusb1-sys`. The Rust wrapper crate is MIT licensed, while libusb itself is
licensed under LGPL-2.1-or-later.

Before distributing an artifact that statically links libusb, the distributor
must satisfy the applicable LGPL obligations, including providing the libusb
license and corresponding source or relinking materials. Do not represent this
notice as legal advice; obtain legal review for the intended distribution model.

The release pipeline copies this notice into every archive. The release owner
must verify whether each built artifact links libusb statically and attach the
required libusb materials before publishing it.
