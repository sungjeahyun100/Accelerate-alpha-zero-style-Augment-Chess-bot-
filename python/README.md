# Python AI

Python 코드는 `python/accelerate_chess` 패키지에 모읍니다. 인코딩·탐색·ResNet·FiLM·LoRA·
자가대국·학습·평가의 책임은 이 패키지에 두고 게임 규칙은 독립 `rust-engine`을 직접 호출합니다.
`bridge/native`의 얇은 PyO3 연동은 immutable `Position`·`Action`·`StepResult`를 제공하고,
maturin은 두 언어를 하나의 설치 가능한 wheel로 묶습니다. 과거의 빈 `alphazero`, `mcts`,
`network`, `selfplay`, `training`, `nnue` 폴더를 구현의 복제 위치로 유지하지 않습니다.

현재 지원 환경은 Python 3.12, Rust 1.96 이상, Windows/Linux x86_64 CPU입니다.
Python 의존성은 루트 `pyproject.toml`과 `uv.lock`, Rust 의존성은 루트 Cargo workspace와
`Cargo.lock`에 고정합니다. PyTorch는 CPU index를 명시적으로 사용합니다.
`validation` extra의 Python ONNX Runtime은 비교 검사용이며 운영 추론 backend의 자동
fallback으로 사용하지 않습니다. 운영의 `ort`/`tract` 선택은 명시적인 native API 계약입니다.

Windows 개발 환경과 생성물을 고정된 외부 슬롯에 준비하는 예시는 다음과 같습니다.
`uv`와 Rust가 PATH에 있어야 합니다.

```powershell
$accelerateRoot = Join-Path $env:APPDATA 'Accelerate'
$env:UV_CACHE_DIR = Join-Path $accelerateRoot 'cache\uv'
$env:UV_PROJECT_ENVIRONMENT = Join-Path $accelerateRoot 'build\windows\full-stack-implementation\venv'
$env:CARGO_TARGET_DIR = Join-Path $accelerateRoot 'build\windows\full-stack-implementation\cargo'
$env:PYTHONDONTWRITEBYTECODE = '1'
uv sync --locked --all-extras --no-editable
uv run --no-sync python -m pytest python/tests/test_native.py -p no:cacheprovider
```

`--no-editable`은 native 확장 파일을 소스 폴더에 복사하지 않고 외부 환경에 설치합니다.
CI는 동일 슬롯의 루트를 `$RUNNER_TEMP/Accelerate`로 지정합니다. WSL은 Linux 전용
가상환경·Cargo 빌드·uv 캐시를 `${XDG_CACHE_HOME:-$HOME/.cache}/accelerate` 아래에 두며
Windows 바이너리·환경을 공유하지 않습니다. 보존할 wheel과 보고서는 호스트 APPDATA를
확인해 Windows 생성물 루트로 내보냅니다. 자세한 경로 기준은 [AGENTS](../AGENTS.md)를 따릅니다.

직접 호출은 다음과 같습니다.

```python
from accelerate_chess import Position

position = Position.new_game({"draftDelete": True}, seed=17)
observation = position.observe(position.decision_actor)
action = position.legal_actions()[0]
step = position.apply(action)
restored = Position.from_json(step.position.to_json())
assert restored.snapshot() == step.position.snapshot()
```

이 예시는 환경 실행 API입니다. 숨겨진 실제 상태로 탐색 후보를 만들면 안 되며 탐색은
관측과 공개 history로 구성한 belief particle에서 `legal_actions()`를 호출합니다.
운영 후보의 신경망 입력에는 particle의 `Action.public_intent()`를 사용하고 실제 위치에는
선택한 intent를 `bind_public_intent()`로 해석해 적용합니다. `as_payload()`와 `bind_action()`은
실행·replay를 위한 lossless 환경 API입니다. 위 예시의 `draftDelete: True`는 초기 draft를
비활성화하는 명시적 설정입니다. 기본 normal/chaos의 weighted 초기화와 초기 공개 관측
역조건화는 별도 native 통합 검사로 검증하며, 카드 획득·효과의 전체 지원 여부와 구분합니다.
설치 패키지는 `site_catalog()`로 컴파일된 동결 카탈로그를 얻습니다.
`position_id`는 저장·stale 검사용 제어 정보이고 신경망 입력이 아닙니다.

`encoding.py`와 `network/`는 공개 관측·후보 행동 인코딩, 잔차 신경망, 그래프 내부 FiLM,
별도 LoRA 어댑터와 ONNX export를 담당합니다. FiLM 조건은 명시적인 ONNX 입력으로
유지하고 정적 LoRA 병합은 원본을 복사해 수행합니다. 모델 승률이나 실제 학습 여부는
코드·형식·수치 검증과 별도로 보고합니다. 이번 구현 검증에서 실제 학습·성능 캠페인은
실행하지 않습니다.

공개 정보 탐색과 유한 실행 CLI는 설치한 패키지에서 모듈로 실행합니다.

```text
python -m accelerate_chess.cli --help
python -m accelerate_chess.cli choose --help
python -m accelerate_chess.cli selfplay --help
```

| 명령 | 코드의 역할 |
|---|---|
| `init` | 독립 seed로 base checkpoint와 ONNX artifact를 초기화 |
| `choose` | 공개 trace에서 particle belief를 재구성하고 public intent 선택 |
| `selfplay` | game·ply·시간 한도가 있는 실행과 양측 공개 trace/replay 보존 |
| `train` | terminal replay dataset, base/adapter optimizer와 재개 checkpoint 연결 |
| `export` | base와 선택한 static LoRA 복사 병합 모델을 ONNX로 export |
| `evaluate` | replay의 제한된 sample에 명시한 실제 backend를 실행하고 수치 기록 |
| `activate` | 지정한 artifact hash를 검사한 뒤 active 참조를 명시적으로 기록 |

기본 모델은 8 block·128 channel·LoRA rank 8이고 봇 encoding은 public intent와 공개 이력
요약을 사용합니다. 기본 추론 backend는 `ort`, `tract`는 명시적으로 선택합니다.
`--threads`는 Torch와 native ort에 적용되며 tract는 1만 허용합니다. artifact 활성화는
검사한 backend를 provenance로 기록하며 다음 명령의 backend 선택을 자동 변경하지 않습니다.
Windows 출력은 `%APPDATA%\Accelerate`, 다른 호스트는 명시한 `--artifact-root`에 모읍니다.

현재 설치 wheel 검사는 normal/chaos 초기 공개 조건화와 첫 draft 선택을 확인했습니다.
실제 CLI 검사에서는 `draftDelete: true`를 명시하고 4 leaf batch 탐색, 1 ply 미완료 replay와
tract 평가를 실행했습니다. 전체 default mode의 draft 이후 플레이와 카드 효과는 포팅 중입니다.
미완료 episode에는 승패 target을 만들지 않습니다. SIGINT는 exit 130, 전체 selfplay/train
deadline은 exit 2로 전달하고 중단 당시 pending decision과 공개 이력을 보존합니다.
이번 구현 검증에서 `train`을 통한 실제 학습 캠페인은 실행하지 않습니다.

[native API](../bridge/native/README.md)와 [전체 설계](../docs/ARCHITECTURE.md)를 함께
참고합니다. FFI 검사가 통과해도 사이트의 모든 규칙이나 전체 프로젝트가 완성된 것은
아닙니다. 전체 코드 완료 여부는 각 구현 영역과 통합 검증의 관측 결과로 판정합니다.
