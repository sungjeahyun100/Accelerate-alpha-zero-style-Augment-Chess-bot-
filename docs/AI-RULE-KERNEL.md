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

## PR #46 후속: 드래프트 정책과 합법 수 실험 실행기

`card_catalog.rs`가 9월 27일/28일 카드 카테고리와 상대 가중치를 소유하고,
`card_registry.rs`가 ID·종류·효과·활성화를 소유한다. `eligibility.rs`는
보드·RULE에 따른 자격을, `card_incompatibility.rs`는 배타 그룹을 판정한다.
`draft_policy.rs`는 Normal의 단계별 혼합 추첨, Chaos의 6장 추첨과 묶음
재배치, Grand의 별도 카테고리 풀 진입을 구분한다. Grand 풀의 단계별
4/10/7/7장 추첨과 Normal/Chaos의 흑 점수 균형 조정은 기존 실행 경로에 남긴다.
일반/Chaos에서는 가중 선택 뒤 선택 ID가 제외 집합에 들어가고, 다음
선택에서 현재 상태의 자격과 배타성을 다시 계산한다. RULE 시작 선택은
일반 카드 풀과 별도로 유지한다.

10월 조사 근거는 PR #44의
`docs/research/2026-10-08--october-client-rules-audit--sungjeahyun100.md`의
드래프트 가중치·`completeRandom` 절과
`projects/augment-chess/oracle/tools/site-parity/october-draft-distribution.js`다.
확인된 상대 가중치는 Common 85, Rare 70, Epic 58, Legendary 44다.
`october_complete_random_step`은 가중 선택한 카드를 호환성 판정 **전에**
풀에서 제거하고, 거절된 카드도 재추첨하지 않는 순서를 제공한다. 이 함수는
주어진 후보 풀 한 단계의 P를 반환한다. 10월 전체 후보 풀·색별 자격·공개
관측·실행 프로필은 아직 검증되지 않았으므로 `October07Probe`는 게임용
P 프로필로 실행할 수 없게 명시적으로 거절한다. 9월 가중치와 10월 희귀도
가중치를 섞지 않는다. `ChanceDensity`는 실제 P와 제안 Q를 별도 필드로
보존한다. Q 사용 시 이 값만으로 학습 편향이 자동 보정되지는 않는다.

v7 일반 합법 수 집합은 `SourceActionCursor`가 후보를 지연 열거하고 즉시
합법성을 확인해 모은다. 중간 전체 후보 배열과 중복 UI 필터를 제거했다.
이미 순수 검증이 확인된 첫 플레이 이동·플래그 카드 외의 일반 행동은
기존 전이 검증을 유지한다. 따라서 모든 행동의 `GameState` 복제가
제거되었다거나 전체 합법 수 생성이 빨라졌다고 주장하지 않는다.

### 사용자가 실행할 8개 실험

Ubuntu 저장소 루트에서 Release 빌드 후 실행한다. 아래 명령은 예시이며
Codex는 이 실행기나 Rust 테스트를 실행하지 않았다.

```bash
cargo build --release -p augment-chess-engine --bin legal-action-experiment
./target/release/legal-action-experiment --scenario all --mode all \
  --seed 19 --warmup 3 --repeats 20 --cpu-mode performance \
  --threads 1 --output "${REPORT_PATH}"

./target/release/legal-action-experiment --scenario 3 --mode normal \
  --seed 19 --warmup 5 --repeats 100 --output "${REPORT_PATH}"
```

`--scenario`는 1~8 또는 `all`, `--mode`는 `normal`, `chaos`, `grand`,
`all`을 받는다. 1은 무카드 기본 보드, 2는 미사용 OPENING `holdout`, 3은
그 카드를 실제 행동으로 사용한 뒤, 4는 실제 발동한 `diagonal-chess` RULE,
5는 같은 20반수 보드의 MIDDLE `e4` 유무, 6은 카드 없는 20반수 보드,
7과 8은 현재 카탈로그의 공개 ACTIVE/PASSIVE 카드를 각각 한 장씩 단독
추가한 상태다. 20반수는 해당 seed에서 매번 합법 이동 집합으로부터
고정 방식으로 선택한다. 카드 추가는 재현 가능한 합성 지급이며 원본
드래프트에서 해당 ID가 나올 확률의 증거가 아니다. 구성 또는 적용이
불가능한 항목은 이유와 함께 `UNSUPPORTED`로 남긴다.

JSON 각 행에는 규칙 버전, 모드, seed·포지션 ID/hash, RULE·카드 ID,
전체/이동/카드 행동 수, 원시 ns 값, 평균·중앙값·p95, 워밍업·반복 수,
빌드 프로필, 사용자가 기록한 CPU 전력 모드·스레드 수, 검증 상태와
사유가 들어간다. 시간 측정은 완성된 상태에서 전체 합법 수를 생성하는
호출만 감싼다. 상태 구성, 워밍업, 기준 집합 비교, JSON 저장은 제외한다.
전체 집합 대신 일부만 반환하는 경로는 성공한 전체 측정으로 기록하지
않고 `UNSUPPORTED`가 된다.

`--oracle path/to/reference.json`은 원본 JS나 별도로 검증된 고정 기준에서
작성한 의미적 행동 ID 배열을 읽는다. 키 형식은
`s<번호>-<모드>-<variant>`이며, 결과의 `semanticIds`가 비교 형식이다.
이 형식에는 행동 종류·색·출발점·도착점과 플래그·카드 ID·선택 대상이
포함된다. 원본과 Rust의 열거 순서나 표시용 JSON 필드는 비교하지 않는다.
기준이 없으면 특히 3·4·5를 포함한 모든 성공한 생성 결과를
`UNVERIFIED`로 표시한다. 동일 Rust 엔진 출력물을 그대로 기준으로
복사해 원본 동등성 `PASS`를 주장해서는 안 된다. 이 8개 실험은
드래프트 전체 확률분포 검증을 수행하지 않는다.
