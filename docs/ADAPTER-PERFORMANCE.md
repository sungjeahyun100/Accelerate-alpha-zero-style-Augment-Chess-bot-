# 게임 어댑터 전이 성능 측정

이 문서는 게임 어댑터의 실제 비용을 같은 동결 client와 같은 계약에서 비교하기 위한 절차다. 기능 동등성 판정은 별도의 규칙 검증 자료를 따른다. 빠른 결과를 얻기 위해 후보·관측·이력을 생략한 구현은 성능 개선으로 인정하지 않는다.

## 기준과 측정 대상

- 기준 원문: `main-OahWs0tU.js`, SHA-256 `e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c`. Acorn parser도 manifest의 해시로 검증한다.
- 실행 profile, rules/catalog version, Node 버전·OS·CPU·샘플 수와 관련 소스의 SHA-256을 결과에 기록한다. profile이 최신 client용으로 확정되기 전의 수치는 탐색 결과로만 다룬다.
- 세 모드마다 `draftDelete: true`, seed `37`로 play 상태를 만든다. 첫 합법 행동을 같은 원본 Position에 반복 적용한다. 각 단계의 반환값을 소비하며 Position ID와 action ID를 결과에 보존한다.
- `sourceVerifyParseCompile`은 매회 새 source 객체에서 원문 확인·Acorn 파싱·VM script 컴파일을 포함한다. Node 프로세스 자체는 재사용하므로 별도의 프로세스 시작 시간을 뜻하지 않는다. `createRuntime`은 준비한 source에서 새 VM을 실행한다. `adapterConstruction`은 bootstrap과 RNG 준비까지 포함한다.
- `newGame`, 원시 후보 목록, 후보 20개 페이지, 전체 합법 행동, 이력 없는 전이, 이력·양측 공개 event가 있는 전이, 양측 관측, 공개 hint, 이력 관측을 따로 측정한다. 내부 `OracleRuntime`의 `restore`·`snapshot`도 각각 측정하며 원시 후보·합법 행동 개수를 기록한다.
- 결과의 p50·p95는 실제 개별 시간의 nearest-rank 통계다. RSS·heap은 단계 전후와 각 호출 직후의 관측치이며, `processMaxRssKiB`는 **프로세스 전체** 최고치다. 동기 호출 중의 순간 peak를 단계별 최고치로 주장하지 않는다.

## 실행과 비교

PowerShell에서 동결 원문의 외부 cache를 환경변수로 지정하고 측정한다. 원문 파일·원시 결과·CPU profile은 Git 밖 `%APPDATA%\Accelerate` 또는 CI의 `$RUNNER_TEMP/Accelerate`에 둔다.

```powershell
$env:ACCELERATE_SITE_BASELINE = Join-Path $env:APPDATA 'Accelerate\cache\site-baseline-20260928-e5ed84fc'
node --expose-gc tests/site-adapter/bench/performance.cjs --style all --samples 20 --cold-samples 5 --label before
```

확인한 병목을 고친 뒤 같은 원문·계약·profile·환경에서 `--label after --compare <before-report의 외부 절대 경로>`로 재실행한다. 비교기는 source/parser 해시, 규칙·catalog·profile 버전, Node·OS·CPU, 샘플 수와 초기·후속 Position ID가 달라지면 비교를 거부한다. `--output`으로 외부 절대 파일을 지정할 수 있고, 기본 보고서는 고정된 `performance-<label>.json` 슬롯을 쓴다.

측정 전에 다른 무거운 작업을 멈추고 같은 전원·CPU 조건을 유지한다. 한 번의 p50 개선만으로 채택하지 않고 반복 실행의 변동 폭, p95, 프로세스 메모리, 세 모드의 전이·관측 결과를 함께 본다. `node --cpu-prof`를 사용할 때 profile 출력 경로를 외부 reports로 지정하고 profile 수집 실행은 전후 시간 비교에 섞지 않는다.

PR #28의 Windows CHAOS·GRAND 흐름에서는 `SearchBudgetError: belief reconstruction time budget exhausted` 두 건이 관측되었다. 이는 이 어댑터 벤치 결과가 아니며, 시간 한도 확대나 후보 누락으로 통과 처리하지 않는다. 원인 분석에는 해당 상태·원문·계약·실행 profile을 고정한 재현 입력과 시간 구간별 측정을 사용한다.

## 결과 기록 규칙

결과를 공유할 때는 실행 SHA, 원문·parser 해시, profile, 모드, 샘플 수, p50·p95, 메모리, 정확성 검사 결과를 함께 적는다. 개선 전후의 원문·profile·계약 또는 초기 Position ID가 다르면 독립 결과로 기록한다. 브라우저 DOM이 있는 환경의 미래 RNG 일치와 전체 규칙 지원은 이 벤치로 입증되지 않는다.

### 원문 준비 단계 예비 측정 (2026-09-28)

최신 동결 client에서 `--stage source --samples 20 --cold-samples 5 --label source-before`를 실행했다. Windows, Node `v22.23.2`, Intel Core 5 210H 기준이다. 이 단계는 게임 계약을 실행하지 않으므로 rules/catalog/profile 버전을 결과에 연결하지 않는다.

| 단계 | 표본 | p50 | p95 |
|---|---:|---:|---:|
| 새 `FrozenClientSource`: 검증·파싱·컴파일 | 5 | 372.4 ms | 580.9 ms |
| 준비된 source에서 새 VM 실행 | 20 | 70.3 ms | 77.9 ms |

프로세스 전체 최고 RSS는 761,432 KiB였다. 이 값은 단계별 메모리 peak나 누수 판정이 아니다. 같은 원문을 재사용하는 객체 경계가 반복 파싱 비용을 피할 수 있는지 확인할 근거이며, 실제 `newGame`·전이 개선 효과는 최신 계약에서 별도로 비교한다.

### v7 전이 기준선과 드래프트 cursor 개선

같은 최신 client, `accelerate-headless-semantic-v7`, Node `v22.23.2`, Windows에서 세 모드 각각 20회(`source` 생성은 5회) 측정했다. 양쪽 보고서의 초기·첫 전이 Position ID와 후보·합법 행동 개수는 같았다. 일반·카오스·그랜드 초기 play 상태는 각각 원시 후보 20개와 합법 행동 20개였고, 드래프트 후보는 각각 3·3·28개였다.

| 드래프트 후보 페이지 | 변경 전 p50 / p95 | 변경 후 p50 / p95 |
|---|---:|---:|
| 일반, 후보 3개 | 113.5 / 120.2 ms | 83.0 / 92.8 ms |
| 카오스, 후보 3개 | 111.9 / 138.4 ms | 80.7 / 88.5 ms |
| 그랜드, 후보 20개 | 249.3 / 380.0 ms | 113.3 / 122.2 ms |

개선은 드래프트·승격·트롤리 선택처럼 이미 만들어진 후보 배열을 페이징할 때 매 후보의 VM 상태를 다시 복원하지 않는 변경이다. play 단계의 source generator와 실제 합법성 전이 검사는 그대로 유지한다. 세 모드에서 두 cursor를 교차 조회한 결과의 순서·내용과 조회 전후의 전체 StepResult가 일치했고, 기존 오라클 검사 12개도 통과했다.

드래프트 개선 후 play 초기 상태의 원시 후보 생성 p50은 7.9~8.1 ms, 전체 합법 행동 계산 p50은 669.0~701.6 ms였다. 20개 후보 각각을 실제 전이로 확인하는 비용이 남는다. 현재 근거로는 이 검사를 건너뛰거나 같은 Position의 결과를 무제한 보관할 수 없다.

성공한 전이에서는 `snapshot()`이 이미 검증한 상태로 한 번 만든 결과를 양측 공개 event와 최종 응답에 재사용한다. 공개 `result(position)`의 입력 검증은 유지한다. 동일 v7 Position에서 검증 포함 결과와 이미 검증된 상태의 결과를 번갈아 100회씩 측정했을 때, 세 모드의 p50은 각각 2.47/2.36/2.43 ms와 약 0.001 ms였고 반환값은 일치했다. 이 수치는 **중복 결과 판정 비용**의 분리 측정이며 전체 전이 처리량의 개선률이 아니다. 통합 전후 실행에서는 다른 동시 작업의 부하 변동이 커서 전체 `apply` 시간의 순수 개선폭은 확정하지 않는다. 전후 비교기는 세 모드의 전체 첫 전이 Position ID가 동일함을 확인했다.
