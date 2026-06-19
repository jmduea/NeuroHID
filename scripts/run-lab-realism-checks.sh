#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage:
  scripts/run-lab-realism-checks.sh simulator
  scripts/run-lab-realism-checks.sh hil

Simulator mode expects an external LSL outlet that publishes a known stream:
  NEUROHID_LSL_EXPECTED_NAME=<stream-name> scripts/run-lab-realism-checks.sh simulator

Hardware-in-the-loop mode requires an explicit acknowledgment and a lab LSL publisher:
  NEUROHID_HIL=1 NEUROHID_HIL_LSL_PREDICATE="type='EEG'" scripts/run-lab-realism-checks.sh hil

Optional:
  NEUROHID_LSL_TIMEOUT_SECS=5
  NEUROHID_HIL_MIN_STREAMS=1
USAGE
}

mode="${1:-}"
case "${mode}" in
  simulator)
    if [[ -z "${NEUROHID_LSL_EXPECTED_NAME:-}" ]]; then
      echo "NEUROHID_LSL_EXPECTED_NAME is required for simulator checks." >&2
      usage >&2
      exit 2
    fi
    cargo test -p neurohid-device --features lsl \
      lsl_simulator_stream_is_discoverable_with_expected_metadata \
      -- --ignored --nocapture
    ;;
  hil)
    if [[ "${NEUROHID_HIL:-}" != "1" ]]; then
      echo "Set NEUROHID_HIL=1 to acknowledge hardware-in-the-loop checks." >&2
      usage >&2
      exit 2
    fi
    cargo test -p neurohid-device --features lsl \
      lsl_hardware_stream_is_discoverable_for_lab_realism \
      -- --ignored --nocapture
    ;;
  *)
    usage >&2
    exit 2
    ;;
esac
