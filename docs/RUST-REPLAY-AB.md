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

## 장기 대국 측정 설계

`rust-replay-ab`는 FullReplay의 첫 play 상태에서 공개 합법 intent만 고른다.
결정적 정책은 첫 행동에서 기존 기준선처럼 첫 합법 intent를 택한다. 이후에는
합법 행동 중 보드 `move`를 우선하고 seed와 행동 index로 후보를
회전 선택한다. 보드 이동이 없을 때만 카드·선택 등 다른 합법 행동을 선택한다.
이 정책은 강한 대국 정책이 아니며 규칙 상태나 RNG를 보정하지 않는다.
`--actions`를 주면 해당 JSON 배열을 입력으로 사용한다. 제공된 행동도 FullReplay에서
순서대로 합법성과 적용을 확인한다. 생성 또는 검증된 공통 시퀀스는
`--save-actions`로 저장한다. 최대 체크포인트에서 수 적용을 재려면 그 상태의
**다음 행동**도 필요하므로 `--max-pre-actions`를 최대 체크포인트보다 1 이상 크게 둔다.

체크포인트 `N`은 첫 play 상태 이후 적용을 마친 공개 intent `N`개다. 체스의
한 수나 반수와 동의어가 아니다. 체크포인트마다 `actualActions`, 공개 `move` 수,
공개 `card` 수, 나머지 공개 행동 수를 기록한다. 엔진의 `moveCount`는 엔진이
유지하는 이동 카운터, `fullMove`는 엔진의 전체 수 카운터,
`turnsTaken`은 색별 완료 턴, `cardsUsedThisTurn`은 현재 턴의 색별 카드 사용 수다.
복합 카드 선택과 추가 행동 때문에 이 값들은 공개 intent 수와 달라질 수 있다.
`phase`, `turn`, `decisionActor`를 함께 기록한다.

각 모드는 공통 초기 위치에서 독립적으로 시작하고 같은 intent를 순서대로
소비한다. B/C에서는 매 행동 직후 FullReplay를 별도로 재실행한 상태와
직렬화된 규칙 상태, RNG, 종료 판정, 공개 합법 행동을 비교한다. 위치·행동 ID의
일치는 요구하지 않는다. `moveReplay`는 규칙 상태 비교에 포함한다.
과거 replay 배열·frame·기보 이력·공개 host history는 의미 비교에서 제외하며
이벤트 개수와 `replayEventNonce`는 replay metadata로 별도 비교한다.
`firstDivergence`는 최초 차이이고 `firstRuleDivergence`는 이후 처음 관측된
semantic 또는 rng 차이다. 각 기록은 mode, 1부터 시작하는 행동 index, intent,
필드 경로, 양쪽 값, 범주와 비교 가능 여부를 포함한다. 규칙·RNG 차이 뒤의
벤치마크는 `diagnostic_only`다. 해당 root에서 공통 다음 행동이 불법이면
수 적용 비교를 생략하고 `comparisonUnavailable`에 이유를 남긴다.

## 결과 스키마와 단위

최상위 `schemaVersion=2`에는 `seed`, `programVersion`, `iterations`,
`mctsSimulations`, `rolloutDepth`, `checkpointsRequested`, `maxPreActions`,
`sequenceSource`, `sequence`, `sequenceStopReason`,
`sequenceGenerationElapsedNs`, `results`가 있다. `results[]`는 mode별로
`actionsConsumed`, `firstDivergence`, `firstRuleDivergence`, `checkpoints[]`를
담는다. 각 체크포인트는 `reached`, `actualActions`, `phase`, `reason`,
`preparationElapsedNs`, `comparisonStatus`, `measurement`를 담는다.
미도달은 `reached=false`와 실제 진행 수·이유로 표시하며 측정값은 `null`이다.
기존 최상위 `results`와 mode별 `apply`, `legalEnumeration`, `mcts` 명칭은
체크포인트의 `measurement` 아래에서 유지한다. 분석기는 `schemaVersion`을 확인해야 한다.

`measurement.metrics`는 `replayEventCount`, `replayEventsJsonBytes`,
`gameStateJsonBytes`, `moveReplayJsonBytes`를 포함한다. 직렬화 크기는 각
값을 compact JSON으로 직렬화한 UTF-8 바이트 수다. `gameStateJsonBytes`에는
전체 `GameState`가 들어가고 Rust 실행 전용 `#[serde(skip)]` 필드는 제외된다.
B/C의 `replayEventCount`는 제거된 배열 길이가 아니라 유지한 실험 카운터다.
`maxRssKiBProcessHighWater`는 Linux `/proc/self/status`의 `VmHWM`이다.
한 프로세스 안에서는 이전 체크포인트·모드의 peak를 포함한다. 독립 peak를
비교하려면 `--mode`로 모드별 프로세스를 실행해야 하며 그 경우도 공통 시퀀스
검증·사전 대국 준비 비용을 포함한다.

`apply`, `legalEnumeration`, `gameStateDeepClone`의 `meanNs`, `medianNs`,
`p95Ns`는 각각 같은 root에서 공개 intent bind+적용, 전체 합법 공개 행동 열거,
`GameState::clone()`을 반복한 나노초다. 수 적용은 공통 시퀀스의 다음 intent를
사용한다. 끝에 다음 intent가 없으면 `apply=null`과 이유를 기록한다.
`mcts`는 실험용 결정적 Rust UCB1만 사용한다. 요청·완료 simulation, rollout
깊이, 경과 나노초, simulations/sec, 선택 intent, 실패 여부를 기록한다.
신경망 추론은 사용하지 않는다. 내부 기록 생성 횟수는 핫 패스 계측을 넣지 않아
`not_instrumented`로 명시한다. `replayPhaseProbe`는 별도 복사본에서 수행한
사후 진단이며 MCTS나 실제 적용 비용에 합산하지 않는다. root 직렬화 상태가
반복 측정 전후 같은지도 `rootStateUnchanged`에 기록한다.

`--progress`는 첫 줄에 설정·시퀀스, 이후 완료된 체크포인트마다
`{mode,checkpointResult}`를 NDJSON 한 줄로 쓰고 동기화한다. 중단되면 이미
완료된 줄을 보존한다. 정상 종료와 부분 실패 시 최종 JSON은 `--output`에 쓴다.
사전 시퀀스 생성 자체가 실패하거나 출력 파일을 쓸 수 없는 오류에는 최종 결과가
없을 수 있다. 오류는 stderr와 비영 종료 코드로 전달한다.

## 사용자 실행 (Ubuntu)

아래 명령은 사용자가 직접 실행한다. Codex는 빌드·테스트·벤치마크·MCTS를
실행하지 않았다. 출력 파일은 사용자가 관리하는 생성물 루트 아래에 둔다.

```bash
cargo run --release -p augment-chess-engine --bin rust-replay-ab -- \
  --seed 19 --checkpoints 0,10,30,50 --max-pre-actions 51 \
  --iterations 20 --simulations 16 --rollout-depth 1 \
  --save-actions "$ARTIFACT_ROOT/reports/replay-actions.json" \
  --progress "$ARTIFACT_ROOT/reports/replay-progress.ndjson" \
  --output "$ARTIFACT_ROOT/reports/replay-all.json"
```

모드별 독립 프로세스 RSS 및 공통 시퀀스 재사용:

```bash
for mode in full_replay no_history_replay no_replay; do
  cargo run --release -p augment-chess-engine --bin rust-replay-ab -- \
    --seed 19 --actions "$ARTIFACT_ROOT/reports/replay-actions.json" \
    --checkpoints 0,10,30,50 --max-pre-actions 51 \
    --iterations 20 --simulations 16 --rollout-depth 1 --mode "$mode" \
    --progress "$ARTIFACT_ROOT/reports/$mode.ndjson" \
    --output "$ARTIFACT_ROOT/reports/$mode.json"
done
```

세 독립 결과의 설정·시퀀스·체크포인트를 결합하기 전 확인하는 분석 명령:

```bash
python3 - "$ARTIFACT_ROOT/reports" <<'PY_COMPARE'
import json, pathlib, sys
root = pathlib.Path(sys.argv[1])
names = ('full_replay', 'no_history_replay', 'no_replay')
files = {name: json.loads((root / f'{name}.json').read_text()) for name in names}
base = files[names[0]]
keys = ('schemaVersion', 'seed', 'sequence', 'checkpointsRequested',
        'iterations', 'mctsSimulations', 'rolloutDepth')
for name, report in files.items():
    assert all(report[key] == base[key] for key in keys), f'{name}: settings differ'
    result = report['results'][0]
    assert result['mode'] == name
    for checkpoint in result['checkpoints']:
        measure = checkpoint['measurement']
        rate = measure.get('mcts', {}).get('simulationsPerSecond') if isinstance(measure, dict) else None
        print(name, checkpoint['checkpoint'], checkpoint['reached'],
              checkpoint['comparisonStatus'], rate,
              measure.get('maxRssKiBProcessHighWater') if isinstance(measure, dict) else None)
PY_COMPARE
```

사용자 검사 명령: `cargo test -p augment-chess-engine --bin rust-replay-ab` 및
`cargo test -p augment-chess-engine replay_experiment`. 이번 변경의 테스트·빌드·
실험 결과는 아직 없다. 특히 10/30/50 행동 도달성과 규칙 동등성, 성능 수치는
사용자 실행 전까지 미검증이다.
