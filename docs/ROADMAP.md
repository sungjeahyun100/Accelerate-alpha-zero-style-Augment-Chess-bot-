# 로드맵

각 Phase는 이전 계약과 검증 기준을 갖춘 뒤 진행한다. 규약·설계 채택과 해당 기능의
구현 완료는 구분한다. PyO3/maturin·ONNX·LoRA·FiLM 방향은 D-004~D-006의 병합 시 적용하며,
이번 규약/검사 변경으로 Rust·Python·신경망 구현 단계를 완료 처리하지 않는다.

| Phase | 내용 | 완료 기준 | 현재 상태 |
|---|---|---|---|
| 0 | 책임·저장소·에이전트 규약 | 문서 간 일치, 구조 검사와 오류 경로, 생성물·WSL 기준 | 구조 확정, 규약·검사 추가 |
| 1 | Bridge 논리 계약 | GameState·Action·카드·턴·RNG·결과·거절 의미와 JSON 검증 | JSON 초안·예시 검사 존재, 의미 확정 전 |
| 2 | JS oracle 호출 경계 | 호출 API와 버전이 있는 비교 fixture | JS API·fixture·하네스 존재, 계약 대응 미완료 |
| 3 | Rust 규칙 포팅 | 독립 규칙 API·기물/카드 지원·단위 검사 | 구현 전, C++ 참고 초안 존재 |
| 4 | JS ↔ Rust 비교 | 실제 Rust 후보로 legal/apply/full state/result 비교 | 하네스 존재, 현재 JS 자체 회귀, Rust 후보 대기 |
| 5 | PyO3/maturin 연결 | 직접 호출·JSON 의미 동등성, 타입·배열 소유권·오류·GIL·패키지 검사 | 방식 설계, 구현 전 |
| 6 | state/action/FiLM 조건 encoding | 상태·관측·행동과 조건의 의미/버전/shape, O-001 결정 | 미구현 |
| 7 | MCTS | 실제 다음 행동자, 확률·숨은 정보·종료·실행 한도 검증 | 미구현 |
| 8 | ResNet·FiLM·LoRA·ONNX | 세부 구조 결정, 별도 LoRA 학습, 조건 입력 유지, 정적 병합 수치·backend 비교 | 방향 설계, 모델·export 없음 |
| 9 | Self-play | 충분한 상태/관측·합법 행동·방문 분포·결과·버전/seed 보존 | 새 AlphaZero 구현 전 |
| 10 | Training / Evaluation | 재현 학습, 독립 평가·승격 기준, 자원·모델 산출물 관리 | 새 AlphaZero 구현 전 |

Phase 8에서 층·채널·LoRA rank/적용 층·FiLM 위치와 backend를 실제 모델로 결정한다.
Hypernetwork는 생성·적용·병합 가능 여부의 확장 경계만 준비한다. 생성 주기와 동적
추론은 별도 설계·검증 작업이며 현재 정적 LoRA 경로의 구현 완료 기준에 섞지 않는다.
모델 정확성·export 호환성 검사와 대전 성능·승격은 별도로 기록한다.

담당자와 정량 기준은 Phase 착수 시 기록한다. O-002(사이트/JS 불일치 기준)는 여전히
미결정이다. 기존 infra의 과거 NNUE 실험은 새 AlphaZero 완료나 성능 증거가 아니다.
