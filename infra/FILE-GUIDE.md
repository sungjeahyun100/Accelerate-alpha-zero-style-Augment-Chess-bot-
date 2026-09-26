# 파일 안내 (infra/)

`infra/`는 증강체스 엔진 프로젝트(Vamp-pire/Augment-Chess-Engine-Twist)에서 가져온 JavaScript oracle과 기존 검증·실험 도구를 보존하는 영역입니다. 최종 봇이나 AlphaZero self-play의 주 엔진이 아니라, 사이트 규칙 비교와 Rust 엔진 포팅의 correctness 기준으로 사용합니다. 각 스크립트는 상대 경로(`../engine-merged.js` 등)로 서로를 부르므로 **`infra/` 폴더 안의 배치를 바꾸지 마세요.** 이 폴더 안에서 `node smoke-merged.js`가 통과하는 것을 확인했습니다.

"향후 활용" 열은 기존 코드를 Rust/Python 구현의 검증·참고 자료로 어디에 활용할 수 있는지 설명합니다. 실제 AlphaZero 실행 경로는 `Python AI → bridge → Rust engine`입니다.

## 1. 규칙 엔진 (환경)

| 파일 | 하는 일 | 향후 활용 |
|---|---|---|
| `engine-merged.js` | 증강체스 규칙 엔진 본체(약 1만 8천 줄). 사이트 AI 워커의 규칙(수 생성, 카드, 특수 기물, 판정)을 옮긴 것이고, 알파-베타 검색과 수제 평가도 들어 있습니다. 내보내는 함수: `generateActions`(가능한 수), `applyAction`(수 적용), `cloneState`(상태 복제), `evaluateState`, `searchBestAction`, `setWorkerBoardDimensions` | Rust 포팅의 oracle/reference implementation. 수 목록과 상태 전이의 expected behavior를 제공합니다 |
| `smoke-merged.js` | 특수 기물과 카드가 있는 판에서 검색이 죽지 않는지 보는 빠른 테스트 | 엔진을 고친 뒤 최소 확인 |

사용 시 알아 둘 것: 손으로 만든 상태에는 `turnsTaken`, `actionsRemaining`, `moveCount`, `castlingCanceled`를 꼭 채우세요. 빠지면 루트 안전 검사가 모든 후보를 버려서 점수 2.5억, 노드 0으로 끝납니다.

## 2. 자가대국

| 파일 | 하는 일 | 향후 활용 |
|---|---|---|
| `selfplay-worker-merged.js` | 게임 한 판을 두는 기존 루프(`playOneGame`). 반복 수와 50수 무승부 규칙, 탐색 무작위성(탐색 초반 무작위 수), 색별 깊이/시간/평가 함수 지정, 핸디캡(대전용)을 지원합니다. 각 수의 국면, 검색 점수, 깊이, 승패, 종료까지 남은 수를 기록으로 남깁니다 | 과거 자가대국 형식과 종료 처리의 참고 자료. 새 AlphaZero self-play는 `python/`과 Rust 엔진을 사용합니다 |
| `selfplay-run-merged.js` | 정해진 시간 동안 기존 자가대국을 반복해 `.jsonl`로 저장하는 실행기 | 기존 데이터 수집 재현과 비교용 |

## 3. 국면 인코딩과 학습 (`nnue/`)

| 파일 | 하는 일 | 향후 활용 |
|---|---|---|
| `nnue/encode.js` | 국면을 신경망 입력 5509개(기물 40종 x 128칸, 카드 184장 x 2, 평가 특징 21개)로 바꿉니다 | 정책/가치망 입력으로 재사용. 입력 표현을 새로 설계할 때 출발점 |
| `nnue/encode-worker.js` | 인코딩을 여러 프로세스로 병렬 처리 | 대량 데이터 인코딩 |
| `nnue/forward.js` | 학습된 가중치로 순전파 계산(브라우저와 노드 공용) | 추론 코드 참고. 정책 헤드를 추가하려면 여기부터 |
| `nnue/train.js` | 학습 본체(tfjs). 라벨은 승패와 검색 점수의 혼합, 깊이별 가중, 잔차 학습 옵션 | 가치 학습 참고. 정책 학습(방문 수 분포)은 새로 만들어야 함 |
| `nnue/mix-datasets.js` | 여러 라운드 데이터를 섞고 검증 세트를 분리 | 라운드별 데이터 혼합 |
| `nnue/pipeline-config.json` | 클라우드 학습의 기본 설정 | 학습 설정 예시 |

## 4. 대전과 판정 (알파제로의 "새 모델이 이겨야 교체" 게이트)

| 파일 | 하는 일 | 향후 활용 |
|---|---|---|
| `nnue/match-two-models.js` | 두 평가기를 색 교환 짝으로 대전. 핸디캡, 시드 오프셋, 검색 파라미터, 출력 매핑을 옵션으로 지원 | 새 모델 대 옛 모델의 승격 게이트 |
| `nnue/match-depth.js` | 깊이만 다른 두 설정 대전 | 검색 예산 비교 |
| `tools/lab/lab.js` | 후보 여러 개를 한 번에 대전시키고(`dispatch`), 판정을 모아 `docs/results/results.jsonl`에 기록(`collect`), 진행 확인(`status`) | 자동 개선 루프의 판정 단계 |
| `tools/lab/example-candidates.json` | 후보 목록 예시 | 사용 예 |
| `nnue/tactics.js`, `nnue/tactics-set.json` | 전술 테스트: 깊은 검색이 고른 수를 짧은 검색이 찾는지 측정(75문제) | 대전 없이 싸게 재는 보조 신호 |
| `docs/results/results.jsonl` | 지금까지의 대전 판정 기록(95% 신뢰구간, 판정 win/lose/unproven) | 결과 형식 참고 |

판정 규칙: 95% 구간(윌슨) 하한이 50%를 넘으면 win, 상한이 50% 아래면 lose, 아니면 unproven. 후보가 여럿이면 우연히 통과할 수 있어서 독립 재실행(`seed_offset`)이 필수입니다. 이 프로젝트에서 첫 66% 결과가 재확인에서 재현되지 않은 실제 사례가 있습니다.

## 5. 검증 도구 (`tools/`)

| 파일 | 하는 일 | 향후 활용 |
|---|---|---|
| `tools/ci/golden-eval.js`, `golden-eval.json` | 평가 함수 출력이 안 바뀌었는지 800개 국면으로 회귀 검사 | 평가와 인코딩을 건드릴 때 안전망 |
| `tools/perf/ab-cards.js`, `ab-time.js` | 이전 엔진 대비 고른 수, 점수, 노드, 컷오프가 동일한지와 속도를 비교(카드 켠 버전과 끈 버전) | 속도 개선이 결과를 안 바꿨는지 증명. MCTS 최적화에도 응용 |
| `tools/perf/search-equiv.js`, `eq-exotic.js`, `eval-equiv.js`, `prefilter-check.js` | 검색, 특수 기물, 평가, 필터의 동일성과 안전성 검사 | 위와 같음 |
| `tools/perf/prof-*.js`, `limits-test.js` | CPU 프로파일 수집과 요약, 검색 시간 제한 검증 | 병목 찾기 |
| `tools/fixtures/generate-fixtures.js` | 오라클 정답지(fixture) 생성기, 재현 검증(`--verify`), 오라클 서버(`--serve`, 줄 단위 JSON). 기물과 카드를 골고루 담고 커버리지 보고서를 냅니다 | 러스트 포팅의 정답지. 위치: `tests/differential/fixtures/` |
| `tools/site-parity/*` | 실제 사이트 워커와 엔진의 수 목록, 적용 결과, 여러 수 진행을 무작위 판에서 대조하고 사이트 업데이트를 감지 | 규칙 엔진이 사이트와 같은지 확인. 알파제로에도 규칙 정확성이 전제 |
| `tools/site-parity/README.md`, `TRIAGE.md` | 사용법과 발견된 차이 분류 | 규칙 차이 사례집 |
| `tools/review-calibration/*` | 검색 점수를 승률로 바꾸는 보정 분석 | 가치 출력을 승률로 보정할 때 참고 |

## 6. 클라우드 자동화 (`.github/workflows/`)

GitHub Actions 6개입니다. 모두 `infra/` 폴더 기준으로 동작하도록 고쳐 두었습니다(`infra/` 안에서 실행하고, 산출물 경로에 `infra/`를 붙임). 무거운 작업은 **자동으로 시작되지 않고 손으로 시작(workflow_dispatch)**합니다.

| 파일 | 하는 일 | 시작 방식 |
|---|---|---|
| `ci.yml` | 문법, 스모크, 골든 평가 검사 | `infra/**` 변경 시 푸시와 PR에서 자동 |
| `selfplay.yml` | 자가대국 병렬 실행과 조각 저장(데이터 브랜치 `gha-segments-16cards`에 15분마다 체크포인트), 목표량에 도달하면 스스로 종료 | 수동. 원본은 6시간마다 cron이었으나 뺐음 |
| `dataset-build.yml` | 조각을 시간 창으로 골라 하나의 데이터셋으로 묶기 | 수동 |
| `nnue-train.yml` | 인코딩 캐시를 쓰며 학습하고 가중치를 아티팩트로 저장(`save_model`이면 데이터 브랜치에도) | 수동. 원본의 "학습 결과를 main에 자동 커밋" 단계는 뺐음 |
| `match.yml` | 두 모델의 병렬 대전과 요약(신뢰구간, 판정 파일 `verdict.json`) | 수동 |
| `site-watch.yml` | 사이트 업데이트를 감지하고 규칙 대조 실행 | 수동. 원본은 매일 cron이었으나 뺐음 |

주의:
- 데이터 브랜치(`gha-segments-16cards`)는 첫 자가대국 실행 때 자동으로 생깁니다. 학습, 대전, 데이터셋 빌드는 그 브랜치의 자료가 있어야 돕니다.
- 이 저장소에서 아직 한 번도 실행해 보지 않은 채 경로만 고친 것입니다. **`ci.yml`부터 실행해 보고**, 나머지는 처음 실행할 때 로그를 꼭 확인하세요. 로컬에서는 문법 검사, 스모크 테스트, 골든 평가, YAML 문법 검사까지 확인했습니다.
- 한도: 공개 저장소 기준 동시 작업 20개, 작업당 최대 6시간.

## 7. 문서 (`docs/`)와 라이선스

| 파일 | 내용 |
|---|---|
| `docs/PROJECT.md` | 원 프로젝트의 폴더 지도, 명령, 규칙(일부 경로는 원본 저장소 기준) |
| `docs/PLAN.md` | 계획, 통과 기준, 위험과 대비 |
| `docs/ExperimentNote.md` | 실험 설정, 비율, 결과, 발견 기록(무엇이 안 통했는지 포함) |
| `docs/CONTRIBUTING.md` | 팀 작업 방식 제안 |
| `SOURCE-LICENSE`, `SOURCE-NOTICE.md` | 원본 저장소의 라이선스(CC BY-NC-ND 4.0)와 적용 범위 고지 |

## 8. 후속 영역에서 새로 만들어야 하는 것

- `bridge/`: 공통 상태·행동·호출 계약
- `rust-engine/`: 실제 탐색과 self-play에 사용할 규칙 엔진
- `tests/differential/`: JS oracle과 Rust 엔진의 동등성 검사
- `python/`: MCTS, 정책/가치망, 자가대국, 학습과 평가

이 항목들은 현재 구조 정리의 범위에 포함되지 않으며 [루트 로드맵](../docs/ROADMAP.md)의 순서에 따라 구현합니다.

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

이 폴더의 코드는 Vamp-pire/Augment-Chess-Engine-Twist에서 복사한 것입니다. 원본 라이선스는 CC BY-NC-ND 4.0(저작권자 Vamp-pire)이고 적용 범위는 `SOURCE-NOTICE.md`에 있으니, 수정해서 배포하려면 저작권자와 조건을 먼저 정하세요. 클라우드 자동화는 `.github/workflows/`에 있습니다(6번).

## 11. 원본 동기화 기록

`infra/`의 원본 복사본을 Vamp-pire/Augment-Chess-Engine-Twist와 맞춘 기록입니다. 원본 커밋은 해당 저장소 `master`의 커밋 해시입니다.

| 날짜 | 원본 커밋 | 동기화한 파일 | 동기화하지 않은 것 |
|---|---|---|---|
| 2026-09-26 | `b297ab3f975cabc837bfc74961374b97cae879c9` (2026-09-26 10:52 KST) | `engine-merged.js`(scarecrow 9월 22일 사이트 패치 수정, `orderActions` 캐시, 도구용 export), `selfplay-run-merged.js`(`SELFPLAY_WORKERS`), `tools/site-parity/common.js`(카드 풀 240장, 날짜 기반 시드), `tools/site-parity/parity-actions.js`, `parity-apply.js`, `parity-playout.js` | `nnue/encode.js`, `nnue/train.js`, `nnue/encode-worker.js`, `nnue/match-two-models.js`, `nnue/pipeline-config.json`, `selfplay-worker-merged.js`, `package.json`, `package-lock.json`(원본에서 `nnue/sparse.js`, `tools/site-rules/`, `tools/mcts/`, `tools/tune/` 등 이 저장소에 없는 파일과 묶여 있음), `tools/site-parity/last-seen.json`(감시 상태 파일), `tools/site-parity/README.md`, 원본에만 있는 새 파일(`make-fast-worker.js`, `diff-fast-worker.js` 등), 모델 가중치와 데이터 |
