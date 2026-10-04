# 로컬 성능 측정

이 도구는 `accelerate_chess.bench` 모듈을 실행한다. 결과는 실행 환경, Git SHA,
설정, 성공·미지원 상태를 JSON으로 남긴다. `--run-id`를 지정하면 저장소 밖의
`reports/<run-id>/`에 기록한다. 기본 루트는 기존 `artifact_root()` 정책을 따른다.
WSL에서 호스트 APPDATA를 찾지 못하면 `--artifact-root`로 저장소 밖의 경로를 명시한다.
같은 run ID는 덮어쓰지 않는다. 원시 보고서의 hostname과 경로는 공유 전에 제거한다.

## 측정 결과 보고서 만들기

1. 아래 벤치마크 명령에 `--run-id`를 붙여 실행한다.
2. 원시 JSON은 기존 외부 artifact root의 `reports/<run-id>/<kind>.json`에 축적된다.
3. 같은 Python 환경에 `matplotlib`을 설치하고 보고서 생성기를 실행한다. 저장소
   내부 패키지를 PyPI에서 설치하지 말고 현재 checkout에서 빌드한 패키지를 사용한다.
4. 저장소의 `projects/accelerate/bench/results/<report-id>/`를 확인한다.
5. `summary.md`와 실제 생성된 PNG를 성능 보고서의 측정 근거로 사용한다.

`matplotlib`은 보고서 렌더링 때만 필요하다. 벤치마크 자체와 CPU 검증 환경의
기존 의존성에는 추가하지 않는다. 현재 잠금 파일에는 포함되지 않았으므로 보고서
환경에서 별도 설치한다. 예를 들어 CUDA 벤치마크용 가상환경을 활성화한 뒤
`python -m pip install 'matplotlib==3.10.7'`을 실행한다.

```bash
# 현재 checkout에서 빌드한 accelerate_chess 패키지가 설치된 환경에서 실행
python -m accelerate_chess.bench.report --report-id local-baseline-001 \
  --runs engine-w1 engine-w2 inference-resnet-s-fp32 inference-resnet-s-bf16

# WSL에서 호스트 APPDATA 탐지가 불가능한 경우에만 외부 루트를 명시
python -m accelerate_chess.bench.report \
  --artifact-root /external/Accelerate --report-id local-baseline-002
```

`--runs`가 없으면 외부 artifact root의 `reports/` 바로 아래에 있는 안전한 run ID
디렉터리 전체에서 `*.json`을 사전순으로 읽는다. 지원 버전은
`local-performance-v1`, 종류는 engine/inference/training/typed-training/pipeline이다. 잘못된
JSON과 다른 버전은 오류로 중단한다. CPU/GPU 구성이 다른 run도 한 보고서로
합치지 않으며 `--runs`로 분리해야 한다. Git SHA가 섞이면 보고서에 경고한다.
`unsupported`/`oom`/실패는 숫자 0 대신 이유와 함께 불완전 측정 표에 남긴다.
현재 pipeline은 모두 `unsupported`이므로 pipeline 그래프는 생성되지 않는다.

보고서에는 `summary.md`, 정규화된 `summary.json`, 모든 source run의 상대 경로와
설정을 담은 `metadata.json`을 쓴다. 성공한 종류에 대해서만 engine operation 및
worker scaling, inference 처리량·지연·VRAM, synthetic training 처리량·단계
시간·VRAM PNG를 만든다. `results/`는 Git에서 무시되며 같은 report ID는 덮어쓰지
않는다. 원시 JSON은 읽기만 한다. Markdown에는 hostname, artifact root의 절대
경로, 체크포인트 경로를 담지 않는다. `metadata.json`의 source JSON 경로도 외부
artifact root 기준 상대 경로다. 공유 전에는 보고서의 환경 정보와 실패 이유에
민감한 문자열이 남지 않았는지 확인한다.

## 환경

CPU correctness/CI는 기존 `uv.lock`과 `pytorch-cpu` index를 그대로 사용한다.
`projects/accelerate`에서 `uv sync --locked --all-extras --no-editable`을 실행한다.

CUDA 성능 측정은 **별도** Linux Python 3.12 가상환경에서 실행한다. PyTorch 공식
[cu130 wheel index](https://download.pytorch.org/whl/cu130/torch/)에는
`torch-2.14.0+cu130-cp312-cp312-manylinux_2_28_x86_64.whl`이 있다.
드라이버가 CUDA 13.0 런타임을 지원하는지 `nvidia-smi`와 설치 뒤의
`torch.cuda.is_available()`로 확인한다. 로컬 13.2 드라이버 표시만으로 설치 성공을
추정하지 않는다. 이 절차는 CPU lockfile을 수정하지 않는다.

```bash
cd projects/accelerate
export BENCH_VENV="${XDG_CACHE_HOME:-$HOME/.cache}/accelerate/build/linux/local-bench/venv"
export BENCH_BUILD="${XDG_CACHE_HOME:-$HOME/.cache}/accelerate/build/linux/local-bench"
python3.12 -m venv "$BENCH_VENV"
source "$BENCH_VENV/bin/activate"
python -m pip install 'torch==2.14.0' --index-url https://download.pytorch.org/whl/cu130
python -m pip install 'numpy==2.5.3' 'onnx==1.23.0' 'onnxscript==0.7.2' 'safetensors==0.8.0' 'jcs==0.2.1' 'maturin==1.15.0'
export CARGO_TARGET_DIR="$BENCH_BUILD/cargo"
python -m maturin build --release --out "$BENCH_BUILD/wheels"
python -m pip install --no-deps "$BENCH_BUILD"/wheels/accelerate_chess-*.whl
python -c 'import torch; assert torch.cuda.is_available(); print(torch.__version__, torch.version.cuda, torch.cuda.get_device_name())'
```

`maturin build`의 작업 디렉터리는 `projects/accelerate`다. `--no-deps`는
CPU 전용 lockfile의 torch 선택이 CUDA 환경에 다시 적용되지 않게 한다.
Linux/WSL의
재생성 가능한 venv와 Cargo 빌드는 위 고정 캐시 슬롯에 두며, 보존할 결과는
호스트 APPDATA 아래 `Accelerate` 루트로 보낸다. GPU 전력과 활용률은 별도
터미널에서 `nvidia-smi dmon -s pucm`으로 관측한다.

## 실행

각 명령은 독립 실행된다. 예시의 ID는 새 실행마다 바꾼다. `--run-id` 없이 실행하면
JSON을 stdout으로만 출력한다. `--artifact-root`는 필요할 때 각 명령에 추가한다.

```bash
python -m accelerate_chess.bench.engine --iterations 10 --workers 1 --run-id engine-smoke
python -m accelerate_chess.bench.inference --model resnet-s --profiles small normal monster --batch-sizes 1 2 4 8 16 32 64 128 256 --iterations 10 --run-id inference-s
python -m accelerate_chess.bench.inference --model resnet-m --profiles normal --batch-sizes 1 8 32 --dtype bf16 --run-id inference-m
python -m accelerate_chess.bench.pipeline --workers 2 --concurrent-games 4 --mcts-simulations 32 --max-inference-batch 8 --batch-wait-us 500 --device cuda --run-id pipeline-capability
python -m accelerate_chess.bench.training --device cuda --steps 10 --batch-size 2 --run-id training-smoke
```

## Production typed architecture 학습 벤치

`typed_training`은 기존 Fixed8 전용 `training`과 별개인 synthetic optimizer throughput
벤치다. 두 family는 같은 typed observation, record, relation, candidate, condition
배열을 `TypedBatch.as_family_inputs()`로 투영한다. 현재 production typed 기본 구성인
128 채널/hidden, mask-resnet 8 block, entity-transformer 4 block·4 heads·512 FFN,
LoRA rank 8을 쓴다. vocabulary 크기는 고정 v7 catalog에서 읽는다.

```bash
python -m accelerate_chess.bench.typed_training \
  --model-family mask-resnet --profile normal --device cuda --dtype bf16 \
  --batch-size 64 --steps 100 --warmup 5 \
  --torch-threads 1 --torch-interop-threads 1 \
  --artifact-root ~/.cache/accelerate/bench-results --run-id typed-mask-bf16-b64

python -m accelerate_chess.bench.typed_training \
  --model-family entity-transformer --profile normal --device cuda --dtype bf16 \
  --batch-size 64 --steps 100 --warmup 5 \
  --torch-threads 1 --torch-interop-threads 1 \
  --artifact-root ~/.cache/accelerate/bench-results --run-id typed-entity-bf16-b64
```

프로필의 `(records, relations, candidates, candidate nodes, board height, board width)`는
small `(8, 8, 32, 3, 8, 8)`, normal `(16, 24, 80, 8, 8, 8)`, monster
`(32, 64, 160, 160, 8, 8)`이다. 실제 게임 분포가 아니라 성능 부하용 synthetic
shape다. 큰 batch와 monster의 조합은 production working-set 제한에 따라 거부될
수 있다. 입력은 한 번 준비·검증하며 `data_load_ms=0`은 timed loop에서 로딩이
없다는 뜻이다. warm-up은 optimizer step까지 실행하지만 성능 측정에서 제외한다.
`typed-training.json`은 model parameter 수와 byte, workload shape, thread 값,
timed latency/throughput, peak VRAM을 기록한다.

이 측정은 실제 replay loading이나 self-play 속도, architecture strength 또는
playing strength를 측정하지 않는다. 두 모델의 parameter count가 다르면 samples/s만으로
우열을 판단할 수 없다. 실제 model quality는 향후 같은 self-play, training,
arena 예산에서 별도로 비교해야 한다.

`engine`은 Rust 어댑터의 공개 행동 경계에서 fork, legal, bind, apply,
observe, encode와 조합 transition을 측정한다. `--workers`는 기계의 논리 CPU
수를 넘을 수 없다. 후보값 1, 2, 4, 6, 8, 10, 12, 14, 16, 20 중 실제
논리 CPU 이하만 선택한다. 각 worker는 독립 초기 위치를 가진다. native 코드는
release wheel로 빌드해야 성능 수치가 유효하다. native의 `BUILD_PROFILE`도
`release`가 아니면 벤치마크를 거부한다. chance 전이는
현재 독립 공개 API가 없어 `unsupported`다.

`inference`는 실제 `Fixed8x8ResNet`의 128×8, 192×12 설정을 사용한다.
합성 `ModelBatch`는 모델의 검증된 텐서 형식과 입력 검사를 통과하지만 실제 대국
분포를 대표하지 않는다. small/normal/monster는 각각 32×3, 80×8, 160×160
후보×노드 padding이다. 전부 synthetic으로 표시한다. 128·256 batch는 현재
모델의 64 batch 상한 때문에 `unsupported`, VRAM 부족은 `oom`으로 기록한다.
동기화된 모델 forward 시간을 재며 인코딩·host→device 전송은 포함하지 않는다.

`pipeline`은 현 구현의 교차 게임 추론 대기열 부재를 `unsupported`로 보고하는
설정 진입점이다. 현재 `InformationSetSearch`는 한 결정 안에서 leaf batch를
처리한다. 여러 게임 간 요청 병합, dispatch, queue wait, 완전한 대국 완료율은
아직 계측하지 않는다. `training`은 합성 공개 계약 입력으로 짧은 forward,
loss, backward, AdamW, checkpoint를 측정한다. replay loading은 이 수치에
포함되지 않는다. 기존 legacy/typed 모델의 terminal replay 전체 경로는
`python -m accelerate_chess.cli train --help`의 `ReplayDataset`·`DatasetCursor`·
checkpoint 계약으로 실행한다. 현재 `Fixed8x8ResNet`은 이 CLI 학습 경로에
연결되지 않았으므로 이 모델의 replay loading 수치는 `unsupported`다.
미완료 대국은 학습 target이 아니다.

## 가중치 강도 평가 준비

기존 `python -m accelerate_chess.cli evaluate --help`는 legacy/typed replay의
policy CE와 terminal value MSE를 기록하지만, `Fixed8x8ResNet` 연결과 두
checkpoint 간 대전 승률은 제공하지 않는다.
향후 baseline/trained checkpoint를 같은 rules/catalog, MCTS simulations,
추론 dtype, 시간/탐색 예산, 평가 game set, 재현 seed에서 색상을 교대하여 비교한다.
승/패/무, decisive games, win rate, Wilson 95% interval, policy loss,
value error를 기록한다. 현재 수치나 Elo·승격 판정은 없다.

## 수치 해석

`engine ops/s`는 단일 어댑터 연산, `MCTS simulations/s`는 탐색 반복,
`NN positions/s`는 모델 입력 위치, `decisions/s`는 선택 완료,
`games/hour`는 완전한 대국, `training samples/s`는 최적화 샘플 처리량이다.
서로 변환 가능한 지표가 아니다. `unsupported`, `oom`, 실패한 실행을 0이나
성공한 성능 숫자로 해석하지 않는다. H100에서의 속도는 이 로컬 결과로 추정하지 않는다.
