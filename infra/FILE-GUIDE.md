# 파일 안내 (infra/)

`infra/`는 증강체스 엔진 프로젝트(Vamp-pire/Augment-Chess-Engine-Twist)에서 알파제로 방식 봇을 만들 때 재사용할 수 있는 부분만 모은 것입니다. 각 스크립트는 상대 경로(`../engine-merged.js` 등)로 서로를 부르므로 **`infra/` 폴더 안의 배치를 바꾸지 마세요.** 이 폴더 안에서 `node smoke-merged.js`가 통과하는 것을 확인했습니다.

"알파제로 활용" 열은 알파제로 방식(신경망이 수 추천 + 국면 점수를 내고, MCTS로 탐색하며, 자가대국으로 학습)에서 어디에 쓰이는지입니다.

## 1. 규칙 엔진 (환경)

| 파일 | 하는 일 | 알파제로 활용 |
|---|---|---|
| `engine-merged.js` | 증강체스 규칙 엔진 본체(약 1만 8천 줄). 사이트 AI 워커의 규칙(수 생성, 카드, 특수 기물, 판정)을 옮긴 것이고, 알파-베타 검색과 수제 평가도 들어 있습니다. 내보내는 함수: `generateActions`(가능한 수), `applyAction`(수 적용), `cloneState`(상태 복제), `evaluateState`, `searchBestAction`, `setWorkerBoardDimensions` | MCTS의 "환경". 수 목록과 상태 전이를 여기서 얻습니다 |
| `smoke-merged.js` | 특수 기물과 카드가 있는 판에서 검색이 죽지 않는지 보는 빠른 테스트 | 엔진을 고친 뒤 최소 확인 |

사용 시 알아 둘 것: 손으로 만든 상태에는 `turnsTaken`, `actionsRemaining`, `moveCount`, `castlingCanceled`를 꼭 채우세요. 빠지면 루트 안전 검사가 모든 후보를 버려서 점수 2.5억, 노드 0으로 끝납니다.

## 2. 자가대국

| 파일 | 하는 일 | 알파제로 활용 |
|---|---|---|
| `selfplay-worker-merged.js` | 게임 한 판을 두는 루프(`playOneGame`). 반복 수와 50수 무승부 규칙, 탐색 무작위성(탐색 초반 무작위 수), 색별 깊이/시간/평가 함수 지정, 핸디캡(대전용)을 지원합니다. 각 수의 국면, 검색 점수, 깊이, 승패, 종료까지 남은 수를 기록으로 남깁니다 | 자가대국 데이터 생성. 여기서 검색 결과 대신 MCTS 방문 수 분포를 기록하도록 바꾸면 정책 라벨이 됩니다 |
| `selfplay-run-merged.js` | 정해진 시간 동안 자가대국을 반복해 `.jsonl`로 저장하는 실행기 | 데이터 수집 실행 |

## 3. 국면 인코딩과 학습 (`nnue/`)

| 파일 | 하는 일 | 알파제로 활용 |
|---|---|---|
| `nnue/encode.js` | 국면을 신경망 입력 5509개(기물 40종 x 128칸, 카드 184장 x 2, 평가 특징 21개)로 바꿉니다 | 정책/가치망 입력으로 재사용. 입력 표현을 새로 설계할 때 출발점 |
| `nnue/encode-worker.js` | 인코딩을 여러 프로세스로 병렬 처리 | 대량 데이터 인코딩 |
| `nnue/forward.js` | 학습된 가중치로 순전파 계산(브라우저와 노드 공용) | 추론 코드 참고. 정책 헤드를 추가하려면 여기부터 |
| `nnue/train.js` | 학습 본체(tfjs). 라벨은 승패와 검색 점수의 혼합, 깊이별 가중, 잔차 학습 옵션 | 가치 학습 참고. 정책 학습(방문 수 분포)은 새로 만들어야 함 |
| `nnue/mix-datasets.js` | 여러 라운드 데이터를 섞고 검증 세트를 분리 | 라운드별 데이터 혼합 |
| `nnue/pipeline-config.json` | 클라우드 학습의 기본 설정 | 학습 설정 예시 |

## 4. 대전과 판정 (알파제로의 "새 모델이 이겨야 교체" 게이트)

| 파일 | 하는 일 | 알파제로 활용 |
|---|---|---|
| `nnue/match-two-models.js` | 두 평가기를 색 교환 짝으로 대전. 핸디캡, 시드 오프셋, 검색 파라미터, 출력 매핑을 옵션으로 지원 | 새 모델 대 옛 모델의 승격 게이트 |
| `nnue/match-depth.js` | 깊이만 다른 두 설정 대전 | 검색 예산 비교 |
| `tools/lab/lab.js` | 후보 여러 개를 한 번에 대전시키고(`dispatch`), 판정을 모아 `docs/results/results.jsonl`에 기록(`collect`), 진행 확인(`status`) | 자동 개선 루프의 판정 단계 |
| `tools/lab/example-candidates.json` | 후보 목록 예시 | 사용 예 |
| `nnue/tactics.js`, `nnue/tactics-set.json` | 전술 테스트: 깊은 검색이 고른 수를 짧은 검색이 찾는지 측정(75문제) | 대전 없이 싸게 재는 보조 신호 |
| `docs/results/results.jsonl` | 지금까지의 대전 판정 기록(95% 신뢰구간, 판정 win/lose/unproven) | 결과 형식 참고 |

판정 규칙: 95% 구간(윌슨) 하한이 50%를 넘으면 win, 상한이 50% 아래면 lose, 아니면 unproven. 후보가 여럿이면 우연히 통과할 수 있어서 독립 재실행(`seed_offset`)이 필수입니다. 이 프로젝트에서 첫 66% 결과가 재확인에서 재현되지 않은 실제 사례가 있습니다.

## 5. 검증 도구 (`tools/`)

| 파일 | 하는 일 | 알파제로 활용 |
|---|---|---|
| `tools/ci/golden-eval.js`, `golden-eval.json` | 평가 함수 출력이 안 바뀌었는지 800개 국면으로 회귀 검사 | 평가와 인코딩을 건드릴 때 안전망 |
| `tools/perf/ab-cards.js`, `ab-time.js` | 이전 엔진 대비 고른 수, 점수, 노드, 컷오프가 동일한지와 속도를 비교(카드 켠 버전과 끈 버전) | 속도 개선이 결과를 안 바꿨는지 증명. MCTS 최적화에도 응용 |
| `tools/perf/search-equiv.js`, `eq-exotic.js`, `eval-equiv.js`, `prefilter-check.js` | 검색, 특수 기물, 평가, 필터의 동일성과 안전성 검사 | 위와 같음 |
| `tools/perf/prof-*.js`, `limits-test.js` | CPU 프로파일 수집과 요약, 검색 시간 제한 검증 | 병목 찾기 |
| `tools/site-parity/*` | 실제 사이트 워커와 엔진의 수 목록, 적용 결과, 여러 수 진행을 무작위 판에서 대조하고 사이트 업데이트를 감지 | 규칙 엔진이 사이트와 같은지 확인. 알파제로에도 규칙 정확성이 전제 |
| `tools/site-parity/README.md`, `TRIAGE.md` | 사용법과 발견된 차이 분류 | 규칙 차이 사례집 |
| `tools/review-calibration/*` | 검색 점수를 승률로 바꾸는 보정 분석 | 가치 출력을 승률로 보정할 때 참고 |

## 6. 클라우드 자동화 템플릿 (`workflows-templates/`)

GitHub Actions 파일 6개입니다. **활성화되지 않도록 `.github/workflows/`가 아니라 여기에 두었습니다.** 쓰려면 `.github/workflows/`로 옮기고, 경로를 `infra/` 기준으로 고쳐야 합니다.

| 파일 | 하는 일 |
|---|---|
| `selfplay.yml` | 자가대국 병렬 실행과 조각 저장(데이터 브랜치에 15분마다 체크포인트), 목표량에 도달하면 스스로 종료 |
| `dataset-build.yml` | 조각을 시간 창으로 골라 하나의 데이터셋으로 묶기 |
| `nnue-train.yml` | 인코딩 캐시를 쓰며 학습하고, 모델을 데이터 브랜치에 저장 |
| `match.yml` | 두 모델의 병렬 대전과 요약(신뢰구간, 판정 파일 `verdict.json`) |
| `ci.yml` | 문법, 스모크, 골든 평가 검사 |
| `site-watch.yml` | 하루 한 번 사이트 업데이트를 감지하고 규칙 대조 실행 |

한도: 공개 저장소 기준 동시 작업 20개, 작업당 최대 6시간.

## 7. 문서 (`docs/`)와 라이선스

| 파일 | 내용 |
|---|---|
| `docs/PROJECT.md` | 원 프로젝트의 폴더 지도, 명령, 규칙(일부 경로는 원본 저장소 기준) |
| `docs/PLAN.md` | 계획, 통과 기준, 위험과 대비 |
| `docs/ExperimentNote.md` | 실험 설정, 비율, 결과, 발견 기록(무엇이 안 통했는지 포함) |
| `docs/CONTRIBUTING.md` | 팀 작업 방식 제안 |
| `SOURCE-LICENSE`, `SOURCE-NOTICE.md` | 원본 저장소의 라이선스(CC BY-NC-ND 4.0)와 적용 범위 고지 |

## 8. 알파제로를 위해 새로 만들어야 하는 것

- MCTS(신경망 길잡이 탐색)와 상태 복제 비용 줄이기(`cloneState`가 병목)
- 정책 출력: 카드와 특수 기물이 수백 종이라 "수 하나"를 신경망 출력으로 표현하는 방식이 가장 어려운 설계 과제
- 정책과 가치를 함께 내는 신경망과 그 학습(탐색 방문 수 분포를 정답으로)
- 자가대국 기록에 방문 수 분포 저장
- 위 판정 도구를 이용한 새 모델 승격 루프

## 9. 알아 둘 것 (실험에서 얻은 교훈)

- 자가대국의 40~57%가 무승부라 승패 신호가 약합니다. 판정에는 결정된 게임이 수백 판 필요합니다.
- 우연한 승리를 걸러 내려면 후보를 여러 개 시험할 때 반드시 독립 재확인을 하세요.
- 속도 검증은 컴퓨터가 한가할 때 돌리세요(시간 기반 검사가 부하에 흔들립니다).
- 이 폴더에는 사이트 원본 코드(`site-oracle/`)를 넣지 않았습니다. 사이트 워커 대조 도구는 `fetch-real-worker.js`로 실시간 사이트에서 받아 씁니다.

## 10. 빠른 시작과 출처

```bash
cd infra
npm install                    # 학습 코드(tfjs)를 쓸 때만 필요
node smoke-merged.js           # 통과하면 ALL SMOKE CHECKS PASSED
node tools/ci/golden-eval.js   # 평가 함수 회귀 검사
```

이 폴더의 코드는 Vamp-pire/Augment-Chess-Engine-Twist에서 복사한 것입니다. 원본 라이선스는 CC BY-NC-ND 4.0(저작권자 Vamp-pire)이고 적용 범위는 `SOURCE-NOTICE.md`에 있으니, 수정해서 배포하려면 저작권자와 조건을 먼저 정하세요. `workflows-templates/`의 파일은 자동으로 실행되지 않으며, 쓰려면 `.github/workflows/`로 옮기고 경로를 `infra/` 기준으로 고쳐야 합니다.
