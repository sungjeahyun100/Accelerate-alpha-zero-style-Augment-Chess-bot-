# Bridge

## 목적

JavaScript oracle, Rust 엔진, Python AI가 같은 게임 상태와 행동을 해석하고 호출할 수 있도록 공통 경계를 정의합니다.

## 책임 범위

- 공통 `GameState`와 `Action` 표현
- 요청/응답 규약
- 직렬화 형식
- 언어 간 호출 인터페이스
- differential test용 데이터 계약

계약 문서는 `schemas/`(JSON Schema)와 `protocol/`(메시지 설명)에 구분해 둡니다.

## 다른 영역과의 관계

Python AI는 bridge를 통해 Rust 엔진을 호출합니다. JS oracle과 Rust 엔진은 같은 bridge 데이터 계약을 사용해 differential test 대상이 됩니다.

## 포함하지 않는 코드

게임 규칙, MCTS, 신경망, 학습, self-play 구현을 두지 않습니다. D-004에서는 얇은
PyO3 바인딩·maturin 빌드/패키징과 직접 타입·배열 호출을 채택할 계획입니다.
JSON은 저장·교환·fixture·검증의 논리 계약으로 유지합니다. 자세한 설계와 적용 시점은
[ARCHITECTURE](../docs/ARCHITECTURE.md)와 [DECISIONS](../docs/DECISIONS.md)에 있습니다.

## 현재 상태

**Phase 1 JSON 초안(DRAFT)** 단계입니다. 메시지 설명은 [protocol/](protocol/README.md),
스키마는 `schemas/`, 예시는 `examples/`에 있고 `node bridge/tools/validate.js`로 검사합니다.
D-003은 D-004로 직접 호출/JSON 기록을 구분하도록 보완하며, 나머지 필드와 상태 방식은
검토용 제안입니다([미결정 목록](protocol/open-questions.md)). PyO3 바인딩과 이 초안의
엔진 구현은 없습니다. 설계 채택만으로 타입·배열·JSON 동등성이 검증된 것은 아닙니다.
