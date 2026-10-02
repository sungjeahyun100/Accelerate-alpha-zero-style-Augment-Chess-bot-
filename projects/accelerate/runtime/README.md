# ONNX CPU runtime

`accelerate-runtime` is independent of the rules engine and Python. It loads
one verified `onnx-policy-value-v2` bundle and creates exactly the selected
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
and [project rules](../../../AGENTS.md) for ownership and generated-file policy.

## typed v3 계약

`onnx-policy-value-v3`는 `mask-resnet`과 `entity-transformer`의 같은 공개 typed IR을
사용한다. 후보별 노드 상한은 Python IR의 `MAX_CANDIDATE_NODES=256`이며 두 PyTorch
모델, ONNX manifest와 네이티브 runtime이 이 상한을 공유한다. 공개 선택 64개를
담는 199개 노드도 손실 없이 전달하며 257개 노드는 거부한다. 입력 64 MiB와 중간
버퍼 256 MiB 한도는 별도로 유지한다.

category ID 순서는 동결 catalog·관측 정책과 IR의 semantic symbol에 결속된다.
네이티브 로더의 vocabulary JCS SHA-256 pin은 현재 Python `TypedEncoderSpec`에서
파생한 순서를 확인한다. 공개 intent symbol이나 동결 입력을 바꾸면 pin과 관련
검증을 함께 갱신하고 기존 wheel·성공 근거를 재사용하지 않는다. manifest 내부
hash를 전부 다시 계산하더라도 category ID를 교환한 모델은 허용하지 않는다.
