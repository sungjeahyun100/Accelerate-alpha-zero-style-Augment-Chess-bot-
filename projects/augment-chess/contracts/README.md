# Augment Chess 게임 계약

## 목적

JavaScript oracle, Rust 엔진, Python AI가 같은 게임 상태와 행동을 해석하고 호출할 수 있도록 게임별 계약을 정의합니다. 프로젝트 공통 객체형 어댑터 계약은 저장소 루트의 `packages/adapter-contract/`에 둡니다.

## 책임 범위

- 공통 `GameState`와 `Action` 표현
- 요청/응답 규약
- 직렬화 형식
- 언어 간 호출 인터페이스
- differential test용 데이터 계약

계약 문서는 `schemas/`(JSON Schema)와 `protocol/`(메시지 설명)에 구분해 둡니다.

## 다른 영역과의 관계

Python AI는 `projects/accelerate/native/`의 바인딩으로 Rust 엔진을 호출합니다. JS oracle과 Rust 엔진은 이 게임 계약을 사용해 차분 검증 대상이 됩니다. Rust의 `augment-chess-contracts` crate는 catalog와 NOTICE를 source distribution에 포함하는 자원 경계이며 규칙을 실행하지 않습니다.

## 포함하지 않는 코드

게임 규칙, MCTS, 신경망, 학습, self-play 구현을 두지 않습니다. PyO3 바인딩·maturin
빌드/패키징은 `projects/accelerate/native/`가 소유합니다. JSON은 저장·교환·fixture·검증의
논리 계약으로 유지합니다. 자세한 설계와 적용 시점은
[ARCHITECTURE](../../../docs/ARCHITECTURE.md)와 [DECISIONS](../../../docs/DECISIONS.md)에 있습니다.

## 현재 상태

기존 메시지 설명은 [protocol/](protocol/README.md)에 보존하고, 스키마는 `schemas/`,
예시는 `examples/`에 둡니다. 저장소 루트에서
`node projects/augment-chess/contracts/tools/validate.js`로 검사합니다. Phase 1 JSON
초안은 역사적 제안이며 [미결정 목록](protocol/open-questions.md)과 현재 실행 계약을
혼동하지 않습니다. 설계 채택만으로 타입·배열·JSON 동등성이 검증된 것은 아닙니다.
