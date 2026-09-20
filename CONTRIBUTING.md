# 기여 방법

## 브랜치 (Gitflow)

- `main`: 안정 버전. 직접 커밋하지 않습니다.
- `develop`: 다음 배포를 준비하는 통합 브랜치입니다.
- `feature/*`: 일반 기능이나 문서 작업 브랜치입니다. `develop`에서 분기하고 PR로 `develop`에 병합합니다.
- `release/*`: 배포 직전 정리 브랜치입니다. 완료 후 `main`과 `develop`에 병합합니다.
- `hotfix/*`: `main`의 긴급 수정 브랜치입니다. 완료 후 `main`과 `develop`에 병합합니다.

일반 개발 흐름은 다음과 같습니다.

```text
develop
   └─> feature/*
          └─> PR ──> develop
```

영역이 드러나는 브랜치 이름을 권장합니다.

```text
feature/rust-engine-*
feature/bridge-*
feature/python-*
feature/infra-*
feature/docs-*
```

## 영역별 책임

- `bridge/`: 언어 간 데이터와 호출 계약. 게임 규칙이나 탐색·학습 로직을 넣지 않습니다.
- `rust-engine/`: 실제 봇과 self-play용 규칙 엔진. Python 학습 로직을 넣지 않습니다.
- `infra/`: JS oracle 및 기존 검증·실험 도구. 기존 경로와 동작을 보존합니다.
- `python/`: AlphaZero/MCTS/신경망/self-play/training 연구 코드. 규칙을 별도로 재구현하지 않습니다.
- `tests/differential/`: JS oracle과 Rust 엔진의 동등성 검증만 둡니다.

PR 하나에는 가능한 한 한 영역의 변경만 포함하세요. 여러 영역의 계약을 함께 바꿔야 한다면 변경 이유와 영향 범위를 PR 본문에 명시하세요.

## 작업 순서

1. `develop`을 최신으로 받고 작업 영역에 맞는 `feature/*` 브랜치를 만듭니다.
2. 변경하고, 무엇을 왜 바꿨는지 드러나는 커밋 메시지를 작성합니다.
3. 관련 검사를 로컬에서 실행합니다.
4. 푸시하고 `develop` 대상 PR을 엽니다.
5. 리뷰 승인을 받은 뒤 병합하고 feature 브랜치를 정리합니다.

리뷰 승인 인원 수는 팀 합의 후 확정합니다.

## 리뷰 기준

- 변경 이유, 책임 영역, 확인한 내용이 PR에 적혀 있는가
- 불필요하게 여러 영역이나 기존 경로를 함께 변경하지 않았는가
- bridge 계약 변경이라면 각 언어 소비자와 differential test에 미칠 영향을 설명했는가
- `infra/engine-merged.js`를 고쳤다면 결과 동등성을 `infra/tools/perf/ab-cards.js` 등으로 확인했는가
- 평가/모델을 바꿨다면 `infra/tools/lab/lab.js`에서 95% 신뢰구간 하한이 50%를 넘고 독립 재실행(`seed_offset`)에서도 같은 방향인가

## 기존 infra 작업 시 주의사항

- `infra/` 내부 파일은 상대 경로로 연결되어 있으므로 꼭 필요한 경우가 아니면 이동하거나 이름을 바꾸지 않습니다.
- 손으로 만든 테스트 보드에는 `turnsTaken`, `actionsRemaining`, `moveCount`, `castlingCanceled`를 채웁니다.
- 클라우드 동시 작업은 최대 20개입니다. 무거운 워크플로는 수동으로 시작하고 자원을 조율합니다.
