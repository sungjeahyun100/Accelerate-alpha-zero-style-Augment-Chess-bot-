# 연구 영수증: typed batch 원본 tensor 생존 기간 A/B

[연구 영수증 기준](../ENGINEERING-STANDARDS.md#공유-연구-영수증)을 따른다.
공유 초안이며 PR 리뷰·병합 전의 실험 기록이다.

## 식별과 출처

| 항목 | 기록 |
|---|---|
| 작성 시점 | 2026-10-06 UTC |
| 마지막 정정 시점 | 2026-10-06 01:40 UTC: 기존 native-bot CI 관측 결과 추가 |
| GitHub 작성자 | [sungjeahyun100](https://github.com/sungjeahyun100), `gh api user`로 확인 |
| 관련 PR | [#39](https://github.com/sungjeahyun100/Accelerate-alpha-zero-style-Augment-Chess-bot-/pull/39) benchmark 기반, [#40](https://github.com/sungjeahyun100/Accelerate-alpha-zero-style-Augment-Chess-bot-/pull/40) 후보 조사 |
| 저장소·기준 | `sungjeahyun100/Accelerate-alpha-zero-style-Augment-Chess-bot-`; PR #39 head `505edfe4271f8d5ee724cf901f5feff32dea4d2c` |
| A/B commit | baseline `32e74abccda21cb004bb154d957dc93a9bf64dd5`; optimized `55337a5251f14cac586b0300b439f3d45354360a` |
| 미커밋 변경 | 측정 시 없음 |
| 자료 유형·근거 | 로컬 합성 메모리 성능 실험; CI·실제 모델 성능 검증 아님 |

## 목적과 범위

**가설:** `batch_typed_positions()`의 stack/pad 후에도 `TypedBatch.positions`와
typed search의 `positions` local이 원본 NumPy 배열을 보유한다. evaluator가
추가 host 입력을 만들 때 동시 생존으로 peak RSS가 높아질 수 있다.

이번 실험은 원본 배열 생존 기간만 바꾼다. `GameState` clone, allocator,
MCTS 구조, dtype, 모델 architecture와 batch 자동 축소는 범위 밖이다.
판정은 stress peak RSS의 의미 있는 감소, 일반 조건 비악화, 약 3% 이상의
지속적인 batching 처리량 저하 여부, 정확성 동일성으로 한다. 3%는 자동 병합
기준이 아니다.

## 재현 설정

| 항목 | 기록 |
|---|---|
| 입력 | `accelerate_chess.bench.typed_memory`의 결정적 공개 v7 observation·candidate; seed 37 |
| encoder | 동일한 `TypedEncoderSpec.from_catalog` 및 digest `f2420a7e98ead825b24d9f378e552596cdb38124549a24cf7803d327a7c78561` |
| profile | normal: 기물 1개, 후보 최대 8개; stress: 기물 32개, record 290개, relation 321개, 후보 최대 48개 |
| batch·반복 | 4, 16, 32, 64 × 각 3회; 조건마다 독립 Python 프로세스, 총 48회 |
| evaluator | `entity-transformer` 순서의 입력을 NumPy로 복사해 추가 host 할당을 모사; 실제 모델 추론 없음 |
| RSS | Linux `/proc/self/status`의 `VmHWM` peak와 `VmRSS` evaluator 입력 복사 직후; `tracemalloc` 미사용 |
| 환경 | Linux x86_64, Intel Core i7-13650HX, Python 3.14.4, NumPy 2.3.5; GPU·PyTorch·ONNX 미사용 |
| 실행·취소 | 프로세스당 1 batch 후 종료; 병렬 worker 없음; 실행 중단은 해당 프로세스 종료 |

측정 명령은 저장소 루트에서 아래 형식으로 실행했다. 실제 실행 환경에서는
Python 3.14의 시스템 NumPy와 별도 설치한 JCS를 사용했다. 각 호출의 stdout
JSON을 `${ARTIFACT_ROOT}/reports/typed-lifetime-ab/{baseline,optimized}/`
아래 `PROFILE-BATCH-REPEAT.json`으로 저장했다. `--non-retaining`은 optimized에만
사용했다. 현재 실행 환경의 산출물 루트는 **임시 영역**이므로 장기 보존 경로가
아니다. 기존 reporter의 `--run-id` 저장 경로는 이 환경의 임시 루트가 Git
경계로 인식되어 사용할 수 없었고, stdout JSON을 동일 봉투 형식으로 보관했다.

```bash
PYTHONPATH=projects/accelerate/python:${BENCH_DEPS} python3 \
  -m accelerate_chess.bench.typed_memory \
  --profile stress --batch-size 64 --seed 37 --repeat 1 \
  --non-retaining > "${ARTIFACT_ROOT}/reports/typed-lifetime-ab/optimized/stress-64-1.json"
```

## 관측 결과

각 칸은 3개 독립 실행의 **median**이다. MiB는 1,048,576 bytes다. 절감은
baseline − optimized이며 음수는 악화다. `steady`는 종료 직전이라기보다
evaluator 복사 직후 RSS로, 이번 실행에서는 모든 조건에서 peak와 같았다.

| profile / batch | peak B/O MiB | 절감 MiB / % | steady B/O MiB | batching B/O ms | batches/s B/O | input / padded MiB |
|---|---:|---:|---:|---:|---:|---:|
| normal / 4 | 44.29 / 44.43 | −0.13 / −0.3% | 44.29 / 44.43 | 0.281 / 0.288 | 3565.1 / 3477.7 | 0.05 / 0.05 |
| normal / 16 | 44.48 / 44.66 | −0.18 / −0.4% | 44.48 / 44.66 | 0.623 / 0.622 | 1605.0 / 1608.3 | 0.21 / 0.22 |
| normal / 32 | 45.39 / 45.41 | −0.02 / −0.0% | 45.39 / 45.41 | 1.059 / 1.073 | 944.1 / 931.8 | 0.41 / 0.44 |
| normal / 64 | 46.54 / 46.65 | −0.11 / −0.2% | 46.54 / 46.65 | 1.997 / 2.004 | 500.7 / 499.1 | 0.82 / 0.88 |
| stress / 4 | 46.60 / 46.70 | −0.10 / −0.2% | 46.60 / 46.70 | 0.346 / 0.361 | 2890.7 / 2771.5 | 0.30 / 0.31 |
| stress / 16 | 47.51 / 47.04 | +0.47 / +1.0% | 47.51 / 47.04 | 0.866 / 0.872 | 1155.4 / 1146.6 | 1.19 / 1.24 |
| stress / 32 | 51.54 / 50.56 | +0.98 / +1.9% | 51.54 / 50.56 | 1.910 / 1.959 | 523.6 / 510.4 | 2.38 / 2.48 |
| stress / 64 | 60.19 / 59.70 | +0.50 / +0.8% | 60.19 / 59.70 | 4.092 / 3.902 | 244.4 / 256.3 | 4.77 / 4.96 |

정확성 회귀 검사에서 retaining/non-retaining batch의 모든 입력 shape, dtype,
mask, padding, ordering, 값과 spec digest가 일치했다. typed search의 두
architecture family에서 evaluator 호출 전에 원본 배열이 해제되고, 고정 logits와
value의 public 확률 결과가 같음을 확인했다. `test_ir.py` 29개와
`test_bench_report.py` 포함 42개 검사는 통과했다. 전체 Python suite는
PyTorch 미설치로 collection 오류 4개(`test_inference_runtime.py`,
`test_model_stack.py`, `test_search.py`, `test_session.py`)에서 중단됐다.
로컬에서는 native evaluator와 고정 seed 전체 search public result 검증이
**미검증**이었다. 이후 기존 [native-bot CI run 37397830283](https://github.com/sungjeahyun100/Accelerate-alpha-zero-style-Augment-Chess-bot-/actions/runs/37397830283)을
실험 head `410c7a5128c70aaac12ea1f9934f89e48e69c460`에서 수동 실행했다.
Linux와 Windows의 설치 wheel Python 검사는 각각 **172 passed, 1 skipped**였다.
두 skip 모두 CUDA BF16 하드웨어 선택 검사였다. 두 CI job은 skip을 거부하는
후처리 규칙 때문에 실패했다. 동결 adapter와 Rust workspace의 Linux·Windows
job 4개는 모두 성공했고, run 전체 결론은 **failure**다. 실제 native evaluator와
전체 search 테스트는 실행됐지만 CI 성공이나 모델 성능 검증으로 해석하지 않는다.

### 오류와 산출물

| 단계 | 실제 오류·원인 | 처리와 남은 영향 |
|---|---|---|
| 전체 Python suite collection | `ModuleNotFoundError: No module named 'torch'`, exit 2; 시험 환경의 PyTorch 부재 | pure typed IR와 benchmark reporter 검사를 별도 실행. native·model 관련 검증은 남음 |
| Linux·Windows native-bot CI 후처리 | 각 `RuntimeError: native bot CI requires real tests with no skips`, exit 1; 각 Python 테스트 172 passed, CUDA BF16 선택 검사 1 skipped | 테스트 assertion 실패는 아님. 전체 CI는 failure로 표시하고 [run](https://github.com/sungjeahyun100/Accelerate-alpha-zero-style-Augment-Chess-bot-/actions/runs/37397830283)과 두 플랫폼 보고 artifact(7일 보존)를 근거로 남김 |
| 표준 artifact 저장 | `ValueError: generated artifacts must be outside the source checkout`; 임시 산출물 루트의 Git 경계 검사 | stdout JSON을 외부 임시 영역에 저장. `report`의 `--run-id` 경로는 미검증 |

- 원시 JSON: `${ARTIFACT_ROOT}/reports/typed-lifetime-ab/baseline/` 24개와
  `optimized/` 24개. 임시 실행 환경에만 있어 장기 재현·공유 보장은 없다.
- 공개 원시 artifact URL: 없음. 원시 기록의 hostname과 절대 경로를 이 문서에 복사하지 않았다.

## 해석과 한계

원본 feature가 evaluator 시점에 해제된 것은 참조 수명 테스트로 확인했다.
하지만 stress batch 64의 RSS 중앙값 감소는 0.50 MiB(0.8%)로, 원본 입력
4.77 MiB보다 작다. allocator의 해제 페이지 재사용·RSS 유지 가능성은 추론이며
이번 측정만으로 원인을 분리할 수 없다. 일반 조건의 peak는 0.02~0.18 MiB
높지만 프로세스 잡음과 구별하기 어렵다. stress batch 4의 중앙값 batching
throughput은 약 4.1% 낮지만 0.3~0.4 ms의 단발 배치 시간에서 지속적인
회귀라고 판정하기 어렵다. 전체 search/evaluator throughput은 측정하지 않았다.

**판정: 정확성의 검증 가능한 부분은 통과, 메모리 개선 채택은 보류.** 이
workload와 환경에서 의미 있는 peak RSS 감소를 확인하지 못했다. 다음 후보로
바로 넘어가기 전에 Python 3.12·실제 evaluator가 있는 환경에서 동일한
독립 프로세스 RSS·throughput을 재측정해야 한다. 관련 Python 테스트는 두 OS의
설치 wheel에서 각각 통과했으며 선택적 CUDA skip 때문에 CI gate는 실패했다.
이 기록은 모델
성능 개선이나 배포 승인 근거가 아니다.

## 정정과 후속 기록

| 시점 | 변경·후속 기록 | 기존 결론에 미치는 영향 |
|---|---|---|
| 2026-10-06 01:40 UTC | 기존 native-bot workflow run의 Linux·Windows Python 결과와 skip gate 실패 추가 | 정확성의 실행 범위는 넓어졌고, RSS 개선 보류 판정은 그대로 |

후속 측정은 같은 연구 질문이므로 이 영수증에 추가한다.
