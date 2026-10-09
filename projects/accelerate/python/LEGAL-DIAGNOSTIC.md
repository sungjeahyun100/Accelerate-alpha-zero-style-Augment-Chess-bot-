# 합법 행동 전용 진단

이 명령은 검증된 `position-provenance.json`을 두 번 재생하여 eager 조회와 페이지
조회를 각각 독립된 원본 상태에서 측정합니다. 기본 실행에는 Rust 계측이 켜지지 않습니다.
공개 어댑터 요청·응답 스키마와 합법성 판정은 바뀌지 않습니다. 신경망·MCTS는 호출하지
않습니다.

Windows PowerShell에서 Native Release wheel을 외부 가상환경에 설치한 뒤 실행합니다.
`$provenance`와 `$report`는 실제 사용자 생성물 위치로 바꾸세요.

```powershell
cd projects/accelerate
$accelerateRoot = Join-Path $env:APPDATA 'Accelerate'
$env:UV_CACHE_DIR = Join-Path $accelerateRoot 'cache\uv'
$env:UV_PROJECT_ENVIRONMENT = Join-Path $accelerateRoot 'build\windows\legal-diagnostic\venv'
$env:CARGO_TARGET_DIR = Join-Path $accelerateRoot 'build\windows\legal-diagnostic\cargo'
$env:PYTHONDONTWRITEBYTECODE = '1'
uv sync --locked --all-extras --no-editable
$provenance = Join-Path $accelerateRoot 'datasets\middle-generate\position-provenance.json'
$report = Join-Path $accelerateRoot 'reports\legal-diagnostic\result.json'
uv run --no-sync python -m accelerate_chess.legal_diagnostic `
  --provenance $provenance `
  --expected-position-id 6621740ee684bcb66ff0ebaebabe072590b85352c8875f4785e93736f58bda53 `
  --mode both --page-size 64 --output $report
(Get-Content $report -Raw | ConvertFrom-Json).runs | Format-List
```

한 경로만 측정하려면 `--mode eager` 또는 `--mode stream`을 선택합니다. 진단은
provenance의 `generated_position_id`, 각 재생 단계의 원본 ID 및 최종 Source Position
ID를 검증합니다. 재생 시간 `replay_ms`는 각 경로의 합법 행동 조회 시간에서 제외합니다.
`state_size_serialization_ms`와 `serialized_state_bytes`는 계측 시작 전에 별도 산출합니다.

`timing_ms`는 구간별 포함 시간이고 `timing_detail`에는 호출 횟수·평균·최솟값·
최댓값·자체 시간이 있습니다. `cursor_page`는 하위 이동·카드 생성, 후보 검증,
공개 투영을 포함하며 `total_legal`은 Python 호출과 어댑터 전달 시간까지 포함합니다.
따라서 포함 시간을 서로 더하지 마세요. `unclassified_ms`는 기록된 최상위 Rust 구간을
제외한 전체 시간이고, `cursor_page.exclusive_ms`는 페이지 안에서 아직 세분화하지
않은 시간입니다. 이 값에는 계측 자체 비용이 포함될 수 있습니다.

`state_clone`은 커서 초기화와 후보 검증에서 명시적으로 호출한 `GameState::clone`을
측정합니다. 카드 효과는 `transition_apply` 안의 `card_effect_apply`로 분리합니다.
내부 규칙 함수의 다른 상태 복제와 카드 인스턴스 내부 할당은 아직 개별 계측하지
않습니다. `movement_targets`는 `movement_generation` 또는 전이 적용 안에서 호출될
수 있습니다. `canonicalization`은 성공한 후보의 JCS 직렬화·역직렬화와 프레임
교체를 포함합니다. 공개 intent의 JCS 중복 제거는 `public_deduplication`입니다.

`counts`에는 생성한 이동·카드 후보, 검사·승인·거절한 후보, 측정 경계의 상태
복제·전이·정규화 횟수가 기록됩니다. `slowest_candidates`는 검사 순번과 시간만
기록합니다. 비공개 카드 내용, Source Action Payload, 내부 RNG와 Hidden State는
보고서에 포함하지 않습니다. 진단 자료를 공유하기 전에 출력 경로를 확인하세요.

필요한 회귀 검사는 사용자가 직접 실행합니다.

```powershell
uv run --no-sync python -m pytest python/tests/test_adapter_client.py -p no:cacheprovider
```
