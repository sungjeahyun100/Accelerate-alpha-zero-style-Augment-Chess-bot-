# 모델 loss 모니터 사용법

학습 실험의 epoch·batch loss를 읽어 CSV, 그래프, Markdown 리포트를 만듭니다. Python 3.9 이상이 필요합니다.

## 설치

저장소 루트에서 가상환경을 만들고 의존성을 설치합니다.

```bash
python3 -m venv .venv
source .venv/bin/activate
python -m pip install -r monitoring/requirements.txt
```

## 입력 데이터 준비

작업 디렉터리의 `graph/` 아래에 실험 파일을 둡니다. 두 형식을 함께 사용해도 됩니다.

새 형식은 헤더가 있는 쉼표 구분 파일입니다.

```text
graph/resnet_8x8/epoch-loss.txt
graph/resnet_8x8/batch-loss.txt
```

`epoch-loss.txt`:

```csv
epoch,avg_loss
0,2.4
1,1.8
5,1.2
```

`batch-loss.txt`:

```csv
epoch,batch_num,loss
0,1,2.5
0,2,2.3
1,1,1.8
5,1,1.2
```

구 형식은 헤더가 없는 공백 구분 파일입니다. 공백이 여러 개여도 됩니다.

```text
graph/epoch-loss-legacy_run.txt
graph/batch-loss-legacy_run.txt
```

`epoch-loss-legacy_run.txt`:

```text
0 2.4
1 1.8
5 1.2
```

`batch-loss-legacy_run.txt`:

```text
0 1 2.5
0 2 2.3
1 1 1.8
5 1 1.2
```

두 형식에 같은 실험 ID가 있으면 디렉터리 형식을 사용하고 경고를 출력합니다. 각 파일에는 유효한 숫자가 필요하며, epoch 데이터에는 서로 다른 epoch가 최소 2개 있어야 합니다. 잘못된 숫자나 NaN이 포함된 행은 20% 이하면 경고 후 제외하고, 그보다 많으면 해당 실험을 건너뜁니다. 다른 정상 실험은 계속 처리합니다.

## 실행

저장소 루트에서 실행하는 예시입니다.

```bash
python monitoring/unified_model_monitor.py
python monitoring/unified_model_monitor.py --latest-only
python monitoring/unified_model_monitor.py --experiments resnet_8x8 legacy_run
python monitoring/unified_model_monitor.py --workspace /path/to/run --output-dir model_result_monitoring
python monitoring/unified_model_monitor.py --help
```

기본 작업 디렉터리는 현재 디렉터리입니다. 다른 곳에 `graph/`가 있으면 `--workspace`로 그 상위 디렉터리를 지정합니다. `--output-dir`은 작업 디렉터리 아래에 생성할 결과 디렉터리 이름입니다. `--latest-only`는 입력 epoch 파일 수정 시각이 가장 최신인 실험 하나를 처리하며, 함께 지정한 `--experiments`보다 우선합니다.

## 결과 확인

```text
model_result_monitoring/
  <experiment_id>/
    csv_data/
    plots/
    experiment_report_<experiment_id>.md
  consolidated_csv/
    experiments_comparison.csv
    all_epochs_comprehensive.csv
    all_epochs_statistics.csv
    all_experiments_summary.csv
  comparison_report.md
```

실험이 2개 이상 정상 처리되면 `comparison_report.md`와 통합 CSV가 생성됩니다. 비교 순위는 **training loss 개선량(초기 loss − 최종 loss)** 내림차순입니다. 이는 모델의 실제 대국 성능 순위가 아닙니다.

`Loss Slope / Plateau Analysis`는 epoch에 따른 loss 변화율을 보여줍니다. 신경망 parameter gradient norm이나 기울기 소실을 측정하지 않습니다. 초기 slope가 0에 가까워 비율을 계산할 수 없는 경우 리포트에는 `N/A`, CSV에는 빈 값으로 기록됩니다.

새 형식 epoch CSV에 `policy_loss`, `value_loss`, `elo`, `win_rate`, `inference_ms` 같은 선택 열이 실제로 있으면 비교 CSV에 포함할 수 있습니다. 기록되지 않은 지표는 생성하지 않습니다. 별도 평가·런타임 로그를 연결하려면 `ModelMonitor.collect_metrics()`에서 읽어 `evaluation` 또는 `runtime`에 넣으면 됩니다.

## 테스트

```bash
python -m unittest discover -s monitoring -p 'test_*.py'
```
