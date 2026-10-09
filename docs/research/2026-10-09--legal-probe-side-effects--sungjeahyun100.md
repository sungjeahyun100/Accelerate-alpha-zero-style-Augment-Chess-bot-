# 연구 영수증: Legal Move 후보 검증의 부수 효과 조사

[공유·이름·보존 기준](../ENGINEERING-STANDARDS.md#공유-연구-영수증)을 적용한다. 이 문서는 PR 리뷰 전 조사 초안이며 구현이나 성능 검증의 완료 근거가 아니다.

## 식별과 출처

| 항목 | 기록 |
|---|---|
| 작성 시점 | 2026-10-09 21:31 UTC |
| 마지막 정정 시점 | 해당 없음 |
| GitHub 작성자 | [sungjeahyun100](https://github.com/sungjeahyun100) |
| 관련 PR | [#43](https://github.com/sungjeahyun100/Accelerate-alpha-zero-style-Augment-Chess-bot-/pull/43) |
| 저장소·기준 commit SHA | `sungjeahyun100/Accelerate-alpha-zero-style-Augment-Chess-bot-`, `9747083f0d8515c979100dd9ab3eec19212fdcde` |
| 미커밋 변경 | 기존 비추적 `_native.abi3.so`가 있었으며 조사에서 읽거나 수정하지 않았다 |
| 자료 유형 | 소스 조사, 결론 불충분 |
| 관측 근거의 범위 | 정적 소스 조사. 빌드·테스트·진단·벤치마크 미실행 |

## 목적과 범위

`SourceActionCursor::accepts()`에서 합법성 판정에 필요하지 않은 표현 계산을 안전하게 생략할 수 있는지 조사했다. 기준은 기존 완전 검증과 동결 JS 실행의 행동·오류·RNG·Replay capture 동등성이다. 사용자가 실행한 기존 37개 합법 행동 포지션의 4060ms 측정은 조사 입력이며 이번 작업에서 재측정하지 않았다.

## 재현 설정

작업 디렉터리는 저장소 루트다. Rust 엔진의 `v7_action_surface.rs`, `transition.rs`, `v7_threat.rs`, `threat.rs`, `replay.rs`와 Python의 `legal_diagnostic.py`를 읽었다. 입력은 사용자가 제공한 Position ID `6621740ee684bcb66ff0ebaebabe072590b85352c8875f4785e93736f58bda53`이다. 실행 입력 provenance, seed, 장비, worker, 실행 예산은 소스 조사에는 해당하지 않는다. `cargo test`, `pytest`, `maturin build`, Legal Move 진단, 벤치마크는 사용자 요청에 따라 실행하지 않았다.

## 관측 결과

| 호출과 범주 | 소스 근거 | 판정 |
|---|---|---|
| `SourceActionCursor::accepts_unprofiled`의 transition과 canonicalization | `projects/augment-chess/engine/src/v7_action_surface.rs` | transition의 합법성·오류 결과는 필수. 성공 뒤 canonicalization은 폐기할 clone에만 적용되지만 직렬화 오류를 반환할 수 있어 오류 동등성을 증명하기 전 생략 불가 |
| `play_move_sound_v7_with_control`의 체크 효과음 | `projects/augment-chess/engine/src/v7_threat.rs` | 소리 이름은 표현이지만 위협 probe의 RNG 진행, 공유 Replay capture 명령, `lastMove.soundName` 변경이 함께 일어난다. 범주 C |
| `collect_capture_entry_keys`의 가상 이동 | `projects/augment-chess/engine/src/v7_threat.rs` | 실제 이동 결과로 왕 포획을 판정하고, 실패한 후보에서도 RNG를 다음 후보에 넘긴다. 범주 A/C |
| `replay::record_with_effects_profiled`의 체크 기보 `+` | `projects/augment-chess/engine/src/replay.rs` | 표시 자체는 표현이지만 `evaluate_royal_capture`가 RNG·capture를 변경할 수 있고 기보는 Replay entry·notation state로 저장된다. 범주 C |
| Replay capture journal | `projects/augment-chess/engine/src/replay.rs` | `ReplayCaptureScope`는 `Arc<Mutex<...>>`이며 clone 간 공유된다. 폐기할 `GameState` clone 내부의 명령도 journal에 남을 수 있다. 범주 A/C |
| `transition::apply_without_public_event`의 Replay settle | `projects/augment-chess/engine/src/transition.rs`, `projects/augment-chess/engine/src/replay.rs` | Replay delta는 되감기 규칙 상태이고 식별자는 게임 RNG를 공유한다. 전체 생략 불가. 범주 A |

`ReplayCaptureScope::for_probe`가 기존 scope를 재사용하고 `attach`가 clone에 같은 `Arc`를 연결한다. `collect_capture_entry_keys`는 각 가상 행동 뒤 `child.rng`를 `source_rng`에 복사하며, `IllegalAction`으로 거절한 경우에도 복사한다. 따라서 화면용 함수라는 이름만으로 생략 가능하다고 판단할 수 없다.

## 경고와 오류 및 복구

정적 조사 중 코드 오류는 관측하지 않았다. `gh pr view 43`은 `error connecting to api.github.com`으로 실패해 원격 PR 본문·현재 CI 상태를 확인하지 못했다. 기존 branch의 로컬 기준 SHA는 확인했다. 원격 상태 확인 실패는 소스 분석으로 대체할 수 없으며, push 후에는 원격 SHA를 별도로 확인해야 한다.

## 해석과 한계

- 이번 조사에서 **실제로 생략 가능하다고 증명된 중첩 위협·Replay 계산은 없다**. 입력 상태와 후보만으로 확인되는 안전한 Fast Path 지원 조건도 아직 정의할 수 없다.
- 복제본의 모든 mutable 상태가 폐기된다는 가정은 Replay capture journal 공유와 맞지 않는다. RNG도 위협 probe의 결과뿐 아니라 거절된 가상 후보의 실행 수에 영향을 받는다.
- `checkDanger` 효과음, `+` 기보, 일반 Replay 계산을 일괄 비활성화하면 기존 오류·이력·후속 규칙의 동등성을 주장할 수 없다.
- 따라서 현재는 모든 일반 후보에 기존 완전 검증을 유지하는 것이 안전하다. 명시적인 `fast` 모드를 추가하더라도 근거 없는 동일 실행 fallback만 제공하므로 성능 개선 구현으로 간주하지 않는다.
- 후속 작업은 외부 Position과 capture journal의 독립 전후 snapshot을 비교하고, 일반·특수 규칙 포지션에서 전체 Public Intent 순서·거절 후보·오류·실제 적용 결과를 동결 JS 및 full 경로와 차분 검증한 뒤, 영향이 없는 호출만 선택적으로 분리해야 한다. 실제 성능은 사용자 실행으로 측정해야 한다.

## 공유 전 점검

- [x] 목적·기준 SHA·미실행 이유를 기록했다.
- [x] 정적 사실과 안전성 추론, 사용자 제공 측정과 이번 미측정을 구분했다.
- [x] 절대 경로·로컬 식별 정보·비밀·원시 로그를 포함하지 않았다.
- [x] 공개 GitHub 로그인과 PR 참조를 기록했다.
