# Forensic Validation with an Authorized Biometric Dataset

This procedure is for an authorized operator or evaluation agent validating the
`forensic` commands against a biometric dataset supplied outside this repository.
It is an evaluation protocol, not an identification workflow and not a certified
PAD or forensic-expert procedure.

## Non-negotiable data handling rules

1. Obtain documented authorization, participant consent where applicable, and
   confirmation of the permitted purpose, retention period, and access controls
   before accessing the dataset.
2. Do **not** copy real biometric images, custody manifests, scores, or logs into
   this repository, `tests/fixtures/`, issue trackers, pull requests, CI output,
   or agent messages. The repository's automated tests are synthetic only.
3. Keep source images and every result outside the checkout, in an access-restricted
   directory owned by the authorized operator. Use a de-identified sample ID; keep
   the identity-to-ID mapping in the dataset custodian's approved system.
4. Treat comparison scores, minutiae JSON, reports, annotated images, and generated
   side-by-side images as sensitive biometric-derived data. They require the same
   handling and retention controls as their source images.
5. Do not use this implementation to make a legal, civil, criminal, employment,
   financial, or access-control identification decision.

The general governance requirements are in [`SECURITY.md`](../SECURITY.md), and
the physical-scanner prerequisites are in [`TESTING.md`](../TESTING.md).

## Preconditions

- Build the exact revision being evaluated, preferably from a clean worktree.
- Record the Git commit, executable version, operating system, scanner model,
  scanner bus/address, and the dataset release/version in the evaluation record.
- The current decoder accepts 320×480, 8-bit grayscale images at the FS80H's
  500-DPI operating convention. Convert or reject incompatible samples outside
  the repository using the dataset's approved preparation process.
- Use a protected location; the following creates one with owner-only permissions:

```sh
umask 077
export FS80H_EVAL_DIR="${FS80H_EVAL_DIR:-$HOME/fs80h-authorized-evaluation}"
mkdir -p "$FS80H_EVAL_DIR/results"
export CLI="$(pwd)/target/release/fs80hctl"
git rev-parse HEAD >"$FS80H_EVAL_DIR/results/evaluated-commit.txt"
"$CLI" --version >"$FS80H_EVAL_DIR/results/cli-version.txt"
```

`FS80H_EVAL_DIR` must not be inside the repository. Do not use a shared or
cloud-synchronised directory unless the dataset authorization explicitly permits it.

## Dataset manifest (de-identified)

The operator should maintain a protected CSV or database **outside the repository**
with at least these fields:

| Field | Meaning |
|---|---|
| `pair_id` | Random evaluation-pair identifier. |
| `probe_path` / `reference_path` | Restricted absolute paths to the samples. |
| `pair_class` | `genuine` (same enrolled finger) or `impostor` (different finger). |
| `finger_code` | De-identified finger/session code, never a participant name. |
| `capture_session` | Session/device conditions needed to analyze repeatability. |
| `pad_class` | `bona_fide` or the authorized presentation-attack instrument class. |

Include multiple acquisitions of each enrolled finger across different placements;
include different-finger and different-person impostor pairs. Do not choose pairs
after viewing scores.

## 1:1 matcher evaluation

First choose a development partition and a completely separate holdout partition.
Only the development partition may be used to explore thresholds. The holdout set
is used once for the final reported result.

For every pair, produce a JSON result and an annotated comparison image in the
restricted results directory:

```sh
PAIR_ID='replace-with-deidentified-pair-id'
PROBE='/restricted/dataset/probe.png'
REFERENCE='/restricted/dataset/reference.png'
THRESHOLD='replace-with-development-threshold'

"$CLI" forensic compare \
  --probe "$PROBE" \
  --reference "$REFERENCE" \
  --output "$FS80H_EVAL_DIR/results/${PAIR_ID}-compare.png" \
  --threshold "$THRESHOLD" \
  >"$FS80H_EVAL_DIR/results/${PAIR_ID}-compare.json"
status=$?
printf '%s,%s\n' "$PAIR_ID" "$status" >>"$FS80H_EVAL_DIR/results/compare-exit-codes.csv"
```

The command uses exit `0` for a score at or above the supplied threshold and `2`
below it. An exit code is not a ground-truth label. Record the actual `score`,
`paired_minutiae_count`, and declared `pair_class` for every pair.

Calculate at minimum on the holdout partition:

- **FNMR/FRR**: genuine pairs below threshold divided by all genuine pairs.
- **FMR/FAR**: impostor pairs at or above threshold divided by all impostor pairs.
- confidence intervals, sample count, and a score distribution by pair class;
- results by capture session, finger code, and permitted demographic/device strata
  to identify instability or disparity.

Recent physical smoke testing exposed insufficient separation at the existing
generic threshold. This is **not** sufficient evidence to set a production
threshold. In particular, do not adopt an exploratory threshold as a default
without the development/holdout evaluation described above.

## Visual annotation and report checks

For a selected, authorized sample from each dataset condition:

```sh
SAMPLE='/restricted/dataset/sample.png'
SAMPLE_ID='replace-with-deidentified-sample-id'

"$CLI" --json forensic annotate \
  --input "$SAMPLE" \
  --output "$FS80H_EVAL_DIR/results/${SAMPLE_ID}-annotated.png" \
  >"$FS80H_EVAL_DIR/results/${SAMPLE_ID}-minutiae.json"

"$CLI" forensic report "$SAMPLE" \
  >"$FS80H_EVAL_DIR/results/${SAMPLE_ID}-report.json"
```

Check that the annotation is RGB 320×480, uses the documented marker colours,
and remains inside the image bounds. Verify that the report has version, timestamp,
source dimensions/DPI, SHA-256, BLAKE3, quality, liveness, and minutiae fields.
Do not publish these artifacts outside the approved evaluation environment.

## PAD / liveness evaluation

`forensic liveness` is a deterministic static texture-screening heuristic. It is
not validated to ISO/IEC 30107-3 or LivDet merely because it executes. Use a
separate, authorized PAD dataset containing bona-fide presentations and each
permitted attack instrument type. Keep the attack materials and data handling
under the dataset owner's safety and legal rules.

```sh
SAMPLE='/restricted/pad-dataset/sample.png'
SAMPLE_ID='replace-with-deidentified-pad-sample-id'

set +e
"$CLI" --json forensic liveness "$SAMPLE" \
  >"$FS80H_EVAL_DIR/results/${SAMPLE_ID}-liveness.json"
status=$?
set -e
printf '%s,%s\n' "$SAMPLE_ID" "$status" >>"$FS80H_EVAL_DIR/results/liveness-exit-codes.csv"
```

Exit `0` means the current heuristic score is at least 65; exit `4` means it is
below 65. Analyze bona-fide rejection and attack acceptance separately by attack
instrument. A formal PAD claim requires a pre-registered protocol, independent
holdout data, sensor-specific testing, and the appropriate ISO/IEC 30107-3 metrics
(including APCER and BPCER) under qualified oversight.

## Close-out

1. Have the authorized reviewer approve the protected aggregate evaluation record.
2. Retain data only for the documented period, then delete it using the custodian's
   approved procedure. Do not use repository cleanup commands on external data.
3. Before any commit, run:

```sh
./scripts/check-clean-workspace.sh
git status --short
```

Both commands must show that no real biometric image, manifest, report, or result
was added to the repository.
