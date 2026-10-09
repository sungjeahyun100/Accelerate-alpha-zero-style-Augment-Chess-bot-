# Headless 규칙 동등성 계약 초안

이 문서는 PR #43의 `5bc0182` 뒤에서 수행한 UI 실행 경로 제거와 차분 도구의 범위를 기록한다. JS 오라클은 수정하지 않았다. 이 변경에서 빌드·테스트·벤치마크·E2E·성능 측정은 실행하지 않았다. 따라서 아래의 분류는 정적 호출·필드 접근 근거이며 규칙 동등성 통과 결과가 아니다.

## 비교 기준

같은 규칙 상태와 합법 행동에서 보드·기물·카드·턴·종료·관측자별 공개 정보·규칙 이력을 비교한다. 무작위 전이는 단일 결과나 RNG seed/cursor/호출 순서를 맞추지 않고 **다음 규칙 상태의 조건부 결합분포**를 비교한다. 공유 숨은 변수와 다음 턴의 의존성은 결과 키에 함께 담아야 한다. 주변분포만 비교한 자료는 `UNSUPPORTED`다.

기존 `v7-native-differential.cjs`와 `v7-native-probe.py`는 source Position ID, RNG, 전체 Replay/표현 상태와 ordered action envelope를 요구하는 **전체 실행 비교**다. 새 `rule_projection.py` 및 `semantic_differential.py`는 별도의 **규칙 의미 비교 초안**이다. 현재 production Position ID는 전체 envelope 해시이므로 규칙 상태 식별자로 재사용하지 않는다. 새 MCTS cache key를 도입하거나 기존 학습 데이터의 식별자를 변경하지 않았다.

## 필드·호출 의존성 조사

| 범주 | 경로와 호출·필드 | 읽기·쓰기 및 처리 |
|---|---|---|
| A 표현 | `v7_threat::play_move_sound_v7`, `play_crown_capture_sound_v7` | 두 함수와 실제 실행 호출을 제거했다. v7의 새 `lastMove`는 `soundName`을 쓰지 않는다. 과거 상태의 값도 덮어쓰지 않는다. v6 사운드 경로는 별도 범위다. |
| A 표현 | `replay::queue_visual`, `pendingReplayVisuals`, `forceAnimatedPieceIds`, `animatedPieceIds` | 시각 이벤트·애니메이션 Set을 생성한다. `replay::record_with_effects_profiled`가 큐를 읽어 이벤트 생성 여부와 번호를 결정한다. 미정산 큐는 새 투영에서 `UNSUPPORTED`다. |
| B 규칙 | `movement.rs`, `variant_movement.rs`, `observation.rs`의 `highlightCells`·`displayCells` | 목적지·포획 셀·안개 속 합법 행동 투영에 읽힌다. 표현용으로 일괄 삭제할 수 없다. 후속 모델에서 `targetCells` 같은 규칙 개념으로 옮겨야 한다. |
| B 규칙 | `card_effects::set_last_move`, `v7_end_move_reactions::consume_idol_encore`의 `lastMove.soundColor` | 이름과 달리 Idol Encore의 행동자 소유권 판정에 읽힌다. 새 투영에서도 보존한다. |
| B 규칙 | 카드 선택의 pending 상태, `moveReplay`, `boardHistory` 및 이전 수 | 카드 대상 순서와 실제 되돌리기·미래 합법성에 영향을 준다. 투영에서 보존한다. |
| C 혼합 | `replay.rs`의 `begin_move`·`commit_move`·`record`·`settle` | 전체 상태 사본, delta, 기보, 시각 이벤트 및 되돌리기 정보가 한 경로에 있다. Replay 카드가 과거를 복원하므로 frame/event 삭제 전 `RuleHistory`와 undo 정보의 독립 계약이 필요하다. |
| C 혼합 | `v7_host::commit_working`의 전체 상태 직렬화·재수입, `v7_adapter_actions`·`game_adapter.rs`의 Position ID | 현재 source DTO round-trip와 stale action 계약을 검증한다. 규칙 투영 해시로 즉시 바꾸면 ABI·action binding·학습 cache에 영향이 있다. |
| C 혼합 | `v7_threat::reconcile_move_replay_capture_v7` | 옛 사운드 콜백이 수행하던 왕실 포획 probe에서 **활성 Replay capture journal만** 현재 상태로 복사한다. probe의 RNG 진행과 `checkDanger` cue 쓰기는 제거했다. `replay::commit_active_move`가 해당 journal을 읽어 `moveReplay` 복원 프레임을 만들 수 있어 이를 단순 삭제할 수 없다. |
| D 미확정 | 위 probe의 Replay capture 및 RNG 효과 | `replay_capture_probe_can_change_the_next_committed_undo_frame` 재현 사례: 백 왕 (7,4), 흑 퀸 (6,4)에서 probe가 흑 capture를 남긴다. 다음 흑 commit 뒤 `moveReplay.black`은 probe 실행 여부에 따라 달라진다. 이후 흑의 Replay 카드 실행에서 `v7_card_turn::replay_available`는 이 frame의 `delta`와 현재 보드를 읽어 적용 가능 여부를 결정하고, `replay_move`는 frame을 읽어 복원한다. 이 미래 행동·복원 상태의 JS/Rust 비교는 아직 수행하지 않았다. probe가 소비하던 난수 중 이후 확률 전이의 **규칙 분포**를 바꾸는 것이 있는지 역시 조건부 결합분포 비교 전에는 확정할 수 없다. RNG cursor 일치는 요구하지 않는다. |

`replay.rs`의 frame serializing/normalizing, `transition.rs`의 move capture 경계, `v7_action_surface.rs`의 폐기 clone, `v7_action_admission.rs`·`v7_adapter_actions.rs`의 source envelope, `card_effects.rs`의 animation, `projects/accelerate/native/src/game_adapter.rs`의 비공개 Position 경계를 조사했다. Replay 구조 전체의 분리는 수행하지 않았다.

## 새 투영과 판정

`rule_projection.py`는 Position envelope에서 RNG와 원본 Position ID를 제외한다. 정산된 `pendingReplayVisuals`·`pendingNotation`·`pendingNotations` 및 애니메이션 Set을 제외한다. `lastMove`, `boardHistory[*].lastMove`, `moveReplay.*.lastMoveAfter/previousLastMove`, Replay delta의 `lastMove` 전후 값과 Replay 시각 이벤트의 이동 기록에서 `soundName`만 정규화한다. 동결 JS의 효과음 재생·기보 표기 외에 `replayMoveCard`가 현재 `lastMove`와 저장된 `lastMoveAfter`를 통째로 비교하여 복원 분기를 선택한다. 그래서 정규화 전 이 일치 여부를 투영 불변식으로 남긴다. `soundColor`, 나머지 `boardHistory`·Replay frames/events·공개 history와 **모든 미분류 필드**는 보존한다. 기물 ID 대응 관계를 검증하는 일반 알고리즘이 아직 없으므로 서로 다른 ID를 가진 동등 상태는 현재 `MISMATCH`가 될 수 있다.

`semantic_differential.py`는 paired JSONL의 시작 규칙 상태, ID·Position ID를 뺀 payload 기준 전체 합법 행동 집합과 동일 선택 행동, 두 관측자의 공개 상태, 잘못된 행동 거절·불변성을 검사한다. `transitionKind: deterministic`일 때 행동 후 규칙 상태·두 관측·결과를 정확히 비교한다. `stochastic`일 때 독립 단일 추첨끼리는 비교하지 않고, 같은 시작 투영 상태·행동에 조건화된 **다음 규칙 상태 + 두 관측 + 결과의 결합 결과 키**에 대한 분포를 비교한다. 실제로 나온 양쪽 결합 결과가 분포 지지집합에 포함돼야 한다. `exact`는 `complete: true`와 합계 1의 유리수 확률 지도가 필요하다. `sample`은 사전 허용오차 `tolerance`, 유의수준 `alpha`, 결과별 독립 표본 수가 필요하다. 표본 판정은 결과별 빈도 차이의 union-bound 범위이며 완전 동등성 증명은 아니다.

`PASS`는 제출된 모든 증거가 비교 계약을 만족함, `MISMATCH`는 관측 불일치, `UNSUPPORTED`는 입력·기능·투영의 미지원, `INCONCLUSIVE`는 전이 분류 또는 표본 수로 판정 불가를 뜻한다. 시간 초과와 공급되지 않은 결과 공간은 `PASS`가 아니다. 새 source export는 잘못된 행동자 거절의 실제 `adapter.apply` 결과를 저장한다. 과거 export에는 이 증거가 없으므로 거절 검사는 `UNSUPPORTED`다. paired 행은 원본 SHA·실행 프로필·source export 및 원본 행 digest를 기록하고 검증기는 원본 보고서와 사례 파일을 받아 출처를 대조한다. 자동 정적 triage는 이동·카드·draft의 아직 닫히지 않은 규칙 효과 경로를 기록하고 해당 행을 `unknown`으로 둔다. 안전하게 증명된 결정적 범위는 아직 0개이며, Replay 연속 경로의 단일 실현도 `INCONCLUSIVE`다.

## 사용자가 실행할 수 있는 명령

아래 명령은 **제공만 하며 이 작업에서 실행하지 않았다**. Python 3.10 이상과 Rust toolchain이 필요하다. 생성물은 `%APPDATA%/Accelerate/reports` 아래에 두고 WSL에서는 호스트 APPDATA를 확인해 `wslpath`로 변환한다. `<...>`는 해당 실행의 절대 경로다. 보고서에 전체 비공개 상태·비밀을 복사하지 않는다.

```text
node projects/augment-chess/tests/differential/v7-native-differential.cjs --oracle-only --export-cases
cargo run -p augment-chess-engine --bin augment-chess-semantic-pairs -- <절대-source-report.json> <절대-source-cases.jsonl> <절대-paired.jsonl>
python3 projects/augment-chess/tests/differential/semantic_differential.py --pairs <절대-paired.jsonl> --source-report <절대-source-report.json> --source-cases <절대-source-cases.jsonl> --report <절대-semantic-report.json>
python3 -m unittest discover -s projects/augment-chess/tests/differential -p test_rule_projection.py
cargo test -p augment-chess-engine --lib replay_capture_probe_can_change_the_next_committed_undo_frame
cargo test -p augment-chess-engine --lib replay_capture_probe_does_not_change_the_recorded_capture_cue
```

첫 명령의 `--oracle-only` 종료 코드는 의도적으로 실패이며, `report.json`의 `sourceExport` SHA·건수와 `source-cases.jsonl`을 확인한 뒤 둘째·셋째 명령에 **같은 실행**의 파일을 준다. 과거 export는 새 거절·Replay 연속 증거가 없으므로 다시 생성해야 한다. 둘째 명령은 공개 Rust host가 거부한 사례를 `.partial`에 기록하고 실패한다. 성공해도 아직 모든 자동 전이 분류가 `unknown`이어서 셋째 명령은 `INCONCLUSIVE` 또는 미지원 증거가 있으면 `UNSUPPORTED`로 종료한다. source 연속 도구는 이동 뒤 자연 합법 행동으로 상대 턴을 넘기고, Replay 카드가 실제 합법 목록에 있을 때만 적용한 뒤 다시 한 행동을 실행한다. 불가능한 경로는 `UNSUPPORTED`이며 자연 도달 Replay·포획·왕 위협 journal 사례의 실제 확보는 미실행이다. 확률적 행은 독립 단일 결과를 비교하지 않고 동일 조건의 완전 분포 또는 표본 분포를 별도로 수집해야 한다. 생성한 JSONL은 원시 비공개 상태를 포함하므로 Git에 넣지 않는다.

## 왕 위협 Replay capture 계산 의존성

`reconcile_move_replay_capture_v7`은 재생 가능한 `GameState` 전체를 복제하고 `ReplayCaptureScope`를 자식 clone에 붙인다. 양쪽 방어자에 대한 왕 위협 후보·가상 행동을 검사하되 첫 확인된 check에서는 중단한다. 실제 반환값으로 쓰는 것은 왕 위협 보고서가 아니라 자식의 마지막 활성 `MoveReplayCapture`이며, 이 값이 다음 `commit_active_move`에서 행동자와 일치하면 `moveReplay` frame으로 기록된다. 기존 내부 재현은 probe 실행 여부가 이 frame을 바꿀 수 있음을 보여준다. 따라서 Replay 복원·이후 행동의 JS↔Rust 검증 전에는 probe 전체를 제거하거나 단순 체크 계산으로 치환할 근거가 없다. 후보별 clone·완전한 위협 목록 중 journal에 영향을 주지 않는 부분은 향후 최적화 후보지만, 본 단계에서 Fast Path를 적용하거나 성능 향상률을 추정하지 않았다.

| UI 제거 단위 | 규칙 영향 | 직접 확인 절차 |
|---|---|---|
| `play_move_sound_v7` 일반 이동·카드·회피 경로 | `soundName` cue와 probe RNG 복사를 제거하고 Replay capture journal 복사만 보존한다. 합법 행동과 전이의 조건부 분포는 미검증이다. | 위 paired 생성·규칙 비교, Replay capture 사례 검사, 해당 행동의 조건부 분포 수집 |
| `play_crown_capture_sound_v7` 왕관 포획 경로 | 별도 사운드 함수를 없애고 같은 Replay capture 규칙 경계를 사용한다. 포획 뒤 복원 frame 영향이 미확정이다. | 왕관 포획의 시작/다음 상태·Replay 카드 복원 paired 자료와 조건부 분포 비교 |
| v7 `lastMove.soundName` 기록 | 신규 기록에서 cue 필드를 제거한다. `soundColor`는 Idol Encore 규칙을 위해 보존한다. | 양측 공개 관측·Idol Encore 행동 전후, 마지막 이동 투영 비교 |

## 남은 구현 위험

- `RuleHistory`/`UndoState`가 아직 분리되지 않아 Replay frame과 표현 event가 규칙 코어에 남는다.
- 분리된 Replay capture probe가 미래 복원 frame에 영향을 줄 수 있다는 정적·재현 사례는 있으나, JS와 Rust의 실제 복원 상태 및 이후 합법 행동을 검증하지 않았다. probe RNG 소비 제거가 조건부 확률 전이에 미치는 영향, 복수 무작위 효과의 공통 숨은 변수, Trolley 등 큰 결과 공간의 결합분포도 검증되지 않았다.
- 관측자별 숨은 정보 투영, 다른 기물 ID의 참조 대응, 연속 대국의 미래 의존성, 모든 카드·특수 기물·종료 분기의 확률 열거가 미완료다.
- 기존 전체 상태 diff는 RNG·UI 표현까지 비교하므로 이번 UI 제거의 규칙 동등성 gate가 아니다. 이 변경은 빌드/검사 미실행이며 Headless Rules Engine 전환의 완료나 규칙 동등성 확인을 뜻하지 않는다.
