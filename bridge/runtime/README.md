# ONNX CPU runtime

`accelerate-runtime` is independent of the rules engine and Python. It loads
one verified `onnx-policy-value-v1` bundle and creates exactly the selected
CPU backend: `ort` by default, or explicitly `tract`. Errors propagate; a
rejected backend or model does not trigger a different implementation.

The implementation pins `ort=2.0.0-rc.13` with the official ONNX Runtime 1.28
CPU binaries and `tract=0.23.8`. It uses the
[public tract API](https://github.com/sonos/tract/blob/v0.23.8/api/rs/src/lib.rs)
for loading and preparing models, rather than depending directly on tract's
internal crates. The Python `network.artifacts.OnnxEvaluator` is a separate
reference validator; production calls use `accelerate_chess.ProductionEvaluator`
and the native `InferenceSession`.

```python
from accelerate_chess import ProductionEvaluator

evaluator = ProductionEvaluator(manifest_path, encoder_spec, backend="ort")
policy_logits, value = evaluator.evaluate(board, condition, action_features)
```

All inputs are finite float32 NumPy arrays: board `[B,C,8,8]`, condition `[B,D]`,
and candidate features `[B,A,F]`. Batch and candidate axes remain independent
and dynamic. Outputs are owned arrays `[B,A]` and `[B,1]`; value is from
`observation.viewer`'s perspective. The Python binding copies logical array
values before detaching from Python, including strided arrays, and serializes
calls to an individual session with an owned Rust mutex. No Python or NumPy
borrow crosses into a backend.

Before creating a backend, the loader checks exact manifest fields, model and
encoder JCS hashes, the frozen rules/catalog and ID order, static adapter
compatibility, architecture dimensions, and the ONNX file SHA-256. Both backends
receive the same verified bytes, so reopening a changed file cannot replace the
validated model. The ONNX boundary checks FP32 inputs/outputs, opset 18,
input/output names, static feature dimensions, shared dynamic axes, finite
weights, external-data rejection and FiLM data connectivity to both outputs.

Resource defaults are batch ≤64, candidates ≤4096, combined input elements
≤16,777,216 and one CPU thread. The loader permits at most 64 million aggregate
model parameters, a 2 MiB manifest and a 512 MiB model file. A conservative
combined input and intermediate-buffer estimate must fit 256 MiB. Invalid limits or a workload
exceeding these bounds fail explicitly. `ort` accepts an explicit thread count
from 1 to 64; the current tract API boundary accepts `threads=1`.

Linux validation includes actual wheel installation, a small model and the full
default ResNet with 8 residual blocks and 128 channels, base and nonzero static
LoRA bundles, original/merged/PyTorch/Rust ort/Rust tract parity, variable B/A and
changed FiLM conditions at `atol=1e-5, rtol=1e-4`. Export preserves the live model.
The strict encoder contract accepts explicit `full` history and the versioned
`public-history-summary-v1` policy. Each mode has a distinct encoder hash and
is validated with both backends. The summary changes only neural features;
the tracker and replay retain the complete public history.
The action policy likewise distinguishes `exact-payload` from
`public-decision-intent-v1`. The latter uses source UI choice identities while
keeping hidden-world execution payloads in the native rules boundary. The
loader validates both metadata policies and never relabels one as the other.
No learning campaign or playing-strength assessment belongs to these tests.
Generated bundles and numerical reports remain in external fixed artifact slots.

The observed Linux wheel requires glibc 2.39 (`manylinux_2_39_x86_64`), so Linux
CI uses Ubuntu 24.04. Windows native execution requires separate observed CI
evidence; Linux parity does not establish Windows completion. Source-package
validation also builds an sdist, extracts it outside the checkout and builds an
installable wheel from that source. See [native binding](../native/README.md)
and [project rules](../../AGENTS.md) for ownership and generated-file policy.
