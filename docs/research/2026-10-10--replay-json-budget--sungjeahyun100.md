# Replay 이력의 JSON 예산 성장 조사

[공유 연구 영수증 기준](../ENGINEERING-STANDARDS.md#공유-연구-영수증)에 따른 조사 초안이다. PR #43 리뷰와 병합 전까지 제안은 채택된 규칙 변경이 아니다.

## 식별과 출처

| 항목 | 기록 |
|---|---|
| 작성 시점 | 2026-10-10 UTC |
| 작성자 | [sungjeahyun100](https://github.com/sungjeahyun100), 원격 저장소 공개 소유자 |
| 관련 PR | [#43](https://github.com/sungjeahyun100/Accelerate-alpha-zero-style-Augment-Chess-bot-/pull/43) |
| 기준 commit | `8937f0ac6cccd890cdb7edb80b95e073941811a9` |
| 자료 유형 | 소스 조사와 사용자 제공 실패 분석. 이번 변경의 사용자 실행은 아직 없음 |
| 범위 | `site-20260928` 동결 실행 프로필과 Replay 탐색. 원본 main SHA-256은 `e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c` |

## 질문과 관측

사용자가 실행한 normal/seed 0/decision 61의 가상 후보 검사에서 `canonical(state)`가 100001 nodes를 넘었다. 상태 필드별 관측은 `replayEvents` 79007, `boardHistory` 10960, `replayTailFrame` 2371, `notationTimeline` 2295 nodes다. 초과 경로 `$/taunt`는 canonical의 정렬 순서에서 한도에 도착한 위치이며 원인 필드가 아니다. 이 값은 한 후보 상태의 관측이며 모든 장기 대국의 필연적 실패를 증명하지 않는다.

## 생성·저장·참조 경로

동결 원본 main 파일은 Git에 보관하지 않는다. 이번 조사 환경에서는 위 SHA와 일치하는 원문을 확인할 수 없었다. 따라서 아래 원본 함수명·소스 위치는 채택된 Rust 대응 주석과 검증된 실행 프로필의 대응 표기이며, 원문 본문을 직접 대조한 결과로 표시하지 않는다. 사용자는 동결 cache를 검증한 뒤 원문 `captureReplayFrame`, `recordBoardHistory`, `replayFrameAt`, `reconstructReplayFrame`, `moveReplay` 호출부를 대조해야 한다. 다른 SHA의 최신 사이트 파일로 이 결론을 확정해서는 안 된다.

| 경로 | 현재 확인한 코드 근거 | 의미 |
|---|---|---|
| frame 캡처 | `engine/src/replay.rs`의 `capture_frame_profiled` (2166행 부근), 실행 프로필의 frame key 목록 | 보드와 다수 규칙 필드, `moveReplay`까지 프레임에 포함한다. 단일 이벤트가 작은 이동 하나만 담는다는 가정은 성립하지 않는다. |
| delta | `replay_delta_profiled` (2227행 부근) | 보드 변경 cell마다 이전·이후 값, 변경한 비보드 필드마다 `before`·`after`를 담는다. 큰 필드가 자주 바뀌면 전체 값이 반복된다. |
| 기록 | `record_with_effects_profiled` (2288~2550행 부근) | 변경·visual·notation이 있으면 `replayEvents`에 event를 추가한다. 누적 event 배열은 일반 기록에서 절단하지 않는다. `boardHistory`는 최근 12개만 남긴다. `replayTailFrame`은 최신 frame이다. |
| 과거 조회 | `v7_history_frame` (128~184행 부근) | `historyViewIndex`에서 base 또는 tail을 출발점으로 event delta를 앞/뒤로 적용한다. 과거 전체 delta가 필요한 조회 범위가 존재한다. |
| 카드 복원 | `engine/src/movement.rs`의 `replay_available` (1000행 부근), `v7_card_turn.rs`의 `replay_move` (700행 부근) | 카드 판단과 복원은 색별 `moveReplay`의 delta와 캡처·규칙 상태를 본다. `replayEvents` 전체와 별개 계약이다. |
| 기타 규칙상 참조 | `v7_move_execution.rs`의 Replay event 길이 비교 (2048~2086행), `v7_queued_effects.rs`의 free move 기록 여부 (1317, 1399행), `movement.rs`의 Relay 초기 profile (1003행 부근) | event *개수*가 기록 중복 방지·특정 profile 판정에 사용된다. 배열 전체 제거는 동등하지 않다. |
| 가상 후보 | `oracle/game-adapter/src/game-adapter.js`의 `actionStream` (500~540행), `apply` (544~580행), `snapshot` (323행 부근) | 각 후보를 원 Position에서 복원·적용하고 새 Position을 만든다. `position`은 상태 전체의 canonical 복사·digest를 수행한다. 후보의 작은 차이도 누적 Replay 이력을 반복 검증·복사한다. |

`engine/src/v7_threat.rs`의 `clone_for_threat_simulation`은 먼저 `state.clone()`으로 이력을 복사한 후 시뮬레이션 창의 이력 필드를 비운다. 복사를 줄일 가능성이 있지만 이 창은 별도 의미를 가진다. 또한 source adapter 후보 검사는 상태 전체를 직렬화하므로 Rust 최적화 하나만으로 JSON 경계의 문제를 해결할 수 없다.

## 의존성 분류

- 합법 행동·Replay 카드: `moveReplay`의 최근 이동 delta와 해당 전후 규칙 상태가 직접 필요하다. 현재 확인된 일반 카드 판정에서 과거 `replayEvents` 전체의 *내용*을 읽는 경로는 없다. 다만 event *길이*를 읽는 Rust 경로가 있으며, 원본 JS의 전체 호출부 확인 전에는 불필요하다고 확정할 수 없다.
- 게임 결과: 현재 board·mode·winner 등 규칙 상태가 직접 근거다. 과거 event 전체가 결과 판정에 쓰이지 않는지 동결 원본 전체 호출부와 차분 검증으로 확인해야 한다.
- 과거 대국 조회·시각화: `replayBaseFrame`, 누적 `replayEvents` delta, 최신 `replayTailFrame`, `historyViewIndex`, `notationTimeline`은 재구성과 화면/기록에 사용된다. `boardHistory`는 짧은 최근 프레임 캐시다.
- 원격 동기화·저장: 동결 원본의 Replay 전송·저장 경로를 원문에서 확인해야 한다. 현재 상태 계약 자체에는 `replayEvents`가 포함되므로 직렬화 경계에서 임의 제거할 수 없다.

## 성장 가설과 진단

가장 유력한 원인은 **이벤트 수의 무한 누적과 양방향 delta의 반복 저장**이다. 이벤트마다 변경 필드 전체의 이전·이후 값이 들어가므로 특정 큰 필드가 자주 바뀌면 이벤트당 크기도 커진다. 79007 nodes가 이벤트 개수 때문인지, 특정 delta의 반복인지, 둘 다인지는 아래 진단을 실행해야 판정할 수 있다.

`--replay-growth`는 기존 bounded Replay search의 각 결정 전과 일반 이동 후에 event 개수·노드·bytes, 최신/최대 delta, board cell·field 변경 수와 최근 변경 필드명을 `report.replayGrowth`에 기록한다. 후보 snapshot이 한도를 넘으면 `jsonBudgetFailure.failedReplayGrowth`에 그 후보의 크기만 기록한다. 원시 상태와 기물 값은 보고서에 넣지 않는다. 노드·bytes 계산은 `canonical`의 charge 모델을 따르며, **필드별 개별 측정치**는 전체 `canonical(state)`의 상위 key 비용을 포함하지 않는다. 진단 과정은 원본 규칙 상태를 바꾸지 않는다.

## 해결책 비교: 모두 후속 제안

| 제안 | 기대 효과 | 규칙 호환성 위험·증명 조건 |
|---|---|---|
| 중복 필드 제거·delta 구조 압축 | 매 event의 반복값 감소 | 원본 저장 wire/과거 복원/온라인 동기화가 같은 형태를 기대할 수 있다. 무손실 인코딩, 정방향·역방향 복원, 원본 상태·관측 동등성 입증 필요. |
| 역사 조회 보존소와 실시간 규칙 상태 분리 | 후보 상태 복사·canonical 비용 감소 | Position ID, 재개/저장/전송, history view index, event 개수의 규칙 사용이 바뀐다. 별도 보존소의 수명·원자성·재현성 계약 필요. |
| 과거 조회 체크포인트와 delta 구간 | 조회 비용 제한·구간 보존 가능성 | 체크포인트만 추가하면 JSON nodes는 오히려 증가한다. 이전 delta 제거에는 조회 가능 범위와 원본 의미 변경 문제가 있다. 원본 전체 범위를 유지하며 외부 보존과 결합해야 한다. |
| 구조 공유 또는 지연 복사 | 가상 후보의 반복 복사 감소 | clone 경계의 가변 별칭·Replay capture scope·거절 후 상태 복원 차분 검증 필요. 동등성이 없는 Fast Path는 도입하지 않는다. |

JSON 상한 증가, 이력 삭제·임의 절단, 원본 JS 변경, 규칙 변경은 이번 단계와 제안 범위에서 제외한다.

## 사용자 실행과 해석

저장소 루트에서 기존에 사용하는 고정된 동결 cache를 `--source`에 지정한다. 출력은 보존할 Windows `%APPDATA%\Accelerate\reports` 아래의 새 디렉터리를 지정한다. Linux에서 임의 경로로 Windows 루트를 대체하지 않는다. 예시는 PowerShell 형태다.

```powershell
$env:ACCELERATE_JSON_BUDGET_DIAGNOSTICS = '1'
node projects/augment-chess/tests/differential/v7-native-differential.cjs --oracle-only --replay-search=normal:0:1:64 --replay-growth --source "$env:APPDATA\Accelerate\cache\site-baseline-20260928-e5ed84fc" --output-root "$env:APPDATA\Accelerate\reports\replay-growth-seed0"
```

보고서 `report.json`의 `replayGrowth`에서 결정별 event count·nodes·`deltaNodes`의 차이와 `largestDelta`, `latestFieldKeys`를 비교한다. `jsonBudgetFailure.failedReplayGrowth`가 있으면 실패한 가상 후보를 저장된 결정 상태와 별도로 비교한다. 서로 다른 seed/스타일과 종료까지 간 실제 대국을 같은 방법으로 측정해야 구조적 발생 범위를 판정할 수 있다. 이번 조사에서는 빌드·테스트·벤치마크·탐색을 실행하지 않았다.

## 한계와 후속

동결 원본 main 본문 직접 대조, 결정 61 재실행, 장기 실제 대국의 실패 비율, 제안별 규칙 동등성은 미검증이다. 사용자 결과를 받은 뒤 같은 영수증에 추세·원문 호출부·복원 차분 근거를 보강한다. 이 기록은 구현 완료·CI 성공·모델 성능 검증이 아니다.
