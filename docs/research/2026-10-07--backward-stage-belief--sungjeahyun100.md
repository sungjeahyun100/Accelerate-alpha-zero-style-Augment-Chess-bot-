# 연구 영수증: 카드 경계 합성 상태와 공개 belief 비용

[공유·이름·보존 기준](../ENGINEERING-STANDARDS.md#공유-연구-영수증)에 따른 공유 초안이다. PR 리뷰와 병합으로 채택하며, 이 기록을 모델 성능 또는 학습 pipeline 완료 근거로 쓰지 않는다.

## 식별과 출처

| 항목 | 기록 |
|---|---|
| 작성 시점 | 2026-10-07 20:40 UTC |
| 마지막 정정 시점 | 2026-10-07 20:54 UTC |
| GitHub 작성자 | [sungjeahyun100](https://github.com/sungjeahyun100) |
| 관련 PR·이슈 | 이 변경의 별도 draft PR; 작성 시점에는 번호 미생성. #42와 별개 |
| 저장소·기준 SHA | sungjeahyun100/Accelerate-alpha-zero-style-Augment-Chess-bot-, 3552b96fb276dd59e805bc09cf80c975a4525474 |
| 미커밋 변경 | projects/accelerate/python/accelerate_chess/staged.py, cli.py, python/tests/test_staged.py, docs/backward-stage-training.md, 이 영수증 |
| 자료 유형 | 합성 상태 검증과 belief 병목 실패 분석 |
| 근거 범위 | 로컬 실행 및 소스 조사. CI·모델 성능 검증은 미실행 |

## 목적과 범위

- 질문: 실제 v7 카드 경계로 Basic 합성 MIDDLE/END 위치를 만들고 재현할 수 있는가? 공개 이력의 particle belief로 middle→end bootstrap smoke를 끝낼 수 있는가?
- 기준: v7 adapter의 공개 intent 바인딩·전이, 완성된 MIDDLE/END draft 상태, frozen typed 모델. 전체 게임 self-play와의 throughput 비교는 별도 측정 항목이다.
- 성공 기준: 위치 revision과 공개 통계가 재생 시 일치하고, teacher 호출은 END draft와 후속 카드 선택 완료 뒤에만 발생한다.

## 재현 설정

| 항목 | 기록 |
|---|---|
| 엔진 규칙 | augment-site-20260928-e5ed84fcf8e72a24 |
| 공개 catalog SHA-256 | c6eef5c7a9b426625c23ab082b6752196650116d6b2a3900434cd125fd4279db |
| typed encoder SHA-256 | f2420a7e98ead825b24d9f378e552596cdb38124549a24cf7803d327a7c78561 |
| smoke 모델 SHA-256 | c7c8b017787461e620019b99d6d53097510517850e60d9a09763c8d2b24cffbd; seed 37, mask-resnet 8 channels/1 block의 초기 가중치. 학습된 teacher가 아님 |
| 입력 | v7 기본 Basic과 Chaos config, seed 37 |
| 환경 | Linux x86_64, Python 3.12.12, Rust native wheel, CPU/ORT, 1 thread |
| 한도 | 생성 최대 100~160 public actions, stage 최대 80 actions, MCTS 4 iterations, belief 1~4 particles/1~16 proposals, 실행별 외부 90~300초 timeout |
| 종료 방법 | timeout의 INT 뒤 10초 후 KILL. 자식 프로세스가 남지 않았음을 실행 종료 시 확인 |

작업 디렉터리: projects/accelerate. 실제 생성물 루트는 외부 임시 슬롯이며, 아래 명령은 이를 ${ARTIFACT_ROOT}로 정규화했다.

    python -m accelerate_chess.cli --model-family mask-resnet --artifact-root "${ARTIFACT_ROOT}" stage-sample --stage middle --generate-only --manifest "${ARTIFACT_ROOT}/models/student-deployment/manifest.json" --seed 37 --run-id middle-generate --max-generation-actions 100 --capture-bias 1
    python -m accelerate_chess.cli --model-family mask-resnet --artifact-root "${ARTIFACT_ROOT}" stage-sample --stage middle --generate-only --source-provenance "${ARTIFACT_ROOT}/datasets/middle-generate/position-provenance.json" --manifest "${ARTIFACT_ROOT}/models/student-deployment/manifest.json" --seed 37 --run-id middle-replay
    python -m accelerate_chess.cli --model-family mask-resnet --artifact-root "${ARTIFACT_ROOT}" stage-sample --stage end --generate-only --source-provenance "${ARTIFACT_ROOT}/datasets/middle-generate/position-provenance.json" --manifest "${ARTIFACT_ROOT}/models/student-deployment/manifest.json" --seed 37 --run-id end-from-middle --max-generation-actions 100 --capture-bias 1

## 관측 결과

| 실행 | 상태 | 표본 | 핵심 수치·오류 | 근거 |
|---|---|---:|---|---|
| Basic MIDDLE 생성 | 성공 | 1 위치, 26 action | 53.25초, move count 20, 양측 10턴, 기물 백 14/흑 15, 왕 양측 존재, 합법 행동 37 | ${ARTIFACT_ROOT}/reports/middle-generate/stage-generation.json |
| 동일 seed·설정에서 새로 생성 | 성공 | 1 위치, 26 action | 53.51초, 첫 실행과 action 전체·전후 position ID·assumed ply·공개 통계 일치 | ${ARTIFACT_ROOT}/reports/middle-generate-repeat/stage-generation.json |
| 동일 위치 action 재생 | 성공 | 1 위치, 26 action | 11.64초, 매 revision·draft phase·최종 통계 일치 | ${ARTIFACT_ROOT}/reports/middle-replay/stage-generation.json |
| MIDDLE에서 END 연장 생성 | 성공 | 1 위치, 총 49 action | 114.48초, move count 40, 양측 20턴, 기물 백 13/흑 14, 왕 양측 존재, 합법 행동 24 | ${ARTIFACT_ROOT}/reports/end-from-middle/stage-generation.json |
| Chaos MIDDLE 생성 | 성공 | 1 위치, 28 action | 66.35초, move count 20, 양측 10턴, 드래프트 intent 4개 모두 bundleIndex와 cardInstanceIds 유지 | ${ARTIFACT_ROOT}/reports/chaos-middle-generate/stage-generation.json |
| 초기 상태부터 END 생성 | 시간 상한 종료 | 0 완료 위치 | 180초 timeout, exit 137 | 외부 원시 실행 기록 |
| middle→end bootstrap, 1 proposal | 실패 | 0 학습 sample | ParticleExhaustedError: no source-valid particles reproduce the complete public trace within the finite proposal budget; exit 2 | ${ARTIFACT_ROOT}/reports/middle-probe-5/stage-sample-failure.json |
| middle→end bootstrap, 16 proposals | 실패 | 0 학습 sample | SearchBudgetError: belief reconstruction time budget exhausted (120초); exit 2 | ${ARTIFACT_ROOT}/reports/middle-probe-6/stage-sample-failure.json |
| 검증 재생 후 middle→end bootstrap, 16 proposals | 실패 | 0 학습 sample | 위치 재생 뒤 SearchBudgetError: belief reconstruction time budget exhausted (300초); exit 2 | ${ARTIFACT_ROOT}/reports/middle-probe-7/stage-sample-failure.json |
| 동등 예산 full-game vs staged | 미측정 | 0 | staged 학습 sample가 생성되지 않아 처리량 비교 불가 | 해당 없음 |

### 경고와 오류 및 복구

| 단계 | 실제 진단·위치 | 상태 | 처리 | 남은 영향 |
|---|---|---|---|---|
| Python import | 시스템 LD_LIBRARY_PATH의 오래된 libtorch가 PyTorch import에서 undefined symbol: _PyCode_SetExtra 발생 | exit 1 | 해당 실행에서만 LD_LIBRARY_PATH 제거, 초기화·export 성공 | 일반 실행 환경의 경로 충돌은 별도 관리 필요 |
| 공개 belief 재구성 | projects/accelerate/python/accelerate_chess/search.py의 ParticleBelief.rebuild: proposal 고갈 또는 elapsed_ms 초과 | exit 2 | proposal 1→16, belief 30→120→300초를 명시해 재실행. 모두 실패 | MCTS stage sample와 frozen teacher bootstrap 실측 미검증 |
| END 직접 생성 | 초기 상태부터 180초 안에 완료되지 않음 | exit 137 | 완료한 MIDDLE 행동을 검증 재생한 뒤 END로 연장하여 성공 | 두 방식은 계산 경로가 달라 속도 비교로 해석 불가 |

## 산출물, 해석과 한계

- 외부 원시 자료: ${ARTIFACT_ROOT}/datasets/middle-generate, middle-replay, end-from-middle 및 대응 reports. 임시 루트이므로 장기 보존 보장은 없다. 핵심 수치와 실패 원인을 이 영수증에 선별했다.
- 확인한 사실: Basic에서 실제 엔진 draft transition을 통해 MIDDLE과 END의 비terminal 합성 위치를 만들었다. MIDDLE 위치의 기록된 action 재생과 동일 seed·설정의 독립 재생성은 action, revision 및 공개 통계가 같았다.
- 추론: 공개 이력에서 상대 행동을 조건화하며 particle을 재구성하는 비용이 작은 stage smoke의 주요 병목이다. 결과는 seed 37, 작은 무학습 모델과 명시한 예산에 한정된다.
- 미검증: end teacher 학습, middle→end bootstrap dataset, Chaos의 stage rollout 및 teacher bootstrap, full-game 대 staged sample throughput, GPU memory, 승률/Elo. Chaos 생성 시 bundle intent를 보존한 것과 전체 staged 학습 검증을 구분한다.
- 후속 작업: 합성 위치에서 belief 재구성의 단계별 비용·정합성을 측정하고, 비공개 transition에만 조건화를 적용하는 현재 계약을 유지하며 최적화한다. 이후 동결 end teacher smoke와 동등 예산 A/B를 수행한다.

## 정정과 공유 점검

추가 실행은 같은 연구 질문의 이 파일에 반영한다. 본문에는 저장소 상대 경로와 ${ARTIFACT_ROOT} 기준 경로만 사용했고, 원시 로그·모델·데이터를 포함하지 않았다. 로컬 검사와 CI, 기능 완료와 모델 성능을 구분했다.
