#!/usr/bin/env bash
set -Eeuo pipefail

# ============================================================
# Accelerate local GPU benchmark environment bootstrap
#
# Target:
#   Python 3.12
#   PyTorch 2.14.0 + CUDA 13.0
#   Rust / maturin native extension
#
# This script does NOT modify the repository's CPU-only uv.lock.
# ============================================================

PYTHON="${PYTHON:-python3.12}"

BENCH_ROOT="${BENCH_ROOT:-${XDG_CACHE_HOME:-$HOME/.cache}/accelerate}"
BENCH_BUILD="${BENCH_BUILD:-$BENCH_ROOT/build/linux/local-bench}"
VENV="$BENCH_BUILD/venv"
CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$BENCH_BUILD/cargo}"

TORCH_VERSION="${TORCH_VERSION:-2.14.0}"
TORCH_INDEX="${TORCH_INDEX:-https://download.pytorch.org/whl/cu130}"

CLEAN="${CLEAN:-0}"
PURGE_PIP_CACHE="${PURGE_PIP_CACHE:-0}"

# ------------------------------------------------------------
# Helpers
# ------------------------------------------------------------

log() {
    printf '\n\033[1;34m==> %s\033[0m\n' "$*"
}

die() {
    printf '\n\033[1;31mERROR: %s\033[0m\n' "$*" >&2
    exit 1
}

on_error() {
    local exit_code=$?
    printf '\n\033[1;31mSetup failed at line %s (exit %s)\033[0m\n' \
        "${BASH_LINENO[0]}" "$exit_code" >&2
    exit "$exit_code"
}

trap on_error ERR

# ------------------------------------------------------------
# Find repository root
# ------------------------------------------------------------

if ! REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null)"; then
    die "Run this script from inside the Accelerate repository."
fi

cd "$REPO_ROOT"

log "Repository"
echo "$REPO_ROOT"

# ------------------------------------------------------------
# Check required programs
# ------------------------------------------------------------

log "Checking tools"

command -v "$PYTHON" >/dev/null ||
    die "$PYTHON was not found."

"$PYTHON" --version

command -v cargo >/dev/null ||
    die "cargo was not found. Install/update Rust first."

command -v rustc >/dev/null ||
    die "rustc was not found."

rustc --version
cargo --version

command -v nvidia-smi >/dev/null ||
    die "nvidia-smi was not found."

nvidia-smi --query-gpu=name,driver_version,memory.total,power.limit \
    --format=csv,noheader

# ------------------------------------------------------------
# Disk diagnostics
# ------------------------------------------------------------

log "Disk status"

df -h "$HOME" || true
df -i "$HOME" || true

echo
echo "Existing cache sizes:"
du -sh "$HOME/.cache/pip" 2>/dev/null || true
du -sh "$BENCH_ROOT" 2>/dev/null || true

# ------------------------------------------------------------
# Optional cleanup
# ------------------------------------------------------------

if [[ "$CLEAN" == "1" ]]; then
    log "Removing previous benchmark environment"
    rm -rf "$BENCH_BUILD"
fi

if [[ "$PURGE_PIP_CACHE" == "1" ]]; then
    log "Purging pip cache"
    "$PYTHON" -m pip cache purge || true
fi

# ------------------------------------------------------------
# Create venv
# ------------------------------------------------------------

log "Creating Python virtual environment"

mkdir -p "$BENCH_BUILD"

if [[ ! -x "$VENV/bin/python" ]]; then
    "$PYTHON" -m venv "$VENV"
fi

# shellcheck disable=SC1091
source "$VENV/bin/activate"

python --version

# Never create a large pip download cache for CUDA wheels.
export PIP_NO_CACHE_DIR=1

# Keep all Cargo artifacts outside the repository.
export CARGO_TARGET_DIR

# Avoid Python bytecode litter in the checkout.
export PYTHONDONTWRITEBYTECODE=1

# ------------------------------------------------------------
# Python tooling
# ------------------------------------------------------------

log "Updating pip"

python -m pip install --no-cache-dir --upgrade pip setuptools wheel

# ------------------------------------------------------------
# CUDA PyTorch
# ------------------------------------------------------------

log "Installing PyTorch ${TORCH_VERSION} CUDA build"

python -m pip install \
    --no-cache-dir \
    "torch==${TORCH_VERSION}" \
    --index-url "$TORCH_INDEX"

# ------------------------------------------------------------
# Accelerate Python dependencies
#
# Deliberately do NOT `uv sync` here because the project uv
# configuration currently pins Torch to the CPU-only index.
# ------------------------------------------------------------

log "Installing project dependencies"

python -m pip install --no-cache-dir \
    "numpy==2.5.3" \
    "onnx==1.23.0" \
    "onnxscript==0.7.2" \
    "safetensors==0.8.0" \
    "jcs==0.2.1" \
    "pytest==9.1.1" \
    "maturin==1.15.0"

# Optional validation backend.
if [[ "${INSTALL_ONNXRUNTIME:-1}" == "1" ]]; then
    python -m pip install --no-cache-dir \
        "onnxruntime==1.30.0"
fi

# ------------------------------------------------------------
# Build/install native PyO3 extension
# ------------------------------------------------------------

log "Building Accelerate native extension"

cd "$REPO_ROOT/projects/accelerate"

maturin develop --release

cd "$REPO_ROOT"

# ------------------------------------------------------------
# Verify CUDA
# ------------------------------------------------------------

log "Verifying PyTorch CUDA"

python - <<'PY'
import sys
import torch

print(f"Python:          {sys.version.split()[0]}")
print(f"PyTorch:         {torch.__version__}")
print(f"Torch CUDA:      {torch.version.cuda}")
print(f"CUDA available:  {torch.cuda.is_available()}")

if not torch.cuda.is_available():
    raise SystemExit(
        "CUDA is not available from PyTorch. "
        "Check the NVIDIA driver and installed Torch wheel."
    )

device = torch.cuda.current_device()
props = torch.cuda.get_device_properties(device)

print(f"GPU:              {torch.cuda.get_device_name(device)}")
print(f"VRAM:             {props.total_memory / 1024**3:.2f} GiB")
print(f"BF16 supported:   {torch.cuda.is_bf16_supported()}")

# Small real CUDA calculation, not just device discovery.
a = torch.randn((1024, 1024), device="cuda")
b = torch.randn((1024, 1024), device="cuda")

torch.cuda.synchronize()
c = a @ b
torch.cuda.synchronize()

if not torch.isfinite(c).all():
    raise SystemExit("CUDA smoke calculation returned non-finite values.")

print("CUDA compute:     PASS")
PY

# ------------------------------------------------------------
# Verify project/native module
# ------------------------------------------------------------

log "Verifying accelerate_chess"

python - <<'PY'
import accelerate_chess

print("accelerate_chess import: PASS")

try:
    import accelerate_chess._native
    print("native PyO3 module:       PASS")
except Exception as exc:
    raise SystemExit(f"native PyO3 module failed: {exc}")
PY

# ------------------------------------------------------------
# Basic package information
# ------------------------------------------------------------

log "Installed versions"

python - <<'PY'
import numpy
import torch
import onnx
import safetensors

print("torch       ", torch.__version__)
print("numpy       ", numpy.__version__)
print("onnx        ", onnx.__version__)
print("safetensors ", safetensors.__version__)
PY

# ------------------------------------------------------------
# Final disk usage
# ------------------------------------------------------------

log "Environment size"

du -sh "$VENV" || true
du -sh "$CARGO_TARGET_DIR" 2>/dev/null || true

# ------------------------------------------------------------
# Done
# ------------------------------------------------------------

cat <<EOF

============================================================
Local GPU benchmark environment is ready.
============================================================

Activate later with:

  source "$VENV/bin/activate"

Cargo target:

  export CARGO_TARGET_DIR="$CARGO_TARGET_DIR"

Quick GPU check:

  python -c 'import torch; print(torch.cuda.get_device_name(0)); print(torch.cuda.is_available())'

CLI check:

  cd "$REPO_ROOT/projects/accelerate"
  python -m accelerate_chess.cli --help

Clean rebuild:

  CLEAN=1 bash scripts/setup-local-gpu-bench.sh

Clean rebuild + pip cache purge:

  CLEAN=1 PURGE_PIP_CACHE=1 bash scripts/setup-local-gpu-bench.sh

============================================================
EOF