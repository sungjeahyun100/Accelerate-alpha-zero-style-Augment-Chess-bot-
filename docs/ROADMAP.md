# 로드맵

각 Phase는 이전 계약과 검증 기준을 갖춘 뒤 진행한다. 규약·설계 채택과 해당 기능의
구현 완료는 구분한다. PyO3/maturin·ONNX·LoRA·FiLM 방향은 D-004~D-006의 병합 시 적용하며,
현재 구현 checkpoint와 전체 **NO-GO** 범위는 [IMPLEMENTATION](IMPLEMENTATION.md)에 기록한다.
소스의 존재·작은 단위 검사·규약 채택으로 전체 단계를 완료 처리하지 않는다.

| Phase | 내용 | 완료 기준 | 현재 상태 |
|---|---|---|---|
| 0 | 책임·저장소·에이전트 규약 | 문서 간 일치, 구조 검사와 오류 경로, 생성물·WSL 기준 | 구조 확정, 규약·검사 추가 |
| 1 | Bridge 논리 계약 | Position·Action·공개 Observation·event·RNG·결과·거절 | v1 구현/검사, 전체 visibility review 진행 |
| 2 | JS oracle 호출 경계 | 최초 동결 client 초기/draft/legal/apply/result 호출 | headless actual adapter 구현, depth0 계약/통합 12개 검사 통과 |
| 3 | Rust 규칙 포팅 | 독립 규칙 API·전체 기물/256카드·단위 검사 | 구현 진행, 전체 semantic coverage 미완료 |
| 4 | JS ↔ Rust 비교 | 실제 Rust 후보로 legal/reject/full state/result/RNG 비교 | 과거349 fixture drift 조사, 전체 client parity 미완료 |
| 5 | PyO3/maturin 연결 | 직접 호출·JSON 동등성, 소유권·오류·GIL·패키지 | Linux wheel/FFI checkpoint 존재, 최종 통합/Windows 별도 확인 |
| 6 | state/action/FiLM 조건 encoding | 관측/history/belief와 후보 행동·조건 의미/version/shape | D-007 Python-first 채택, 구현 및 최종 관측 통합 진행 |
| 7 | MCTS | 실제 다음 행동자, 확률·숨은 정보·종료·실행 한도 검증 | 미구현 |
| 8 | ResNet·FiLM·LoRA·ONNX | 조건 입력 유지, 정적 LoRA 복사 병합 수치·ort/tract 비교 | 구현 진행, checkpoint 검사와 최종 runtime 검증 구분 |
| 9 | Self-play | 충분한 상태/관측·합법 행동·방문 분포·결과·버전/seed 보존 | 새 AlphaZero 구현 전 |
| 10 | Training / Evaluation | 재현 학습, 독립 평가·승격 기준, 자원·모델 산출물 관리 | 새 AlphaZero 구현 전 |

Phase 8에서 층·채널·LoRA rank/적용 층·FiLM 위치와 backend를 실제 모델로 결정한다.
Hypernetwork는 생성·적용·병합 가능 여부의 확장 경계만 준비한다. 생성 주기와 동적
추론은 별도 설계·검증 작업이며 현재 정적 LoRA 경로의 구현 완료 기준에 섞지 않는다.
모델 정확성·export 호환성 검사와 대전 성능·승격은 별도로 기록한다.

O-001은 D-007(Python-first encoding), O-002는 D-008(최초 동결 client 정답)로 해결했다.
이번 GO/NO-GO는 코드·계약 완성 기준이며 실제 학습은 제외한다. 기존 infra의 과거 NNUE
실험은 새 AlphaZero 완료나 성능 증거가 아니다.
