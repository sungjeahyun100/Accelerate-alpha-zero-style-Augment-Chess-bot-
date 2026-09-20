# 기여 방법

> 틀입니다. 팀이 합의한 내용으로 고쳐 주세요.

## 브랜치 (git flow)

- `main`: 안정 버전. 직접 커밋하지 않습니다.
- `develop`: 다음 배포를 준비하는 통합 브랜치. (아직 없으면 팀이 먼저 만들고, 만들기 전에는 PR 대상을 `main`으로 합니다.)
- `feature/이름`: 기능 하나를 만드는 브랜치. `develop`에서 갈라져 나오고, 끝나면 PR로 `develop`에 합친 뒤 지웁니다.
- `release/버전`: 배포 직전 정리. `main`과 `develop`에 합칩니다.
- `hotfix/이름`: `main`의 급한 버그 수정. `main`과 `develop`에 합칩니다.

## 작업 순서

1. `develop`을 최신으로 받고 `feature/이름` 브랜치를 만듭니다.
2. 고치고 커밋합니다. 커밋 메시지는 무엇을 왜 바꿨는지 한 줄로 씁니다.
3. 푸시하고 PR을 올립니다(양식이 자동으로 채워집니다).
4. 리뷰 승인을 받은 뒤 머지합니다. TODO: 승인 인원 수

## 리뷰 기준 (TODO: 팀 합의)

- 변경 이유와 확인한 내용이 PR에 적혀 있는가
- 엔진(`infra/engine-merged.js`)을 고쳤다면 결과가 이전과 같은지(`infra/tools/perf/ab-cards.js`) 확인했는가
- 평가/모델을 바꿨다면 대전 판정(`infra/tools/lab/lab.js`)에서 95% 신뢰구간 하한이 50%를 넘고 독립 재실행(`seed_offset`)에서도 같은 방향인가

## 알아 둘 것

- 손으로 만든 테스트 보드에는 `turnsTaken`, `actionsRemaining`, `moveCount`, `castlingCanceled`를 꼭 채웁니다(빠지면 검색이 비정상으로 끝납니다).
- 클라우드 동시 작업은 20개까지입니다. 무거운 워크플로는 수동으로 시작하고, 서로 자리를 나눠 쓰세요.
