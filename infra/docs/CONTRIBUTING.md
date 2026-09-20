# Accelerate 팀 작업 안내 (제안, 팀이 확정하면 고치세요)

Accelerate는 증강체스(augmentchess.org) 비공식 엔진 프로젝트를 4명이 함께 진행하는 팀 이름입니다. 처음 온 사람은 `PROJECT.md`(지도) -> `PLAN.md`(계획과 통과 기준) -> `TODO.md`(할 일) -> `ExperimentNote.md`(실험 기록) 순서로 읽으면 됩니다.

## 역할 분담 (제안)

| 영역 | 맡는 일 | 주로 보는 곳 |
|---|---|---|
| 엔진과 속도 | 검색과 평가 속도, 동일성 검증 | `engine-merged.js`, `tools/perf/` |
| 학습과 데이터 | 자가대국, 데이터셋, NNUE 학습 | `nnue/`, `selfplay-*.js`, `.github/workflows/` |
| 측정과 실험실 | 대전, 판정, 전술 세트, 결과 기록 | `tools/lab/`, `docs/results/`, `nnue/tactics.js` |
| 규칙과 확장 | 사이트 규칙 일치, 확장 UI, 모델 선택기 | `tools/site-parity/`, `extension/` |

## 작업 방식 (제안)

- 사람의 변경은 짧은 브랜치에서 작업하고 PR로 합칩니다. CI(`ci.yml`)가 통과해야 합니다. 자동 파이프라인(자가대국, 학습, 대전)은 데이터 브랜치 `gha-segments-16cards`에만 씁니다. (지금까지의 "master에 바로 푸시" 방식은 1인 작업용이었습니다.)
- 엔진(`engine-merged.js`)을 고치면 `extension/engine.js`를 맞추고(`tools/ci/engine-sync.js`가 검사) `tools/perf/ab-cards.js`로 이전 엔진과 결과가 같은지 확인합니다. 검증은 컴퓨터가 한가할 때 돌립니다.
- 평가나 모델 변경은 실험실(`tools/lab/lab.js`)로 대전해서 95% 신뢰구간 하한이 50%를 넘고, 독립 재실행(`seed_offset`)에서도 같은 방향일 때만 반영합니다. 후보가 여럿이면 우연히 통과할 수 있어서 재확인은 필수입니다.
- 실험 설정과 결과는 `ExperimentNote.md`에 남깁니다.
- 클라우드 동시 작업은 20개까지입니다. 자가대국이 돌 때는 8개를 쓰므로 나머지 12개를 나눠 쓰세요.
- 손으로 만든 테스트 보드에는 `turnsTaken`, `actionsRemaining`, `moveCount`, `castlingCanceled`를 꼭 채웁니다(빠지면 검색이 비정상으로 끝납니다).

## 팀이 정해야 할 것

1. 저장소 이름과 공개 표기를 Accelerate로 바꿀지.
2. 저장소 권한(협업자 추가)과 master 보호(PR 필수 여부).
3. 라이선스: 지금은 저작권자가 `Vamp-pire` 한 명이고 CC BY-NC-ND 4.0(수정본 배포 금지)입니다. 4명이 기여하면 각자의 기여를 어떤 조건으로 합칠지(기여자 동의, 저작권자 표기) 정해야 합니다. `NOTICE.md`의 "개인 프로젝트" 문구도 함께 손봐야 합니다.
4. 사이트 운영자 동의(NOTICE.md가 인용하는 2026-09-19 동의)가 팀 전체의 활동을 포함하는지 확인.
