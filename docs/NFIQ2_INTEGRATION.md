# User Guide: External Integration of fs80hctl with NIST NFIQ 2

This document provides a practical integration guide and quality pipeline between the **`fs80hctl`** CLI and the official **NFIQ 2** utility (NIST Fingerprint Image Quality v2.3.0 / ISO/IEC 29794-4:2017).

Without requiring C dependencies, bindings, or compilation complexity inside the pure Rust codebase, integration is achieved through a clean, file-based workflow using standardized forensic formats (**PNG**, **TIFF 500 DPI**, and **Netpbm P5 PGM**) and actionable NFIQ 2 metrics.

---

## 1. Integration Architecture Overview

```text
┌─────────────────┐       ┌────────────────────────┐       ┌──────────────────────┐
│   USB Scanner   │       │   fs80hctl capture     │       │   /usr/local/bin/    │
│  Futronic FS80H ├──────►│   (L8 320x480, 500 DPI)├──────►│   nfiq2 -a -i        │
└─────────────────┘       └───────────┬────────────┘       └──────────┬───────────┘
                                      │                               │
                                      ▼                               ▼
                          digital.{png|tiff|pgm}              Actionable CSV Output
                          digital.meta.json                   (Score, Minutiae, Area)
                                      │                               │
                                      └───────────────┬───────────────┘
                                                      │
                                                      ▼
                                       ┌──────────────────────────────┐
                                       │ Quality Gate (Script / CI)   │
                                       │ Score >= 50? Reject / Accept │
                                       └──────────────────────────────┘
```

---

## 2. Format Compatibility with NFIQ 2

NFIQ 2 strictly requires images with a resolution of **500 DPI/PPI**. Format handling with `fs80hctl` behaves as follows:

| `fs80hctl` Format | Resolution Metadata | NFIQ 2 Behavior | Recommended Command |
|---|---|---|---|
| **TIFF (`.tiff`)** | `ResolutionUnit=Inch`, `X/YResolution=500` | **Native immediate recognition** without prompts or warnings. | `nfiq2 -a -i digital.tiff` |
| **PNG (`.png`)** | `pHYs` chunk at 19,685 px/m (500 DPI) | **Native immediate recognition**. | `nfiq2 -a -i digital.png` |
| **PGM (`.pgm`)** | No DPI metadata (Netpbm specification) | NFIQ 2 asks for confirmation (`Assume 500 PPI? (y/[N])`). Use `-F` for non-interactive execution. | `nfiq2 -F -a -i digital.pgm` |

> [!TIP]
> For fully automated forensic pipelines with NFIQ 2, **TIFF** and **PNG** are the recommended formats, as both embed 500 DPI resolution tags directly in their headers.

---

## 3. Essential NFIQ 2 Commands

The official binary located at `/usr/local/bin/nfiq2` provides several evaluation modes:

### 3.1 Fast Unified Score (0 to 100)
Calculates only the overall NFIQ 2 quality score:
```bash
/usr/local/bin/nfiq2 -i digital.tiff
```
*Output:*
```text
"Filename",FingerCode,QualityScore,"OptionalError"
"digital.tiff",0,62,NA
```

### 3.2 Actionable Quality Features (`-a`) — Recommended
Provides actionable diagnostic metrics explaining why an image scored high or low:
```bash
/usr/local/bin/nfiq2 -a -i digital.tiff
```
*Example CSV output (10 columns):*
```text
"Filename",FingerCode,QualityScore,"OptionalError",Quantized,Resampled,UniformImage,EmptyImageOrContrastTooLow,FingerprintImageWithMinutiae,SufficientFingerprintForeground
"digital.tiff",0,62,NA,0,0,80.78536,158.60615,54,84517
```

### 3.3 Full Feature Dump (`-v`)
Outputs all detailed feature measurements (Gabor filters, Orientation Certainty Level, Frequency Domain Analysis, etc.):
```bash
/usr/local/bin/nfiq2 -v -i digital.tiff
```

---

## 4. NFIQ 2 Column Reference (`-a`)

When executed with `-a`, NFIQ 2 returns 10 fields useful for operational quality decisions:

1. **`QualityScore`** ($0\text{--}100$):
   - NIST unified quality score predicting AFIS matching performance.
   - **$0\text{--}29$**: Poor quality / unusable (high risk of false matches or rejections).
   - **$30\text{--}49$**: Marginal quality.
   - **$50\text{--}69$**: Good quality (standard for civil, corporate, and access enrollment).
   - **$70\text{--}100$**: Excellent / forensic-grade quality.
2. **`FingerprintImageWithMinutiae`** (Integer):
   - Estimated count of usable minutiae extracted by FingerJet.
   - Typical range: $35$ to $90$ minutiae for well-positioned adult fingers. Counts below $20$ suggest partial contact or extreme fingertip placement.
3. **`SufficientFingerprintForeground`** (Integer):
   - Number of foreground ridge pixels in contact with the optical prism.
   - The full frame consists of $153,600$ pixels ($320 \times 480$). A typical contact area contains $60,000$ to $110,000$ pixels.
4. **`UniformImage`** and **`EmptyImageOrContrastTooLow`** (Float):
   - Contrast and uniformity indicators (helps detect dry fingers vs excessively oily/saturated placements).
5. **`OptionalError`** (String):
   - `NA` indicates clean execution.
   - Error messages such as `FRFXLL_ERR_FB_TOO_SMALL_AREA` alert that the subject placed only the tip of their finger on the sensor.

---

## 5. Step-by-Step Workflow

### Step 1: Capture with `fs80hctl`
Capture directly into your chosen format:
```bash
# Capture as forensic 500 DPI TIFF (AFIS standard with dark ridges)
cargo run -p fs80h-cli -- capture --format tiff --output digital.tiff

# Or capture as standard PNG
cargo run -p fs80h-cli -- capture --output digital.png
```

### Step 2: Inspect Metadata and Local Quality
```bash
cargo run -p fs80h-cli -- inspect digital.tiff
```

### Step 3: Run Official NIST NFIQ 2 Assessment
```bash
/usr/local/bin/nfiq2 -a -i digital.tiff
```

### Step 4: Transcode to PGM or RAW (for NIST NBIS or Python Pipelines)
```bash
# Transcode to PGM while preserving root custody
cargo run -p fs80h-cli -- convert digital.tiff --to pgm -o digital.pgm

# Verify with NFIQ 2 using the -F force flag
/usr/local/bin/nfiq2 -F -a -i digital.pgm
```

---

## 6. Automated Capture Script with Quality Gate

To automate quality verification and automatically reject captures that fail minimum standards, use [`scripts/capture_with_nfiq2.sh`](../scripts/capture_with_nfiq2.sh):

```bash
# Run capture and require NFIQ 2 score >= 50
./scripts/capture_with_nfiq2.sh --output approved.tiff --min-score 50
```

When quality meets the threshold:
1. The script saves the image and `.meta.json` custody manifest.
2. Extracts NFIQ 2 metrics (`score`, `minutiae`, `foreground`).
3. Returns exit code `0`. If quality is below the threshold, it rejects the capture with exit code `2` and instructs the operator to reposition the finger.
