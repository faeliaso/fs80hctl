#!/usr/bin/env bash
set -euo pipefail

# Runs the hardware smoke test without storing biometric material in the repo.
# Optionally select a scanner with FS80H_BUS and FS80H_ADDRESS.
if [[ -n "${FS80H_BUS:-}" || -n "${FS80H_ADDRESS:-}" ]]; then
  if [[ -z "${FS80H_BUS:-}" || -z "${FS80H_ADDRESS:-}" ]]; then
    echo "Set both FS80H_BUS and FS80H_ADDRESS, or neither." >&2
    exit 2
  fi
  selected=1
else
  selected=0
fi

download_root="${FS80H_OUTPUT_DIR:-${HOME}/Downloads}"
mkdir -p "$download_root"
output_dir="$(mktemp -d "${download_root}/fs80h-physical.XXXXXX")"
output_png="$output_dir/capture.png"

echo "1/4 Listing compatible scanners"
cargo run -p fs80h-cli -- devices

echo "2/4 Opening scanner"
if [[ "$selected" -eq 1 ]]; then
  cargo run -p fs80h-cli -- device inspect --bus "$FS80H_BUS" --address "$FS80H_ADDRESS"
else
  cargo run -p fs80h-cli -- device inspect
fi

read -r -p "3/4 Place a finger on the scanner, then press Enter to capture... "
if [[ "$selected" -eq 1 ]]; then
  if [[ "${FS80H_VARIABLE_DOSE:-0}" == "1" ]]; then
    cargo run -p fs80h-cli -- capture --bus "$FS80H_BUS" --address "$FS80H_ADDRESS" --output "$output_png" --variable-dose
  else
    cargo run -p fs80h-cli -- capture --bus "$FS80H_BUS" --address "$FS80H_ADDRESS" --output "$output_png"
  fi
else
  if [[ "${FS80H_VARIABLE_DOSE:-0}" == "1" ]]; then
    cargo run -p fs80h-cli -- capture --output "$output_png" --variable-dose
  else
    cargo run -p fs80h-cli -- capture --output "$output_png"
  fi
fi

echo "4/4 Validating the captured PNG through the local verification pipeline"
cargo run -p fs80h-cli -- verify "$output_png" "$output_png" --threshold 40

echo
echo "Physical test passed. Temporary biometric artifacts: $output_dir"
echo "Review them if needed, then remove the directory manually."
