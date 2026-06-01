#!/usr/bin/env bash
# Optional script for reproducible BrainFlow native build (Phase 10).
# See docs/brainflow.md for the canonical build order; this script is for convenience.
#
# Usage:
#   BRAINFLOW_REPO_DIR=/path/to/brainflow ./scripts/build-brainflow-native.sh
#   # or from BrainFlow repo root:
#   BRAINFLOW_REPO_DIR=. ./scripts/build-brainflow-native.sh
#
# Optional env:
#   BRAINFLOW_REPO_DIR  - BrainFlow repo root (default: . if tools/build.py exists, else must set)
#   BRAINFLOW_VERSION   - Tag or version for build.py (default: 5.13.0, pinned in docs/brainflow.md)
#   BRAINFLOW_LIB_DIR   - Target dir for installed libs (default: $BRAINFLOW_REPO_DIR/rust_package/brainflow/lib)
#
# Requires: uv (for Python), BrainFlow repo at a pinned tag. On Windows use WSL or follow docs/brainflow.md manually.

set -euo pipefail

REPO_DIR="${BRAINFLOW_REPO_DIR:-.}"
VERSION="${BRAINFLOW_VERSION:-5.13.0}"
INSTALLED_LIB="${REPO_DIR}/installed/lib"
TARGET_LIB="${BRAINFLOW_LIB_DIR:-${REPO_DIR}/rust_package/brainflow/lib}"

if [[ ! -f "${REPO_DIR}/tools/build.py" ]]; then
  echo "Error: tools/build.py not found in BRAINFLOW_REPO_DIR=${REPO_DIR}. Set BRAINFLOW_REPO_DIR to BrainFlow repo root." >&2
  echo "See docs/brainflow.md for manual steps." >&2
  exit 1
fi

echo "Building BrainFlow C++ (version ${VERSION}) in ${REPO_DIR}..."
# Use uv for Python per project policy (AGENTS.md)
(cd "${REPO_DIR}" && uv run python tools/build.py --brainflow-version "${VERSION}")

if [[ ! -d "${INSTALLED_LIB}" ]]; then
  echo "Error: installed/lib not found after build at ${INSTALLED_LIB}" >&2
  exit 1
fi

mkdir -p "${TARGET_LIB}"
echo "Copying installed/lib/* to ${TARGET_LIB}..."
cp -v "${INSTALLED_LIB}"/* "${TARGET_LIB}/"

echo "Done. Next: build Rust crate from ${REPO_DIR}/rust_package/brainflow, then NeuroHID with --features brainflow,brainflow-native (see docs/brainflow.md)."
