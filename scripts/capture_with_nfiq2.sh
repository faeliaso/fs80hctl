#!/usr/bin/env bash
#
# capture_with_nfiq2.sh - Automated Fingerprint Capture with NIST NFIQ 2 Quality Gate
# External integration between fs80hctl and /usr/local/bin/nfiq2
#
set -euo pipefail

OUTPUT="capture.tiff"
FORMAT="tiff"
MIN_SCORE=50

while [[ $# -gt 0 ]]; do
  case "$1" in
    -o|--output)
      OUTPUT="$2"
      shift 2
      ;;
    -f|--format)
      FORMAT="$2"
      shift 2
      ;;
    -m|--min-score)
      MIN_SCORE="$2"
      shift 2
      ;;
    -h|--help)
      echo "Usage: $0 [options]"
      echo "Options:"
      echo "  -o, --output <path>     Output file path (default: capture.tiff)"
      echo "  -f, --format <format>   Format: tiff, png, pgm (default: tiff)"
      echo "  -m, --min-score <score> Minimum NFIQ 2 score required (default: 50)"
      exit 0
      ;;
    *)
      echo "Unknown option: $1"
      exit 1
      ;;
  esac
done

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# 1. Locate nfiq2 binary
NFIQ2_BIN=""
for candidate in "/usr/local/bin/nfiq2" "/opt/homebrew/bin/nfiq2" "$(which nfiq2 2>/dev/null || true)"; do
  if [[ -x "$candidate" ]]; then
    NFIQ2_BIN="$candidate"
    break
  fi
done

if [[ -z "$NFIQ2_BIN" ]]; then
  echo "ERROR: Official nfiq2 binary not found in /usr/local/bin/nfiq2 or PATH."
  echo "Please install NFIQ 2 to run this quality gate."
  exit 1
fi

# 2. Locate or build fs80hctl CLI
CLI_BIN="$WORKSPACE_ROOT/target/debug/fs80hctl"
if [[ ! -x "$CLI_BIN" ]]; then
  echo "Building fs80hctl..."
  cargo build -p fs80h-cli --manifest-path "$WORKSPACE_ROOT/Cargo.toml"
fi

echo "=========================================================="
echo " Pipeline: fs80hctl Capture + NIST NFIQ 2 Quality Gate"
echo "=========================================================="
echo "Target output: $OUTPUT ($FORMAT)"
echo "Minimum NFIQ 2 threshold: $MIN_SCORE / 100"
echo

# 3. Trigger optical sensor capture
echo "[1/3] Triggering Futronic FS80H sensor..."
"$CLI_BIN" capture --format "$FORMAT" --output "$OUTPUT"
echo

# 4. Run official NIST NFIQ 2 quality assessment
echo "[2/3] Evaluating quality with official NFIQ 2 ($NFIQ2_BIN)..."
NFIQ_FLAGS="-a"
if [[ "$FORMAT" == "pgm" ]]; then
  NFIQ_FLAGS="-F -a"
fi

NFIQ_RAW="$("$NFIQ2_BIN" $NFIQ_FLAGS -i "$OUTPUT")"
DATA_LINE=$(echo "$NFIQ_RAW" | sed -n '2p')

IFS=',' read -r FILENAME FINGER_CODE QUALITY_SCORE OPT_ERROR QUANT RESAMP UNIFORM CONTRAST MINUTIAE FOREGROUND <<< "$DATA_LINE"

QUALITY_SCORE=$(echo "$QUALITY_SCORE" | tr -d '"')
MINUTIAE=$(echo "$MINUTIAE" | tr -d '"')
FOREGROUND=$(echo "$FOREGROUND" | tr -d '"')
OPT_ERROR=$(echo "$OPT_ERROR" | tr -d '"')

echo
echo "NFIQ 2 Quality Report (ISO/IEC 29794-4):"
echo "  - Unified Score: $QUALITY_SCORE / 100"
echo "  - Minutiae Count: $MINUTIAE"
echo "  - Foreground Contact Area: $FOREGROUND pixels"
if [[ "$OPT_ERROR" != "NA" ]]; then
  echo "  - Extractor Warning: $OPT_ERROR"
fi
echo

# 5. Apply Quality Gate
echo "[3/3] Evaluating Quality Gate..."
if [[ "$QUALITY_SCORE" == "NA" ]] || [[ "$QUALITY_SCORE" -lt "$MIN_SCORE" ]]; then
  echo "REJECTED: Capture did not meet minimum quality threshold ($MIN_SCORE/100)."
  echo "Hint: Clean the glass prism, center the fingerprint pad, and press firmly."
  exit 2
else
  echo "APPROVED: High biometric quality image accepted (Score: $QUALITY_SCORE >= $MIN_SCORE)."
  exit 0
fi
