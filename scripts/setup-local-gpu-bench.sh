#!/usr/bin/env bash
set -Eeuo pipefail

# ============================================================
# Accelerate local GPU benchmark environment bootstrap
#
# Responsibilities
# ------------------------------------------------------------
# Third-party software:
#   - Python 3.12
#   - PyTorch 2.14.0 + CUDA 13.0
#   - numpy / onnx / onnxruntime / etc.
#   - maturin
#
# Project code:
#   - accelerate-native
#   - accelerate-runtime
#   - adapter-runtime
#   - augment-chess-engine
#   - augment-chess-contracts
#   - accelerate_chess Python package
#
# Project code is ALWAYS built from the CURRENT repository checkout.
# It is NEVER downloaded from PyPI or another project package registry.
#
# This script does NOT:
#   - modify uv.lock
#   - modify the repository's CPU PyTorch configuration
#   - install/update Rust
#   - switch Git branches
# ============================================================


# ------------------------------------------------------------
# Configuration
# ------------------------------------------------------------

PYTHON="${PYTHON:-python3.12}"

BENCH_ROOT="${BENCH_ROOT:-${XDG_CACHE_HOME:-$HOME/.cache}/accelerate}"
BENCH_BUILD="${BENCH_BUILD:-$BENCH_ROOT/build/linux/local-bench}"

VENV="$BENCH_BUILD/venv"

CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$BENCH_BUILD/cargo}"

TORCH_VERSION="${TORCH_VERSION:-2.14.0}"
TORCH_CUDA_VERSION="${TORCH_CUDA_VERSION:-13.0}"
TORCH_INDEX="${TORCH_INDEX:-https://download.pytorch.org/whl/cu130}"

CLEAN="${CLEAN:-0}"
PURGE_PIP_CACHE="${PURGE_PIP_CACHE:-0}"

INSTALL_ONNXRUNTIME="${INSTALL_ONNXRUNTIME:-1}"
INSTALL_TEST_DEPS="${INSTALL_TEST_DEPS:-1}"


# ------------------------------------------------------------
# Logging / errors
# ------------------------------------------------------------

log() {
    printf '\n\033[1;34m==> %s\033[0m\n' "$*"
}

warn() {
    printf '\n\033[1;33mWARNING: %s\033[0m\n' "$*" >&2
}

die() {
    printf '\n\033[1;31mERROR: %s\033[0m\n' "$*" >&2
    exit 1
}

on_error() {
    local code=$?

    printf '\n\033[1;31mSetup failed at line %s (exit %s)\033[0m\n' \
        "${BASH_LINENO[0]:-unknown}" \
        "$code" >&2

    return "$code"
}

trap on_error ERR


# ------------------------------------------------------------
# Find repository
# ------------------------------------------------------------

command -v git >/dev/null 2>&1 ||
    die "git was not found."

REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null)" ||
    die "Run this script from inside the Accelerate repository."

cd "$REPO_ROOT"

PROJECT_ROOT="$REPO_ROOT/projects/accelerate"
NATIVE_MANIFEST="$PROJECT_ROOT/native/Cargo.toml"


# ------------------------------------------------------------
# Temporary directory
#
# IMPORTANT:
# /tmp on this machine is a quota-controlled tmpfs.
# Use /var/tmp, which resides on the normal filesystem.
#
# Use a unique directory every execution so a deleted TMPDIR
# cannot leak into later rustup/pip commands.
# ------------------------------------------------------------

mkdir -p /var/tmp

TMP_ROOT="$(
    mktemp \
        -d \
        -p /var/tmp \
        "accelerate-local-bench-${USER}.XXXXXXXX"
)"

TMP_ROOT_OWNED=1

cleanup() {
    if [[ "${TMP_ROOT_OWNED:-0}" == "1" ]] &&
       [[ -n "${TMP_ROOT:-}" ]] &&
       [[ -d "$TMP_ROOT" ]]; then
        rm -rf -- "$TMP_ROOT"
    fi
}

trap cleanup EXIT

export TMPDIR="$TMP_ROOT"
export TMP="$TMP_ROOT"
export TEMP="$TMP_ROOT"

export PIP_NO_CACHE_DIR=1
export PIP_DISABLE_PIP_VERSION_CHECK=1
export PYTHONDONTWRITEBYTECODE=1

export CARGO_TARGET_DIR


# ------------------------------------------------------------
# Repository information
# ------------------------------------------------------------

log "Repository"

echo "Root:   $REPO_ROOT"
echo "Branch: $(git branch --show-current || true)"
echo "Commit: $(git rev-parse HEAD)"

if [[ -n "$(git status --porcelain)" ]]; then
    echo "State:  DIRTY"
else
    echo "State:  clean"
fi

echo "Temp:   $TMP_ROOT"
echo "Build:  $BENCH_BUILD"


# ------------------------------------------------------------
# Python check
# ------------------------------------------------------------

log "Checking Python"

command -v "$PYTHON" >/dev/null 2>&1 ||
    die "$PYTHON was not found."

"$PYTHON" - <<'PY'
import sys

print("Python:", sys.version.split()[0])

if sys.version_info[:2] != (3, 12):
    raise SystemExit(
        "This checkout requires Python 3.12."
    )
PY


# ------------------------------------------------------------
# Rust check
#
# Do NOT install or update Rust here.
# The repository currently requires rustc >= 1.96.
# ------------------------------------------------------------

log "Checking Rust toolchain"

command -v rustc >/dev/null 2>&1 ||
    die "rustc was not found."

command -v cargo >/dev/null 2>&1 ||
    die "cargo was not found."

echo "rustc: $(rustc --version)"
echo "cargo: $(cargo --version)"

RUST_VERSION="$(rustc --version | awk '{print $2}')"

"$PYTHON" - "$RUST_VERSION" <<'PY'
import sys

raw = sys.argv[1]

try:
    version = tuple(map(int, raw.split(".")[:3]))
except Exception as exc:
    raise SystemExit(
        f"Could not parse rustc version: {raw}"
    ) from exc

required = (1, 96, 0)

if version < required:
    raise SystemExit(
        f"rustc {raw} is too old. "
        "The current repository checkout requires rustc >= 1.96.0. "
        "This setup script will not install or update Rust automatically."
    )

print(f"Rust requirement: PASS ({raw} >= 1.96.0)")
PY


# ------------------------------------------------------------
# NVIDIA driver check
# ------------------------------------------------------------

log "Checking NVIDIA GPU"

command -v nvidia-smi >/dev/null 2>&1 ||
    die "nvidia-smi was not found."

nvidia-smi \
    --query-gpu=name,driver_version,memory.total \
    --format=csv,noheader


# ------------------------------------------------------------
# Disk diagnostics
# ------------------------------------------------------------

log "Disk status"

echo
echo "[HOME]"
df -h "$HOME" || true

echo
echo "[/var/tmp]"
df -h /var/tmp || true

echo
echo "[inodes]"
df -i "$HOME" /var/tmp || true

echo
echo "[existing caches]"
du -sh "$HOME/.cache/pip" 2>/dev/null || true
du -sh "$BENCH_ROOT" 2>/dev/null || true


# ------------------------------------------------------------
# Optional cleanup
# ------------------------------------------------------------

safe_remove_build() {
    local path="$1"

    if [[ -z "$path" ]] ||
       [[ "$path" == "/" ]] ||
       [[ "$path" == "$HOME" ]] ||
       [[ "$path" == "$REPO_ROOT" ]]; then
        die "Refusing unsafe cleanup path: $path"
    fi

    rm -rf -- "$path"
}

if [[ "$CLEAN" == "1" ]]; then
    log "Removing previous benchmark environment"

    safe_remove_build "$BENCH_BUILD"
fi

if [[ "$PURGE_PIP_CACHE" == "1" ]]; then
    log "Purging pip cache"

    "$PYTHON" -m pip cache purge || true
fi


# ------------------------------------------------------------
# Create/reuse virtual environment
# ------------------------------------------------------------

log "Preparing benchmark virtual environment"

mkdir -p "$BENCH_BUILD"

if [[ ! -x "$VENV/bin/python" ]]; then
    echo "Creating new venv:"
    echo "  $VENV"

    "$PYTHON" -m venv "$VENV"
else
    echo "Reusing existing venv:"
    echo "  $VENV"
fi

# shellcheck disable=SC1091
source "$VENV/bin/activate"

echo "venv Python: $(python --version)"


# ------------------------------------------------------------
# Packaging tools
# ------------------------------------------------------------

log "Preparing Python packaging tools"

python -m pip install \
    --no-cache-dir \
    --upgrade \
    pip \
    setuptools \
    wheel


# ------------------------------------------------------------
# PyTorch CUDA
#
# Do not redownload the huge CUDA stack when the existing venv
# already contains the correct PyTorch/CUDA build.
# ------------------------------------------------------------

log "Checking PyTorch CUDA installation"

TORCH_OK=0

if python - "$TORCH_VERSION" "$TORCH_CUDA_VERSION" <<'PY'
import sys

wanted_torch = sys.argv[1]
wanted_cuda = sys.argv[2]

try:
    import torch
except Exception:
    raise SystemExit(1)

actual_torch = torch.__version__.split("+", 1)[0]
actual_cuda = torch.version.cuda

print("Existing PyTorch:", torch.__version__)
print("Existing CUDA runtime:", actual_cuda)

if actual_torch != wanted_torch:
    raise SystemExit(1)

if actual_cuda != wanted_cuda:
    raise SystemExit(1)
PY
then
    TORCH_OK=1
fi

if [[ "$TORCH_OK" == "1" ]]; then
    echo "Correct CUDA PyTorch is already installed."
    echo "Skipping PyTorch download."
else
    log "Installing PyTorch ${TORCH_VERSION} + CUDA ${TORCH_CUDA_VERSION}"

    python -m pip install \
        --no-cache-dir \
        --upgrade \
        --force-reinstall \
        "torch==${TORCH_VERSION}" \
        --index-url "$TORCH_INDEX"
fi


# ------------------------------------------------------------
# Third-party Python dependencies
#
# IMPORTANT:
# Only external dependencies are installed here.
#
# DO NOT add:
#   accelerate-chess
#   accelerate-native
#   augment-chess-engine
#   adapter-runtime
#   accelerate-runtime
#
# Those are repository code and are built below from the
# current checkout.
# ------------------------------------------------------------

log "Installing third-party Python dependencies"

python -m pip install \
    --no-cache-dir \
    "numpy==2.5.3" \
    "onnx==1.23.0" \
    "onnxscript==0.7.2" \
    "safetensors==0.8.0" \
    "jcs==0.2.1" \
    "maturin==1.15.0"

if [[ "$INSTALL_ONNXRUNTIME" == "1" ]]; then
    python -m pip install \
        --no-cache-dir \
        "onnxruntime==1.30.0"
fi

if [[ "$INSTALL_TEST_DEPS" == "1" ]]; then
    python -m pip install \
        --no-cache-dir \
        "pytest==9.1.1"
fi


# ------------------------------------------------------------
# Verify repository files required for native build
# ------------------------------------------------------------

log "Checking local project sources"

[[ -f "$PROJECT_ROOT/pyproject.toml" ]] ||
    die "Missing projects/accelerate/pyproject.toml"

[[ -f "$NATIVE_MANIFEST" ]] ||
    die "Missing projects/accelerate/native/Cargo.toml"

[[ -f "$REPO_ROOT/Cargo.toml" ]] ||
    die "Missing workspace Cargo.toml"

echo "Project source:"
echo "  $PROJECT_ROOT"

echo
echo "Native manifest:"
echo "  $NATIVE_MANIFEST"

echo
echo "The following project code will be compiled from this checkout:"
echo "  accelerate-native"
echo "  accelerate-runtime"
echo "  adapter-runtime"
echo "  augment-chess-engine"
echo "  augment-chess-contracts"
echo
echo "No project package will be downloaded from PyPI."


# ------------------------------------------------------------
# Build native project from CURRENT repository checkout
#
# maturin invokes Cargo using projects/accelerate/native/Cargo.toml.
# Its project dependencies are local Cargo path dependencies.
# ------------------------------------------------------------

log "Building project native extension from current checkout"

cd "$PROJECT_ROOT"

maturin develop \
    --release \
    --manifest-path "$NATIVE_MANIFEST"

cd "$REPO_ROOT"


# ------------------------------------------------------------
# CUDA verification
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
        "PyTorch installed successfully, but CUDA is unavailable."
    )

device = torch.cuda.current_device()
props = torch.cuda.get_device_properties(device)

print(f"GPU:             {torch.cuda.get_device_name(device)}")
print(f"VRAM:            {props.total_memory / 1024**3:.2f} GiB")
print(f"BF16 supported:  {torch.cuda.is_bf16_supported()}")

# Real GPU smoke test.
a = torch.randn(
    (1024, 1024),
    device="cuda",
    dtype=torch.float32,
)

b = torch.randn(
    (1024, 1024),
    device="cuda",
    dtype=torch.float32,
)

torch.cuda.synchronize()

c = a @ b

torch.cuda.synchronize()

if not torch.isfinite(c).all():
    raise SystemExit(
        "CUDA matrix multiplication produced non-finite values."
    )

print("CUDA compute:    PASS")
PY


# ------------------------------------------------------------
# Verify Python + local PyO3 project
# ------------------------------------------------------------

log "Verifying locally built Accelerate package"

cd "$PROJECT_ROOT"

python - <<'PY'
from pathlib import Path

import accelerate_chess
import accelerate_chess._native as native

print("accelerate_chess:        PASS")
print("accelerate_chess path:  ", Path(accelerate_chess.__file__).resolve())
print("native PyO3 module:       PASS")
print("native module path:      ", Path(native.__file__).resolve())
PY


# ------------------------------------------------------------
# CLI smoke
# ------------------------------------------------------------

log "Checking Accelerate CLI"

python -m accelerate_chess.cli --help >/dev/null

echo "Accelerate CLI:           PASS"

cd "$REPO_ROOT"


# ------------------------------------------------------------
# Installed dependency versions
# ------------------------------------------------------------

log "Installed third-party versions"

python - <<'PY'
import numpy
import onnx
import safetensors
import torch

print("torch:       ", torch.__version__)
print("torch CUDA:  ", torch.version.cuda)
print("numpy:       ", numpy.__version__)
print("onnx:        ", onnx.__version__)
print("safetensors: ", safetensors.__version__)

try:
    import onnxruntime
except ImportError:
    print("onnxruntime:  not installed")
else:
    print("onnxruntime: ", onnxruntime.__version__)
PY


# ------------------------------------------------------------
# Build size
# ------------------------------------------------------------

log "Benchmark environment size"

du -sh "$VENV" 2>/dev/null || true
du -sh "$CARGO_TARGET_DIR" 2>/dev/null || true


# ------------------------------------------------------------
# Final repository provenance
# ------------------------------------------------------------

log "Build provenance"

echo "Repository: $REPO_ROOT"
echo "Branch:     $(git branch --show-current || true)"
echo "Commit:     $(git rev-parse HEAD)"
echo "Rust:       $(rustc --version)"
echo "Cargo:      $(cargo --version)"
echo "Python:     $(python --version)"
echo "Venv:       $VENV"
echo "Cargo dir:  $CARGO_TARGET_DIR"


# ------------------------------------------------------------
# Done
# ------------------------------------------------------------

cat <<EOF

============================================================
Accelerate local GPU environment: READY
============================================================

Project code:
  Built directly from the current repository checkout.

Git commit:
  $(git rev-parse HEAD)

Virtual environment:
  $VENV

Cargo build cache:
  $CARGO_TARGET_DIR

Activate this environment later with:

  source "$VENV/bin/activate"

For Cargo builds:

  export CARGO_TARGET_DIR="$CARGO_TARGET_DIR"

Quick GPU check:

  python -c 'import torch; print(torch.__version__); print(torch.cuda.get_device_name(0)); print(torch.cuda.is_available())'

Accelerate CLI:

  cd "$PROJECT_ROOT"
  python -m accelerate_chess.cli --help

Normal rerun:

  ./scripts/setup-local-gpu-bench.sh

Intentional clean rebuild:

  CLEAN=1 ./scripts/setup-local-gpu-bench.sh

Clean rebuild + pip cache purge:

  CLEAN=1 PURGE_PIP_CACHE=1 ./scripts/setup-local-gpu-bench.sh

============================================================
EOF