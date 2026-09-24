# 로드맵

각 Phase는 이전 단계의 계약과 검증 기준이 갖춰진 뒤 진행합니다. 이 변경에서 수행하는 범위는 **Phase 0뿐**이며 이후 단계의 구현은 포함하지 않습니다.

| Phase | 내용 | 완료 기준 | 상태 |
|---|---|---|---|
| 0 | 프로젝트 구조 및 문서 확정 | 4개 영역과 책임, 의존성 방향, 후속 단계가 문서화됨 | 완료 |
| 1 | Bridge protocol 설계 | `GameState`, `Action`, 요청/응답, 직렬화 규약이 합의되고 문서화됨 | 예정 |
| 2 | JS oracle 인터페이스 정리 | bridge 규약에 맞춘 oracle 호출 경계와 기준 fixture가 정의됨 | 예정 |
| 3 | Rust engine 포팅 | 핵심 규칙 API가 Rust에 구현되고 단위 검사를 통과함 | 예정 |
| 4 | JS ↔ Rust differential test | 주요 상태·행동·종료 결과의 동등성을 자동 검증함 | 하네스 완료, 실제 검증은 Rust 후보 대기중 |
| 5 | Python ↔ Rust bridge 연결 | Python에서 Rust 규칙 API를 안정적으로 호출함 | 예정 |
| 6 | AlphaZero state/action encoding | 상태 입력과 정책 행동 공간이 결정되고 왕복 검증됨 | 예정 |
| 7 | MCTS | Rust 환경을 사용하는 기본 MCTS가 검증됨 | 예정 |
| 8 | Policy/Value Network | 정책·가치 추론과 학습의 최소 파이프라인이 동작함 | 예정 |
| 9 | Self-play | 탐색 방문 분포와 결과를 포함한 자가대국 데이터가 생성됨 | 예정 |
| 10 | Training / Evaluation | 반복 학습, 후보 평가, 승격 흐름이 재현 가능하게 동작함 | 예정 |

담당자와 정량 기준은 각 Phase를 시작할 때 이 문서 또는 관련 결정 기록에 추가합니다.
