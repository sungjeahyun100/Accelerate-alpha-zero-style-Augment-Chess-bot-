# AI 전용 규칙 커널: 첫 합법성 분리 단계

## 기준과 실행 경로

이 단계의 기준은 `origin/develop`의 `12e98c380e1bc8e405f03ba1b7d57eabdc004ce5`다.
`git fetch origin develop`로 이 SHA가 현재 원격의 `develop`임을 확인했다.
로컬 ref의 #43, #44, #45 head는 각각 `bc49fccdf98b43fcae98ecb7a38e69451eadd009`,
`22c42603dbdb61110391f15e7fd629534a710da8`,
`fd63778df749dced142825afaac9fb110d8ccb37`이다. GitHub API에서 세 PR이
모두 열린 `develop` 대상이고 서버 head도 위 SHA와 일치함을 확인했다.

- Python 탐색: `search.py`의 `_stream` → native `Position.action_stream()` →
  `movement::ActionCursor::examine()` → `piece_moves` 또는 카드 후보 →
  `Position.apply()` → `movement::validate_action()` → `transition::apply()`.
  일반 이동의 기존 `movement::validate_action()`은 이미 상태를 변경하지 않고
  행마 후보와 비교한다. 이 경로를 v7 Host의 후보 실행 경로와 혼동하지 않는다.
- v7 Host: `V7PublicActionCursor::next_page()` →
  `SourceActionCursor::next_candidate()` → `SourceActionCursor::accepts()` →
  기존 `GameState::clone()` 및 `transition::apply_without_public_event()`.
  개별 공개 행동 결속은 `validate_source_selection()`에서 후보 소속과 전이
  수락을 다시 확인한다. 실제 적용은 v7 Host transaction의 별도 경계다.
- #43은 효과음 호출 제거, Replay capture 정산 분리, `legal_profile.rs` 계측 및
  `semantic_differential.py`를 제공한다. #44는 10월 프로필·`switcheroo`·차분
  자료를 제공한다. #45의 `docs/RUST-REPLAY-AB.md`와 `rust-replay-ab`은
  FullReplay/NoHistoryReplay/NoReplay, 0·10·30·50행동 비교 기준이다.
  NoReplay의 규칙 동등성은 증명되지 않았다. 각 브랜치 전체는 이 단계에 병합하지 않는다.

## 이번 구현 경계

`SourceActionCursor`는 동결 원본으로 확인된 seed-19 첫 플레이의 normal·chaos·grand
상태에서 일반 이동 후보 20개를 한 번만 순수 생성한다. 상태 전체, RNG, history를
확인한 프로필에만 이 판정을 사용한다. 동일한 cursor 상태의 이동은 후보 집합
소속으로 수락하며 매 후보마다 전체 전이·Replay 정산을 실행하지 않는다.
같은 동결 상태의 Relay와 Quantum Mechanics 카드는 사용 가능 정책과 효과의
입력·색별 플래그 형태를 순수 검사한다. 효과 적용과 턴 정산은 실제 적용 단계에
남는다. 프로필 밖의 상태, 다른 카드, 특수 선택은 기존 실행 기반 판정으로 남는다.
이 proof는 cursor의 불변 상태와 함께 소유하므로 다른 revision에 재사용하지 않는다.

`first_play_move_validator_matches_transition_without_mutating_state`는 세 스타일의
20개 이동에 대해 기존 전이의 수락 결과와 새 판정의 수락을 비교하고,
변조된 이동 거절·상태/RNG 불변·프로필 이탈 시 fallback을 검사한다.
`first_play_flag_cards_match_transition_without_consuming_rng`는 두 카드의
기존 전이 수락, 순수 판정, 잘못된 대상 거절, 원본 상태·RNG 불변을 비교한다.
또한 MCTS가 호출하는 `movement::ActionCursor`의 첫 플레이 Relay 후보도 확인한다.
이 검사는 아직 실행되지 않았다. 검사 통과 전에는 일반 v7 이동 전체로
확장하거나 기본 MCTS 경로를 전환하지 않는다.

## 사용자가 실행할 검사와 완료 조건

저장소 루트에서 다음을 실행한다. Codex는 빌드·테스트·벤치마크·MCTS를 실행하지 않는다.

```bash
cargo test -p augment-chess-engine first_play_move_validator_matches_transition_without_mutating_state
cargo test -p augment-chess-engine first_play_flag_cards_match_transition_without_consuming_rng
cargo test -p augment-chess-engine source_ordered_generic_candidate_path_matches_normal_opening
node --test .github/scripts/check-repository-policy.test.mjs
node .github/scripts/check-repository-policy.mjs
git diff --cached --check
```

첫 단계의 완료 조건은 세 스타일의 이동과 두 카드에서 기존 전이와 새 판정의
수락 일치, 변조 입력 거절, 상태·RNG 불변, 변경한 구조 검사 통과다. 전체 규칙·양측
관측·성능 동등성을 의미하지 않는다. 성능 측정은 #45의 공통 행동 시퀀스와
FullReplay 기준선, #43의 합법성 계측을 같은 규칙 프로필·SHA·입력에서
재실행한 뒤 사용자 결과로만 판정한다. 현재 측정 결과는 `NOT RUN`이다.

다음 단계는 실제 MCTS가 사용하는 `movement::ActionCursor`의 지원 범위를
명시하고, 더 많은 카드의 합법 조건을 효과 적용에서 분리하는 것이다. Headless,
왕 위협, Replay history의 기본 경로 변경은 해당 동등성 검증 뒤에 진행한다.
