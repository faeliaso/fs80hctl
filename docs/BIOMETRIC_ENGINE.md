# Biometric Engine Architecture (`fs80h-core`)

`fs80h-core` implements a **100% pure Rust**, deterministic fingerprint processing and matching pipeline with zero external C/C++ dependencies and optional `rayon` parallelism (`feature = "parallel"`).

## Pipeline Overview

```text
RawFrame (320x480, 8-bit, 500 DPI)
  │
  ▼
1. Quality Gatekeeper (src/quality.rs)
   ├─ Rejects dry, wet/saturated, blurry, or partial captures
   └─ Emits recapture warnings before extraction
  │
  ▼
2. Contextual Gabor Enhancement (src/enhance/gabor.rs)
   ├─ Block-aware intensity normalization + 3x3 Gaussian pore smoothing
   ├─ Smoothed doubled-angle orientation field with bilinear pixel interpolation (36 bins)
   ├─ Adaptive local ridge frequency estimation (1/20 .. 1/5 cycles/px)
   └─ Contextual 11x11 Gabor convolution + robust std normalization
  │
  ▼
3. Morphological Minutiae Extraction (src/minutiae/mod.rs)
   ├─ Foreground finger mask (8x8 blocks with 1-block boundary erosion)
   ├─ Adaptive local binarization + island removal + pinhole filling
   ├─ Zhang-Suen thinning + short-spur pruning (<= 4 px)
   └─ Crossing Number (CN == 1, CN == 3) with continuous [0, 360) angles
  │
  ▼
4. 360° Rigid Alignment & Anti-False-Positive Matcher (src/matcher.rs)
   ├─ Invariant segment hypotheses (L >= 20 px, |ΔL| <= 4 px, |s - 1.0| <= 0.12)
   ├─ Coarse Hough pose voting -> top 24 clusters evaluated in 360°
   ├─ Partial Procrustes transform + 1:1 tolerance boxes (r <= 14 px, Δθ <= 20°)
   └─ 4 ZeroFMR gates -> MatchReport
```

## Module Details

### 1. Capture Quality Gatekeeper (`src/quality.rs`)
`evaluate_quality(frame: &RawFrame) -> QualityReport` evaluates `16x16` blocks and enforces:
- `mean_local_variance >= 80.0` and `ridge_contrast >= 25.0` (rejects dry / low-contrast captures)
- `saturated_ratio <= 0.45` (rejects wet / washed-out captures)
- `usable_area_ratio >= 0.30` (rejects partial contact)
- `sharpness >= 12.0` on active blocks (rejects motion/focus blur)

`QualityReport` preserves backward compatibility with existing `.meta.json` manifests via `#[serde(default)]` on `usable_area_ratio`, `ridge_contrast`, and `sharpness`.

### 2. Contextual Gabor Enhancement (`src/enhance/gabor.rs`)
- `enhance_gabor(frame: &RawFrame)` and `enhance_gabor_image(image: &GrayscaleImage)` normalize local intensity without amplifying flat background blocks, apply a `3x3` Gaussian pre-smoothing pass to attenuate sweat pores, estimate local ridge frequencies along ridge normals, and interpolate smoothed `(cos 2θ, sin 2θ)` tensors bilinearly per pixel across `36` orientation bins (`11x11` kernel, `RADIUS = 5`).

### 3. Morphological Minutiae Extraction (`src/minutiae/mod.rs`)
- `extract_skeleton_minutiae` segments the finger foreground (`8x8` blocks, 1-block perimeter erosion), removes small islands (`< 8 px`), fills `1–2 px` sweat-pore pinholes (`fill_pinholes`), applies Zhang-Suen thinning, prunes `<= 4 px` spurs (`prune_spurs`), and extracts `MinutiaKind::RidgeEnding` (`CN == 1`) and `MinutiaKind::Bifurcation` (`CN == 3`) with continuous `[0.0, 360.0)` angles.
- `local_ridge_quality` weights coherence and variance (`(0.35 + 0.65 * coherence) * sqrt(variance / 450)`) so high-curvature core/delta minutiae are preserved.
- `extract_enhanced_minutiae` chains `enhance_gabor_image` + `extract_skeleton_minutiae`.

### 4. Rigid Alignment & Anti-False-Positive Gates (`src/matcher.rs`)
`compare_minutiae_sets` and `compare_frames` apply four mandatory ZeroFMR gates before setting `is_verified = true`:
1. **Minimum Pair Support:** `matched_minutiae >= MIN_MATCHED_PAIRS` (`10`).
2. **Pose-Clustering Consistency:** Fraction `consistency_ratio >= 0.5` of the top `K = 10` hypotheses within `±14.0 px` translation and `±10.0°` rotation of the winning pose.
3. **2x2 Affine Deformation Sanity:** Least-squares `2x2` affine fit over matched pairs (`>= 6`), rejecting physically implausible scale (`[0.85, 1.18]`) or log-shear (`> 0.35`).
4. **ZeroFMR Composite Overlap Score:**
   $$\text{Score} = \left(\frac{N_{\text{matched}}}{\sqrt{N_{\text{overlap\_P}} \cdot N_{\text{overlap\_R}}}} \times Q_{\text{pair}} \times C\right) \times 100$$

`MatchReport` exposes `alignment_rotation_deg`, `alignment_translation`, `consistency_ratio`, and `is_verified` for full auditability.

## Operational Notes (HU20 Optical Sensor)
- **FAR vs. FRR trade-off:** The `MIN_MATCHED_PAIRS = 10` and `consistency_ratio >= 0.5` gates strongly suppress impostor matches, while extreme pressure differences or `180°` upside-down captures with small central overlap can still cause false rejections on single-template enrollments. Multi-template enrollment (2–3 captures per finger) is recommended when higher genuine acceptance is required.

## Verification Commands

```bash
cd fs80hctl
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p fs80h-core --no-default-features --lib
```
