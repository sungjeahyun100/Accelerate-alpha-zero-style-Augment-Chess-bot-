# Rust Replay 제거 A/B/C 실험

이 실험은 Rust v7 엔진 내부에서만 실행한다. 동결 JS 오라클, JS↔Rust 차분 검사,
Python 탐색, 신경망, 학습은 호출하지 않는다. 구현 기준 커밋은 `a574446603f0c019bdffe10322f95c40e1dee183`이며
실험 브랜치는 `feature/rust-replay-ab`이다. 기존 PR #43은 변경하지 않는다.

## 모드와 상태 경계

`ReplayMode`는 `GameState`에 세션별로 보관되고 소스 JSON에는 직렬화되지 않는다.
`Position::with_replay_mode` 또는 `V7HostPosition::with_replay_mode`로 같은 초기 위치에서
독립 세션을 만든다. 일반 생성과 import는 `FullReplay`가 기본이다. 상태 clone과
host transaction은 모드를 유지한다. 이전 위치에 이력이 있으면 모드 설정 시
`replayEvents`, `boardHistory`, `notationTimeline`, base/tail frame을 먼저 비운다.
비운 뒤 clone하므로 이미 쌓인 이력의 반복 깊은 복사를 피한다.

| 모드 | 유지 | 생략 |
|---|---|---|
| FullReplay | 기존 기록, 프레임, moveReplay, capture 및 delta | 없음 |
| NoHistoryReplay | moveReplay, 이동 취소 delta, 기록 시 금속 동기화·chain bond 정리·기보 이벤트/난수 | 과거 replay event/board frame/timeline 저장·프레임 delta |
| NoReplay | 기록 시 실제 규칙 부수 효과 | B의 제거 항목과 begin snapshot, active capture, moveReplay delta, 이전 보드가 필요한 수 기보 생성 |

`record` 전체를 건너뛰지 않는다. `sync_active_metal`은 분기 전 실행한다.
체인 본드 정리와 legacy timeline 검사는 간결 기록 경로에서 실행한다.
기보가 생성된 경로의 `notationEvent(s)` 갱신과 pending 정리도 유지한다.
위협 probe의 event nonce와 replay 횟수는 별도 숫자로 유지한다.
`boardHistory`의 소스 12개 상한도 숫자에 적용한다. `moveReplay`는 B에서 유지하며
C에서는 초기화 후 생성하지 않는다. C에서 이동 기보의 이전 보드 참조가 필요한
경로는 생략되므로 해당 기보의 RNG draw 역시 생략된다.

## 동등성 한계와 예상 차이

B도 아직 규칙 동등성이 증명된 모드가 아니다. 정식 기록은 이전 frame과 새 frame의
delta 및 시각 효과를 비교해 event 생성 여부를 결정한다. B/C는 frame 자체를 만들지
않고 기록 호출마다 event count와 nonce를 증가시킨다. 변경 없는 record 호출에서는
FullReplay와 nonce가 달라질 수 있다. nonce를 검사하는 빠른 경로나 ID, 향후 규칙
분기에 영향이 생길 수 있다. 정식 record의 notation timeline 중복 제거는
간결 기록에 포함되지 않는다. 이것이 현재 보드,
합법 행동, RNG 또는 종료 판정에 영향을 주는지는 사용자 실행 결과로 확인해야 한다.

C는 규칙 동등성 모드가 아니다. `moveReplay`를 읽는 카드와 ReplayCaptureScope에
의존하는 임시 위협 probe, 이동 기보에 따라 다른 행동·RNG가 나올 수 있다.
이 차이를 보정하지 않는다. v7 host의 공개 `history`와 공개 이벤트 생성은 host
계약상 계속 실행된다. 따라서 측정치는 Rust 엔진의 내부 과거 replay 자료 제거
효과이며, 공개 host 이벤트 제거의 성능 상한은 아니다. C에서도 위협 probe scope
객체 자체의 생성 비용은 남을 수 있다.

위치 ID와 action ID는 이력 필드 때문에 모드마다 달라질 수 있다. 결과의
`comparisons`는 ID를 동등성 지표로 쓰지 않고, replay 전용 필드를 제외한 상태의
서로 다른 경로, RNG, 공개 합법 행동, 종료 결과, MCTS 선택 행동을 각각 기록한다.
서로 다른 경로 목록은 최대 100개다. `ruleState` 원본도 JSON에 남으므로 사용자가
전체 차이를 확인할 수 있다.

## 사용자 실행

아래 명령은 **사용자가 직접 실행**한다. Codex는 이번 작업에서 빌드·테스트·
벤치마크·MCTS·학습을 실행하지 않았다. `--seed`는 공통 초기 v7 게임에 한 번만
적용한다. `draftDelete=true`의 기본 설정으로 첫 play 위치를 만든 뒤 세 모드로
복제한다. `--actions`는 공개 intent 객체의 JSON 배열이며, 모든 모드에 같은 순서로
적용한다. 생략 시 기준선의 첫 합법 공개 행동 하나를 사용한다.

```bash
cargo run --release -p augment-chess-engine --bin rust-replay-ab -- \
  --seed 19 --iterations 20 --simulations 32 --rollout-depth 2 \
  --output /tmp/rust-replay-ab.json
```

```bash
cargo run --release -p augment-chess-engine --bin rust-replay-ab -- \
  --seed 19 --actions /tmp/public-intents.json \
  --iterations 20 --simulations 32 --rollout-depth 2 \
  --output /tmp/rust-replay-ab.json
```

사용자 검사 명령:

```bash
cargo test -p augment-chess-engine replay_experiment
```

## 측정 정의

각 모드에서 같은 root 위치를 반복 사용한다. `apply`는 같은 공개 intent의 bind 및
적용 전체 시간, `legalEnumeration`은 전체 공개 합법 intent 열거,
`gameStateDeepClone`은 `GameState::clone` 시간이다. `meanNs`, `medianNs`, `p95Ns`는
각 반복의 나노초이고 `applyActionsPerSecond`는 적용 수/적용 시간 합계다.
`replayPhaseProbe`는 시간 루프 밖에서 수행하며, 적용 전후 상태의 `commit_move`
delta 생성과 이미 정산된 적용 후 상태에 대한 추가 `record` 호출을 각각 측정한다.
이 record는 변경 없는 이벤트를 억제할 수 있어 실제 행동 중 record 비용과 같은
값으로 해석하면 안 된다.

`mcts`는 Rust 규칙 엔진만 쓰는 결정적 root UCB1 탐색이다. 신경망 추론은 꺼져 있고,
고정 simulation 수와 rollout 깊이, 같은 초기 위치를 사용한다. leaf 평가는 고정
기물 가치의 물질 점수이며 정식 학습 모델의 MCTS 성능이 아니다. 선택한 공개 intent,
완료 simulation 수, 전체 소요 시간과 초당 simulation 수를 기록한다.
상세 상태 비교는 시간 측정 후 별도로 실행한다. `/proc/self/status`의 `VmHWM`은
프로세스 시작 후 최대 RSS로, 순차 모드별 독립 peak가 아니라 누적 high-water다.
Linux 이외에는 `null`이다. seed, 반복 수, simulation 수, 깊이, 추론 설정을 JSON에
기록한다. CPU, Rust 버전, 빌드 프로필, 환경 부하와 입력 행동 파일은 사용자가
결과와 함께 별도 보존해야 한다.
모드 실행 오류가 있으면 해당 모드의 `error`와 비교 불가 이유를 JSON에 남긴 뒤
프로세스가 비영(非零) exit code로 종료한다.

모드별 독립 최대 RSS가 필요하면 동일 옵션으로 프로세스를 세 번 실행한다.
`--mode full_replay`, `--mode no_history_replay`, `--mode no_replay`가 각각 한 모드만
실행한다. 이 경우 `comparisons`는 비어 있으므로 세 JSON의 `ruleState`, `rng`,
`legalIntents`, `result`, `mcts.selectedIntent`를 비교한다.

## 변경 파일 및 미검증 범위

- 엔진 모드/기록: `projects/augment-chess/engine/src/replay_experiment.rs`, `replay.rs`, `state.rs`
- 상태·호스트 전달: `projects/augment-chess/engine/src/lib.rs`, `v7_host.rs`
- 적용 및 횟수 소비 경로: `transition.rs`, `movement.rs`, `v7_move_execution.rs`, `v7_move_transition.rs`, `v7_queued_effects.rs`
- 실행 도구: `projects/augment-chess/engine/src/bin/rust-replay-ab.rs`, 엔진 `Cargo.toml`

빌드·테스트·실험은 사용자 실행 정책에 따라 미실행이다. 위의 동등성 한계와
컴파일/실행 결과는 아직 검증되지 않았다. 이 문서는 기준선 성능 수치나 향상률을
제시하지 않는다.
