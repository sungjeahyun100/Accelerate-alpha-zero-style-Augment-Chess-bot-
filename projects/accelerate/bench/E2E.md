# 로컬 E2E 연결 실험

## 목적과 경계

`accelerate_chess.bench.e2e`는 기존 typed CLI의 초기화·export·source-backed
self-play·terminal replay·실제 optimizer·offline evaluation 계약을 순서대로
호출한다. 별도의 arena는 같은 native `GameAdapterClient`와 공개 tracker,
`ParticleBelief`, `TypedInformationSetSearch`를 이용하고 baseline/trained manifest를
동시에 보유한다. 색상은 game 번호마다 교대하며 두 게임마다 environment seed를
공유한다. 각 게임의 종료 조건은 plies와 wall time이다. 미완료 게임은 승패에
포함하지 않는다.

첫 typed export는 아직 replay가 없으므로 native 초기 위치의 공개 observation과
native 공개 legal intent 페이지에서 sample을 만든다. 실제 public typed encoder로
검사한 뒤 export에 전달한다. Python에서 규칙이나 tensor를 조립하지 않는다.

## 실행

먼저 [기존 로컬 CUDA 환경 절차](README.md#환경)로 현재 checkout의 wheel을
설치한다. 재현할 때 run ID는 새 값으로 지정한다. `${ARTIFACT_ROOT}`는 저장소
밖의 호스트 APPDATA `Accelerate` 루트 또는 명시적인 보존 위치다.

```bash
python -m accelerate_chess.bench.e2e \
  --artifact-root "${ARTIFACT_ROOT}" --run-id local-e2e-001 \
  --model-family mask-resnet --backend ort --device cuda --dtype fp32 \
  --seed 37 --belief-seed 71 --channels 32 --blocks 1 --rank 4 \
  --games 1 --max-plies 64 --elapsed-ms 180000 \
  --iterations 1 --leaf-batch-size 1 --search-ms 5000 \
  --particles 2 --proposals 4 --belief-ms 5000 \
  --steps 1 --batch-size 4 --memory-mib 1024 --arena-games 2
```

기본값도 모두 유한하다. 장시간 실험은 `Ctrl-C`로 취소할 수 있다. 이 실험은
normal game을 사용하므로 짧은 예산에서 terminal이 없을 수 있다. 그때는
`no-terminal-replay`를 기록하고 실제 학습·trained export·평가·arena를
수행하지 않는다. 재시도는 새로운 run ID와 명시적 예산으로 별개로 남긴다.

원시 영수증은 `${ARTIFACT_ROOT}/reports/<run-id>/e2e.json`, 시계열은 같은
디렉터리의 `observer.jsonl`이다. 모델·dataset·training checkpoint는 기존
artifact slot 정책을 따른다. 시계열 수집은 약 1초 간격이며 `/proc`와
`nvidia-smi`를 사용한다. nvidia-smi가 실패하면 GPU 수집만 unsupported다.
각 stage의 `rss_hwm_bytes`는 프로세스 시작 이래 고수위 값이라 stage별
순수 증분이 아니다. CUDA peak allocated/reserved는 stage 시작마다 초기화한다.
CUDA 학습 시 단계별 시간 계측을 위해 동기화하므로 throughput에 계측 비용이
포함된다. Rust ORT 실행 provider는 API로 직접 노출되지 않아 `unknown`이며
CUDA 장치의 존재만으로 self-play GPU 실행을 주장하지 않는다.

## 2026-10-07 UTC 관측

PR #39 최신 기반 `b9db54f06153b779d8042f89d27b6e2df08b58a8`의 미커밋
E2E 코드로 실행했다. RTX 4060 Laptop 8188 MiB, driver 595.91.07,
PyTorch 2.14.0+cu130, Python 3.12.12, power profile `power-saver`였다.
`torch.cuda.is_available()`는 참이었지만 아래 run에서 GPU 학습까지
도달하지 않아 CUDA optimizer 사용은 **미검증**이다.

| run | 예산 | 마지막 단계 | 관측 |
| --- | --- | --- | --- |
| `e2e-smoke-20261007-b` | 2 plies, search 1, belief 1초 | replay-load | terminal 0, decisions 2, simulations 2, inference batches 4. `no-terminal-replay`로 중단. |
| `e2e-smoke-20261007-c` | 64 plies, search 1초, belief 1초 | selfplay 실패 | `SearchBudgetError: belief reconstruction time budget exhausted`. |
| `e2e-smoke-20261007-d` | 64 plies, search 1초, belief 5초 | selfplay 실패 | `SearchBudgetError: no public decision was evaluated before elapsed`. |
| `e2e-smoke-20261007-e` | 2 plies, search 5초, belief 5초 | replay-load | terminal 0, decisions 2, simulations 2, inference batches 4. `no-terminal-replay`로 중단. |
| `e2e-smoke-20261007-f` | 64 plies, search 5초, belief 5초 | selfplay 실패 | 15.006초 뒤 `SearchBudgetError: belief reconstruction time budget exhausted`; RSS HWM 732 MB. |

최신 기반인 `d`의 stage wall time은 bootstrap 0.253초, init 0.027초,
baseline export 0.372초, 실패한 self-play 9.059초다. 프로세스 RSS HWM은
각각 637, 644, 688, 728 MB였다. PyTorch CUDA peak allocated와 reserved는
모두 0 byte였으며 이 수치는 ORT 실행 공간을 포함하지 않는다. observer는
10개 시점을 수집했고 수집 비용 합계 0.266초였다. bootstrap 모델은
328,354 parameters였다.

최종 코드의 `e`에서는 실제 native source에서 나온 두 결정의 unfinished replay를
`ReplayEpisode.load`로 검증했다. Search 소요는 0.907초, 후보 합계는 6개,
self-play wall time은 1.826초이고 프로세스 RSS HWM은 724 MB였다.
시뮬레이션 처리량은 초당 1.095, 완료 대국 처리량은 0 games/hour다.
정확한 NN 평가 position 수는 `SearchResult`에 없어 미측정이다.
terminal episode 0, training example 0이며
optimizer, trained export, offline metric과 arena는 **미실행**이다. 따라서
학습 처리량, baseline 대 trained 결과, 모델 강도는 판단할 수 없다.

현재 측정상 self-play search/belief 경계가 첫 장애 지점이다. `f`에서도
terminal replay 이전에 같은 belief 제한이 발생했다. 4060 단독
환경에서 최적화 우선순위 세 가지를 산정할 만큼 성공한 terminal run과
학습·arena 데이터가 아직 없다. 이 결과를 PR #40 후보의 A/B 기준으로
사용할 때는 먼저 동일한 normal-game 조건에서 terminal replay가 생성되는
유한 예산을 확보해야 한다. 합성 training 수치와 이 E2E 관측을 합치지 않는다.
