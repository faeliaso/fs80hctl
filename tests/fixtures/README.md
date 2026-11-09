# Synthetic Fixtures

`verify_pipeline.rs` generates four deterministic 320×480 L8 PNG images in a temporary directory: probe, shifted match, perpendicular mismatch, and noisy degraded frame. They do not represent human biometrics and allow executing the entire test suite without physical hardware.
