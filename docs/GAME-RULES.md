# 증강체스 규칙 요약

> 알파제로 팀이 규칙을 잘못 이해해서 생기는 버그를 줄이기 위한 문서입니다. 사이트(augmentchess.org)와 `infra/engine-merged.js`의 JS oracle을 현재 규칙 기준으로 삼고, 향후 Rust 엔진은 differential test로 이 동작과의 동등성을 검증합니다.

## 기본

- 표준 체스에 카드와 특수 기물이 더해진 변형입니다.
- TODO: 승리 조건, 무승부 규칙(반복 수, 50수), 턴 구조

## 카드

- 카드는 대부분 턴을 소모하지 않는 무료 행동입니다.
- TODO: 카드 사용 방식, 핸드, 카드풀, 주요 카드의 예외

## 특수 기물

- TODO: 종류와 이동, 대표적인 예외(다칸 기물, 상태 이상 등)

## 사이트와 엔진의 일치

- JS oracle이 사이트와 같은지 확인하는 도구: `infra/tools/site-parity/`
- Rust 엔진과 JS oracle의 동등성 검증 위치(계획): `tests/differential/`
