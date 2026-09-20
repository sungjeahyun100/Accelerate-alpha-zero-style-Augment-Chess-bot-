# Bridge

## 목적

JavaScript oracle, Rust 엔진, Python AI가 같은 게임 상태와 행동을 해석하고 호출할 수 있도록 공통 경계를 정의합니다.

## 책임 범위

- 공통 `GameState`와 `Action` 표현
- 요청/응답 규약
- 직렬화 형식
- 언어 간 호출 인터페이스
- differential test용 데이터 계약

계약 문서는 향후 `schemas/`와 `protocol/`에 구분해 둘 예정입니다.

## 다른 영역과의 관계

Python AI는 bridge를 통해 Rust 엔진을 호출합니다. JS oracle과 Rust 엔진은 같은 bridge 데이터 계약을 사용해 differential test 대상이 됩니다.

## 포함하지 않는 코드

게임 규칙, MCTS, 신경망, 학습, self-play 구현을 두지 않습니다. 구체적인 FFI나 서버 방식도 아직 결정하지 않았습니다.

## 현재 상태

**구현 전**입니다. 이 디렉터리에는 책임 문서와 빈 구조만 있습니다.
