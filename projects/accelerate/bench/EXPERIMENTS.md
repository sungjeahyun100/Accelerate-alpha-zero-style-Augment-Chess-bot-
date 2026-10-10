# 로컬 성능 실험 기록

## 목적과 측정 경계

RTX 4060 Laptop GPU에서 production 네트워크의 계산 비용을 파악하고, 동일한
synthetic typed workload에서 Mask ResNet과 Entity Transformer의 optimizer
처리량을 비교했다. 여기의 처리량은 준비된 batch의 forward, loss, backward,
AdamW를 포함한다. replay loading, augmentation, self-play, MCTS, 대국 강도는
포함하지 않는다. 실행 방법은 [README](README.md)에 둔다.

## 신뢰할 수 있는 기준에 이른 과정

1. 먼저 production `Fixed8x8ResNet`의 ResNet-S/M, FP32/BF16, 여러 batch에
   대해 inference와 synthetic optimizer training을 측정했다. 목적은 최적 모델
   선정이 아니라 로컬 GPU의 실제 계산 비용 파악이었다.
2. 초기 BF16 training의 optimizer 시간이 비정상적으로 컸다.
   `validate_fp32_training_state(model, optimizer)`가 timed hot loop의 매 step에서
   모든 parameter와 Adam state에 `torch.isfinite(...).all()`을 실행했다. 전체
   상태 검사는 optimize 시작·종료, benchmark 시작·종료, checkpoint 저장
   경계로 옮겼다. 매 step에는 finite loss와
   `clip_grad_norm_(..., error_if_nonfinite=True)`를 남겼다. **수정 전 BF16
   결과는 성능 비교에서 제외했다.**
3. CUDA synthetic training의 CPU 기본 thread 14/14는 run 간 분산이 컸다.
   백그라운드 앱을 정리하고 실행 순서를 교차해도 흔들렸다. intra/inter-op
   1/1에서는 훨씬 안정적이어서 이후 `--torch-threads 1
   --torch-interop-threads 1`을 기준으로 삼았다. 이는 현재 synthetic GPU
   측정 조건이다. 실제 replay loading·augmentation·preprocessing에는 CPU
   병렬성이 다시 필요할 수 있다.
4. 시스템 swap 8 GiB가 거의 가득 찬 상태도 관찰했다. `vmstat 1`의 swap-in,
   swap-out은 대부분 0이고 `wa`도 0이어서 지속적인 active swap thrashing은
   관찰되지 않았다. 가득 찬 swap 자체를 느린 benchmark의 원인으로 단정하지
   않았다. IDE, Codex, Discord 등 백그라운드 작업은 일부 분산에 영향을
   주었고 최종 baseline에서는 가능한 한 종료했다. benchmark가 이를 자동
   통제하지는 않는다.
5. 초기 smoke 일부는 powersave, Transformer smoke 일부는 balanced/default,
   최종 matched-size 36-run은 performance 전원 모드였다. **전원 모드가 다른
   smoke 처리량은 직접 비교하지 않았다.** 초기 smoke는 실행 가능성,
   parameter 수, 대략적 VRAM, benchmark 계약 확인에만 사용했다. 최종 성능
   해석은 같은 performance 모드의 36-run 세트만 사용한다. 당시 raw JSON에는
   전원 모드 필드가 없으므로 이 조건은 실험 기록에 근거하며 JSON 자체만으로
   검증할 수 없다. 이후 실행은 가능한 경우 `power_profile`을 기록한다.

## Typed architecture와 크기 맞춤

기존 legacy benchmark는 Fixed8 전용이었다. production typed 경로의
`MaskResNetPolicyValueNetwork`와 `EntityTransformer`를 비교하기 위해
`typed_training`을 만들었다. 같은 synthetic typed batch를
`TypedBatch.as_family_inputs()`로 family별 입력 계약에 투영한다. record,
relation, candidate, condition의 의미는 같고, Mask ResNet에 필요한
spatial/layout tensor만 추가한다.

기본 Mask ResNet은 channels 128, residual blocks 8, hidden 128, LoRA rank 8로
4,196,226개 parameter다. 기본 Transformer는 hidden 128, blocks 4, heads 4,
FFN 512, LoRA rank 8로 2,232,850개다. 이 크기 차이 때문에 기본 구성의
throughput만으로 architecture 계산 비용을 공정하게 비교하기 어렵다.
benchmark에서만 Transformer hidden을 208로 바꾸면 4,159,570개가 되어 차이는
36,656개, 약 0.87%다. production 기본 hidden 128은 변경하지 않았다.
Parameter 수를 맞추는 것은 계산 비용 비교 조건일 뿐 모델 품질이나 대국 강도를
맞춘다는 뜻이 아니다.

## 최종 실험 조건

| 항목 | 조건 |
| --- | --- |
| CPU/GPU | Intel Core i7-13650HX / NVIDIA GeForce RTX 4060 Laptop GPU, 약 8 GiB |
| 소프트웨어 | PyTorch 2.14.0+cu130, CUDA runtime 13.0 |
| workload | normal: records 16, relations 24, candidates 80, candidate nodes 8, board 8×8 |
| 모델 | Mask ResNet 4,196,226 / Transformer hidden 208, 4,159,570 parameters |
| 정밀도와 batch | FP32/BF16 × 16/32/64 |
| 반복과 step | 조건당 3회, 각 100 timed steps, warmup 3 |
| CPU thread | intra-op 1 / inter-op 1 |
| 전원 | performance |

총 2모델 × 2 dtype × 3 batch × 3회 = 36 run이다. 시간·온도 bias를 줄이려고
모델과 dtype의 실행 순서를 반복마다 일부 교차했다. 벤치 입력은 준비된 synthetic
typed batch이며 실제 게임 분포를 대표한다고 가정하지 않는다.

## 최종 처리량

아래 값은 36개 raw JSON을 reporter로 다시 집계한 mean이다. 표본 표준편차,
median, CV, step breakdown, VRAM, 각 조건의 source run ID는 재생성된
`summary.md`와 `summary.json`에 있다. delta는 `(Transformer − Mask) / Mask × 100`이다.

| dtype / batch | Mask samples/s | Transformer h208 samples/s | Transformer delta |
| --- | ---: | ---: | ---: |
| FP32 / 16 | 1,122.6 | 1,646.4 | +46.66% |
| FP32 / 32 | 1,909.3 | 2,308.3 | +20.90% |
| FP32 / 64 | 2,437.7 | 2,716.3 | +11.43% |
| BF16 / 16 | 974.0 | 1,416.2 | +45.39% |
| BF16 / 32 | 1,848.3 | 2,388.8 | +29.24% |
| BF16 / 64 | 2,816.0 | 3,012.9 | +6.99% |

가장 높은 처리량은 Transformer h208 BF16 batch 64의 약 3,013 samples/s,
21.24 ms/step이다. 같은 조건 Mask ResNet은 약 2,816 samples/s,
22.73 ms/step이다. peak VRAM은 각각 대략 375 MB(358 MiB), 421 MB(402 MiB)로
Transformer가 약 11% 적었다. 정확한 집계 수치는 raw JSON을 우선한다.

## 해석과 다음 실험

약 4.2M parameter로 크기를 맞춘 이 synthetic typed optimizer 측정에서는
Transformer가 테스트한 모든 dtype/batch에서 높은 throughput을 보였다.
이 결과로 Transformer가 게임을 더 잘 두는지, 더 적은 self-play로 강해지는지,
같은 wall-clock 학습에서 승률이 높은지, 최종 production 모델인지 판단할 수 없다.

다음에는 실제 replay loading·augmentation을 연결해 측정한다. 동일 dataset
또는 self-play 생성 조건에서 동일 optimizer/training budget과 동일 wall-clock
budget을 각각 비교하고 policy CE와 value MSE를 기록한다. old/new 또는 A/B
arena에서는 동일 MCTS budget, paired seeds, color swap을 적용해 W/L/D와
confidence interval을 보고한다.

## 보고서 재생성

36개 raw run은 외부 artifact root의 `reports/matched-*/typed-training.json`에
보관한다. 같은 root에 다른 `matched-` run을 추가했다면 prefix가 더 많은
run을 선택하므로 재생성 전에 선택 목록을 확인한다. 결과 파일은 Git에서
무시하는 `projects/accelerate/bench/results/` 아래에 만든다. 기존 report ID는
덮어쓰지 않으므로 재실행에는 새 ID를 사용한다.

```bash
python -m accelerate_chess.bench.report \
  --artifact-root "${ARTIFACT_ROOT}" \
  --report-id typed-matched-rtx4060-performance \
  --run-prefix matched-
```

reporter는 동일 hardware와 구조화된 model/config/workload 조건으로 묶고,
표본 표준편차와 CV를 계산한다. 단일 run의 표준편차·CV는 미정의다. 같은
workload·전원 조건에서 parameter 차이가 2% 이내인 Mask/Transformer만 A/B
표에 넣는다. 서로 다른 전원 모드 또는 모드 미상의 혼합은 경고하며 동일한
aggregate나 A/B pair로 묶지 않는다. 과거 JSON에 전원 필드가 없어도 읽는다.
