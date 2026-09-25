# Rust Engine

## 목적

실제 봇 탐색과 self-play에 사용할 고성능 증강체스 규칙 엔진을 Rust로 제공합니다.

## 책임 범위

장기적으로 `GameState`, `Action`, `legal_actions()`, `apply_action()`, `is_terminal()`, `result()`에 해당하는 독립 규칙 API를 담당합니다.

## 다른 영역과의 관계

동작은 `infra/`의 JS oracle을 기준으로 differential test에서 검증합니다. Python AI는 `bridge/`의 계약을 통해 이 엔진을 사용하며, Rust 엔진은 Python을 알거나 의존하지 않습니다.

## 포함하지 않는 코드

Python 학습 코드, MCTS, 신경망, JS oracle, 언어별 orchestration을 두지 않습니다.

## 현재 상태 (2026-09-25)

`pre_cpp_engine_code/engine.cpp`(손코딩 스케치)를 그대로 옮긴 **소스 초안**이 `src/lib.rs`에 있습니다.
포팅 범위는 cpp 원본과 동일합니다 — King/Queen/Rook/Bishop/Knight/Pawn 기본 행마와
MOVE/TAKE/CATCH/TAKEMOVE/BOTHTAKEMOVE 해석만 있고, cpp가 `// TODO`로 남긴 부분
(SHIFT, JUMP, `moveChunk.next` 연쇄 해석, `apply_action` 본문, 카드 효과, 킹 세이프티 검사)은
이 초안에도 그대로 없습니다. 새로 추가한 기능은 없고, 언어만 옮겼습니다.

**`Cargo.toml`은 의도적으로 아직 추가하지 않았습니다.** `.github/workflows/differential.yml`은
`rust-engine/Cargo.toml`이 생기는 순간 `cargo build --release` 후
`./rust-engine/target/release/oracle_bridge`를 후보 바이너리로 실행해 JS oracle과 대조합니다.
지금 이 초안은 보드/카드 상태 일부만 다루고 프로토콜 어댑터도 없어서, 여기서 `Cargo.toml`만
먼저 추가하면 `oracle_bridge`가 없어 `develop`의 모든 후속 push에서 differential CI가 깨집니다.
`Cargo.toml`은 `infra/tools/fixtures/run-differential.js` 헤더에 적힌 stdin/stdout 프로토콜을
구현하는 `oracle_bridge` bin과 **함께** 추가하세요.

Phase 순서상(`docs/ROADMAP.md`) 이 저장소는 아직 Phase 0(구조 확정)만 끝났고 Phase 1(bridge
프로토콜)·Phase 2(oracle 인터페이스 정리)는 진행 전입니다. 이 초안은 그 순서를 건너뛴 게
아니라, `pre_cpp_engine_code/engine.cpp`가 이미 존재하는 스케치라 먼저 옮겨 둔 것뿐이며
실제 Phase 3 착수(GameState/Action 최종 형태, bridge 계약 반영)는 Phase 1·2가 끝난 뒤
이 초안을 기준으로 다시 정리하는 편이 맞습니다.

## 다음에 할 일

- Phase 1에서 확정될 `bridge/` 계약에 맞춰 `GameState`/`Action` 필드 재검토
- `apply_move_action`/`apply_card_action` 본문 구현
- 킹 세이프티(자기 킹 체크 노출 금지) 검사 추가
- SHIFT/JUMP, `moveChunk.next` 연쇄 해석
- 카드 효과 적용 로직
- 위가 board/카드 상태 기준으로 충분히 갖춰지면 `Cargo.toml` + `oracle_bridge` bin을 같이 추가해
  `differential.yml`이 실제 교차 검증을 시작하게 하기
