#!/usr/bin/env bash
set -euo pipefail

# Imports a tested native distribution into this repository. The destination is
# deliberately target-specific so bundles for several platforms can coexist.
project_root="$(cd "$(dirname "$0")/.." && pwd)"
source_root="${1:-/Volumes/MAIN/Workspace/futronic-fs80h-rs/dist/futronic-fs80h-native-v0.2.0-darwin-arm64}"

if [[ ! -f "$source_root/include/futronic_fs80h.h" || ! -f "$source_root/manifest.json" || ! -d "$source_root/lib" ]]; then
  echo "Invalid Futronic distribution: $source_root" >&2
  exit 2
fi

if ! grep -q 'FS80H_ABI_VERSION 2u' "$source_root/include/futronic_fs80h.h"; then
  echo "Refusing non-v2 Futronic header" >&2; exit 2
fi
expected_target="darwin-arm64"
if [[ "$(uname -s)" == Linux ]]; then expected_target="linux-$(uname -m)"; fi
python3 - "$source_root" "$expected_target" <<'PY'
import hashlib, json, pathlib, sys
root, expected = pathlib.Path(sys.argv[1]), sys.argv[2]
manifest = json.loads((root / 'manifest.json').read_text())
if manifest.get('abi_version') != 2 or manifest.get('target') != expected:
    raise SystemExit('manifest ABI/target mismatch')
for relative, wanted in manifest.get('artifacts', {}).items():
    actual = hashlib.sha256((root / relative).read_bytes()).hexdigest()
    if actual != wanted:
        raise SystemExit(f'artifact digest mismatch: {relative}')
PY

bundle_name="$(basename "$source_root")"
destination="$project_root/vendor/futronic/$bundle_name"
mkdir -p "$destination"
rsync -a --delete --exclude '.DS_Store' "$source_root/" "$destination/"

echo "Imported Futronic bundle: $destination"
echo "Verify with: cargo check --workspace"
