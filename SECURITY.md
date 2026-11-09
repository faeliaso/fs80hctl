# Security Policy & Biometric Data Governance

## 1. Operational Scope & Boundaries

`fs80hctl` is a technical control, diagnostics, and image processing utility for Futronic FS80, FS80H, and FS81H fingerprint scanners.

- **Technical Scope**: The 1:1 similarity computation and quality reports produced by `fs80h-core` are deterministic diagnostic engineering metrics.
- **Strict Usage Boundaries**:
  - This software **does not** perform legal, civil, or criminal identification.
  - It **does not** issue expert forensic testimonies or certified reports.
  - It **does not** provide guaranteed presentation attack detection (PAD / *liveness / anti-spoofing*) against physical spoofing attacks.
  - It **must not** be used as the sole authentication factor in mission-critical physical or financial access control systems without multi-factor defense layers and explicit consent.

## 2. Protection & Lifecycle of Biometric Data

1. **Minimization**:
   - Biometric frame capture should only occur upon express, on-demand operator command.
   - The repository configuration and `.gitignore` strictly prohibit committing any raw biometric imagery or manifests outside the synthetic test folder (`tests/fixtures/`).
2. **Custody & Integrity**:
   - The SHA-256 custody hash is computed directly on the raw optical sensor buffer before saving to disk.
   - Metadata contains technical hardware attributes for device traceability and auditability.
3. **Retention & Disposal**:
   - Store real biometric frames only in memory or in restricted temporary directories (`mktemp -d` with `0700` permissions), securely disposing of them immediately after their intended technical verification.

## 3. Responsible Vulnerability Reporting

If you discover a security vulnerability in this project:
- **Do not open a public issue** detailing exploitable vulnerabilities or potential data leaks.
- Send a private report to the project maintainers or open a private *Security Advisory* on GitHub.
- Include reproduction steps, expected impact, and suggested mitigations.
- Security reports are prioritized with high urgency.
