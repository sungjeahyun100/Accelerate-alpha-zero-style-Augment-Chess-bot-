# Headless 규칙 동등성 계약 초안

이 문서는 PR #43의 `d7f0f41` 위에서 수행한 정적 조사와 새 차분 도구의 현재 범위를 기록한다. JS 오라클은 수정하지 않았다. 이 변경에서 빌드·테스트·벤치마크·E2E·성능 측정은 실행하지 않았다. 따라서 아래의 분류는 호출·필드 접근 근거이며 규칙 동등성 통과 결과가 아니다.

## 비교 기준

같은 규칙 상태와 합법 행동에서 보드·기물·카드·턴·종료·관측자별 공개 정보·규칙 이력을 비교한다. 무작위 전이는 단일 결과나 RNG seed/cursor/호출 순서를 맞추지 않고 **다음 규칙 상태의 조건부 결합분포**를 비교한다. 공유 숨은 변수와 다음 턴의 의존성은 결과 키에 함께 담아야 한다. 주변분포만 비교한 자료는 `UNSUPPORTED`다.

기존 `v7-native-differential.cjs`와 `v7-native-probe.py`는 source Position ID, RNG, 전체 Replay/표현 상태와 ordered action envelope를 요구하는 **전체 실행 비교**다. 새 `rule_projection.py` 및 `semantic_differential.py`는 별도의 **규칙 의미 비교 초안**이다. 현재 production Position ID는 전체 envelope 해시이므로 규칙 상태 식별자로 재사용하지 않는다. 새 MCTS cache key를 도입하거나 기존 학습 데이터의 식별자를 변경하지 않았다.

## 필드·호출 의존성 조사

| 범주 | 경로와 호출·필드 | 읽기·쓰기 및 처리 |
|---|---|---|
| A 표현 | `v7_threat::play_move_sound_v7`, `play_crown_capture_sound_v7`의 `lastMove.soundName` | cue 선택과 `checkDanger` 쓰기는 소리 표현이다. 다만 함수 전체가 위협 후보 실행, RNG, Replay capture journal을 변경하므로 이번 변경에서 호출을 지우지 않았다. 독립 규칙 probe와 분리해야 한다. |
| A 표현 | `replay::queue_visual`, `pendingReplayVisuals`, `forceAnimatedPieceIds`, `animatedPieceIds` | 시각 이벤트·애니메이션 Set을 생성한다. `replay::record_with_effects_profiled`가 큐를 읽어 이벤트 생성 여부와 번호를 결정한다. 미정산 큐는 새 투영에서 `UNSUPPORTED`다. |
| B 규칙 | `movement.rs`, `variant_movement.rs`, `observation.rs`의 `highlightCells`·`displayCells` | 목적지·포획 셀·안개 속 합법 행동 투영에 읽힌다. 표현용으로 일괄 삭제할 수 없다. 후속 모델에서 `targetCells` 같은 규칙 개념으로 옮겨야 한다. |
| B 규칙 | `card_effects::set_last_move`, `v7_end_move_reactions::consume_idol_encore`의 `lastMove.soundColor` | 이름과 달리 Idol Encore의 행동자 소유권 판정에 읽힌다. 새 투영에서도 보존한다. |
| B 규칙 | 카드 선택의 pending 상태, `moveReplay`, `boardHistory` 및 이전 수 | 카드 대상 순서와 실제 되돌리기·미래 합법성에 영향을 준다. 투영에서 보존한다. |
| C 혼합 | `replay.rs`의 `begin_move`·`commit_move`·`record`·`settle` | 전체 상태 사본, delta, 기보, 시각 이벤트 및 되돌리기 정보가 한 경로에 있다. Replay 카드가 과거를 복원하므로 frame/event 삭제 전 `RuleHistory`와 undo 정보의 독립 계약이 필요하다. |
| C 혼합 | `v7_host::commit_working`의 전체 상태 직렬화·재수입, `v7_adapter_actions`·`game_adapter.rs`의 Position ID | 현재 source DTO round-trip와 stale action 계약을 검증한다. 규칙 투영 해시로 즉시 바꾸면 ABI·action binding·학습 cache에 영향이 있다. |
| D 미확정 | `v7_threat::probe_royal_capture`가 sound 경로 안에서 변경하는 RNG·Replay scope | 위협 결과는 규칙이지만 소리만을 위해 실행한 probe의 공유 journal 명령이 이후 되돌리기에 영향을 주는지 정적 조사만으로 확정되지 않았다. 독립 비교 뒤 제거한다. |

`replay.rs`의 frame serializing/normalizing, `transition.rs`의 move sound 경계, `v7_action_surface.rs`의 폐기 clone, `v7_action_admission.rs`·`v7_adapter_actions.rs`의 source envelope, `card_effects.rs`의 animation, `projects/accelerate/native/src/game_adapter.rs`의 비공개 Position 경계를 조사했다. 이 단계에서 실제 엔진의 UI 생성 코드를 제거하거나 Replay 구조를 분리했다고 주장하지 않는다.

## 새 투영과 판정

`rule_projection.py`는 Position envelope에서 RNG와 원본 Position ID를 제외한다. 정산된 `pendingReplayVisuals`·`pendingNotation`·`pendingNotations` 및 애니메이션 Set을 제외하고 `lastMove.soundName`만 제외한다. `lastMove.soundColor`, `boardHistory`, Replay frames/events, 공개 history와 **모든 미분류 필드**는 남긴다. 기물 ID 대응 관계를 검증하는 일반 알고리즘이 아직 없으므로 서로 다른 ID를 가진 동등 상태는 현재 `MISMATCH`가 될 수 있다. 이것은 미지원 범위이며 통과로 완화하지 않는다.

`semantic_differential.py`는 별도로 생성한 paired JSONL의 source/Rust 상태, ID·Position ID를 뺀 payload 기준 전체 합법 행동 집합과 동일 선택 행동, 두 관측자의 공개 상태, 잘못된 행동 거절·불변성, 확률분포 증거를 검사한다. 확률 입력은 하나의 행동에서 함께 발생한 결과와 이후 의존 상태를 나타내는 **완전한 결합 결과 키**여야 한다. `exact`는 `complete: true`와 합계 1의 유리수 확률 지도가 필요하다. `sample`은 동일 조건부 상태·행동을 나타내는 `conditionedOn`, 사전 허용오차 `tolerance`, 유의수준 `alpha`, 결과별 독립 표본 수가 필요하다. 표본 판정은 결과별 빈도 차이의 union-bound 범위이며 완전 동등성 증명은 아니다.

`PASS`는 제출된 모든 증거가 비교 계약을 만족함, `MISMATCH`는 관측 불일치, `UNSUPPORTED`는 입력·기능·투영의 미지원, `INCONCLUSIVE`는 표본 수로 판정 불가를 뜻한다. 시간 초과와 공급되지 않은 결과 공간은 `PASS`가 아니다. 이 CLI는 oracle이나 Rust 엔진을 실행하거나 paired 자료를 생성하지 않는다. 기존 `source-cases.jsonl`만으로는 Rust의 비공개 전체 상태가 없어 새 CLI를 통과시킬 수 없다. 전체 결합 결과 자동 열거와 자연 연속 대국 입력 생성도 아직 구현되지 않았다.

## 사용자가 실행할 수 있는 명령

아래 명령은 **제공만 하며 이 작업에서 실행하지 않았다**. Python 3.10 이상, 외부에 보관한 paired JSONL이 필요하다. 생성물은 `%APPDATA%/Accelerate/reports` 아래에 두고 WSL에서는 호스트 APPDATA를 확인해 변환한다. 보고서에 전체 비공개 상태·비밀을 복사하지 않는다.

```text
python -m unittest discover -s projects/augment-chess/tests/differential -p test_rule_projection.py
python projects/augment-chess/tests/differential/semantic_differential.py --pairs <외부-paired.jsonl> --report <외부-report.json>
```

paired 각 행에는 `name`, `source`, `rust` Position envelope, `sourceActions`/`rustActions`의 전체 행동 목록, `sourceAction`/`rustAction`의 동일 선택 행동, `sourceObservations`/`rustObservations`의 `white`·`black`, `sourceRejection`/`rustRejection`의 `rejected: true`·`unchanged: true`, `distribution`이 필요하다. 정확 열거 시 `distribution`은 `{"kind":"exact","complete":true,"source":{"결합결과":"1"},"rust":{"결합결과":"1"}}` 형태다. 표본일 때 `kind: sample`, `conditionedOn`, `tolerance`, `alpha`, 양쪽 결과별 정수 count를 제공한다. 종료 코드 0은 제출된 범위의 `PASS`일 때뿐이다. 기존 전체 실행 비교 명령은 `projects/augment-chess/tests/differential/README.md`에 남아 있으며 새 semantic gate의 성공 근거로 승격하지 않는다.

## 남은 구현 위험

- `RuleHistory`/`UndoState`가 아직 분리되지 않아 Replay frame과 표현 event가 규칙 코어에 남는다.
- sound용 probe의 RNG·공유 Replay capture 영향, 복수 무작위 효과의 공통 숨은 변수, Trolley 등 큰 결과 공간의 결합분포가 아직 검증되지 않았다.
- 관측자별 숨은 정보 투영, 다른 기물 ID의 참조 대응, 연속 대국의 미래 의존성, 모든 카드·특수 기물·종료 분기의 확률 열거가 미완료다.
- 전체 상태 diff가 여전히 기존 테스트의 gate다. 이 문서와 새 비교 도구를 추가한 것만으로 Headless Rules Engine 전환이 완료되지 않는다.
