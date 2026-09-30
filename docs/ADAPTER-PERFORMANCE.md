# 게임 어댑터 전이 성능 측정

이 문서는 게임 어댑터의 실제 비용을 같은 동결 client와 같은 계약에서 비교하기 위한 절차다. 기능 동등성 판정은 별도의 규칙 검증 자료를 따른다. 빠른 결과를 얻기 위해 후보·관측·이력을 생략한 구현은 성능 개선으로 인정하지 않는다.

## 기준과 측정 대상

- 기준 원문: `main-OahWs0tU.js`, SHA-256 `e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c`. Acorn parser도 manifest의 해시로 검증한다.
- 현행 실행 profile은 `accelerate-headless-semantic-v7-faithful-init-v1`이다. [실행 manifest](../projects/augment-chess/contracts/catalog/execution-profile-20260928.json)의 원문 선언·binding, 채택 initializer 175개·제외 initializer 168개와 원래 순서, bootstrap·replay metadata digest를 검증한다. manifest digest는 catalog identity에도 포함된다. client SHA가 같아도 이전 profile의 성공과 시간 수치를 현행 profile의 검증으로 재사용하지 않는다.
- 실행 profile, rules/catalog version, Node 버전·OS·CPU·샘플 수와 관련 소스의 SHA-256을 결과에 기록한다. source-only의 profile이 `null`인 자료에는 실제 loader와 initializer 의존성의 출처를 별도로 기록한다.
- 세 모드마다 `draftDelete: true`, seed `37`로 play 상태를 만든다. 첫 합법 행동을 같은 원본 Position에 반복 적용한다. 각 단계의 반환값을 소비하며 Position ID와 action ID를 결과에 보존한다.
- `sourceVerifyParseCompile`은 매회 새 source 객체에서 원문 확인·Acorn 파싱·VM script 컴파일을 포함한다. Node 프로세스 자체는 재사용하므로 별도의 프로세스 시작 시간을 뜻하지 않는다. `createRuntime`은 준비한 source에서 새 VM을 실행한다. `adapterConstruction`은 bootstrap과 RNG 준비까지 포함한다.
- `newGame`, 원시 후보 목록, 후보 20개 페이지, 전체 합법 행동, 이력 없는 전이, 이력·양측 공개 event가 있는 전이, 양측 관측, 공개 hint, 이력 관측을 따로 측정한다. 내부 `OracleRuntime`의 `restore`·`snapshot`도 각각 측정하며 원시 후보·합법 행동 개수를 기록한다.
- 결과의 p50·p95는 실제 개별 시간의 nearest-rank 통계다. RSS·heap은 단계 전후와 각 호출 직후의 관측치이며, `processMaxRssKiB`는 **프로세스 전체** 최고치다. 동기 호출 중의 순간 peak를 단계별 최고치로 주장하지 않는다.

## 실행과 비교

현행 native와 JS의 비교는 [측정 도구 안내](../projects/augment-chess/perf/README.md)의
순서로 통합 담당자가 실행한다. worker 1개·GPU 없음·동일 OS의 공유 Cargo
target을 유지해 하네스 검사, release 빌드, normal 1개 표본 검사,
성공 시 세 모드 각 7개 표본 측정을 순차 진행한다. 미지원·불일치·실행 오류는
실제 kind/code/reason을 보존하고 중단하며 정상 동작으로 숨기지 않는다.
어느 단계도 카드 전체나 후속 턴까지 검증한 것으로 승격하지 않는다.

원문 준비·VM 복원 비용을 분리하는 기존 JS 도구는 아래처럼 작은 표본으로
실행한다. 3개 표본과 source 생성 1개 표본은 경계 진단용이며 최종 성능
안정성의 근거가 아니다. 이 도구의 warmup은 단계별 기본 1회이고 source
생성에는 없다. 내부 전체 시간 한도가 없으므로 WSL에서는 안내의 외부
130초 `timeout`을 사용한다. PowerShell에서는 소유한 실행의 기존 Job Object
또는 수동 취소 경계를 기록한다. 원문 파일·원시 결과·CPU profile은 Git 밖
`%APPDATA%\Accelerate` 또는 CI의 `$RUNNER_TEMP/Accelerate`에 둔다.

```powershell
$env:ACCELERATE_SITE_BASELINE = Join-Path $env:APPDATA 'Accelerate\cache\site-baseline-20260928-e5ed84fc'
node --expose-gc --max-old-space-size=4096 projects/augment-chess/tests/site-adapter/bench/performance.cjs --stage all --style all --samples 3 --cold-samples 1 --label current-vm-boundaries --output (Join-Path $env:APPDATA 'Accelerate\reports\adapter-transition-perf\vm-boundaries.json')
```

확인한 병목을 고친 뒤 같은 원문·계약·profile·환경에서 `--label after --compare <before-report의 외부 절대 경로>`로 재실행한다. 비교기는 source/parser 해시, 규칙·catalog·profile 버전, Node·OS·CPU, 샘플 수와 초기·후속 Position ID가 달라지면 비교를 거부한다. `--output`으로 외부 절대 파일을 지정할 수 있고, 기본 보고서는 고정된 `performance-<label>.json` 슬롯을 쓴다.

기존 JS 비교기는 initializer manifest와 `reviewed-initializers.js`를 별도
비교 키로 묶지 않는다. 특히 `--stage source`는 rules/catalog/profile이
`null`이므로 이전 reviewed initializer 23개 실행과 현행 175개 실행의 차이를
자동 비교만으로 막지 못한다. source-only의 과거 p50/p95는 loader·manifest·
bootstrap·semantic profile 변화까지 확인한 뒤 독립된 관측으로 다룬다.
준비한 immutable `FrozenClientSource`를 같은 프로세스에서 공유하는 비용과
mutable VM의 Position 복원·RNG 상태를 구분한다. 두 성능 도구는 GC·표본
방법이 다르므로 서로의 phase 수치를 전후 개선 비율로 섞지 않는다.

측정 시점은 다른 담당자와 조율하고 같은 전원·CPU 조건을 유지한다. 조율할 수 없는 동시 부하는 결과에 기록하며 다른 작업자의 실행을 임의로 중단하지 않는다. 한 번의 p50 개선만으로 채택하지 않고 반복 실행의 변동 폭, p95, 프로세스 메모리, 세 모드의 전이·관측 결과를 함께 본다. `node --cpu-prof`를 사용할 때 profile 출력 경로를 외부 reports로 지정하고 profile 수집 실행은 전후 시간 비교에 섞지 않는다.

PR #28의 Windows CHAOS·GRAND 흐름에서는 `SearchBudgetError: belief reconstruction time budget exhausted` 두 건이 관측되었다. 이는 이 어댑터 벤치 결과가 아니며, 시간 한도 확대나 후보 누락으로 통과 처리하지 않는다. 원인 분석에는 해당 상태·원문·계약·실행 profile을 고정한 재현 입력과 시간 구간별 측정을 사용한다.

## 결과 기록 규칙

결과를 공유할 때는 실행 SHA, 원문·parser 해시, profile, 모드, 샘플 수, p50·p95, 메모리, 정확성 검사 결과를 함께 적는다. 개선 전후의 원문·profile·계약 또는 초기 Position ID가 다르면 독립 결과로 기록한다. 브라우저 DOM이 있는 환경의 미래 RNG 일치와 전체 규칙 지원은 이 벤치로 입증되지 않는다.

## Rust 공개 어댑터와 비용 분리

`projects/augment-chess/perf/measure.cjs`와 `perf/native`는 동결 JS VM adapter와
Rust `GameAdapterSession`의 같은 첫 국면을 비교한다. 실행 절차와 자원·취소
경계는 [측정 도구 안내](../projects/augment-chess/perf/README.md)에 둔다.
새 게임, 준비한 국면의 새 세션 관측, 기존 세션 관측, 전체 legal, 이력 포함
apply를 분리한다. Rust 전이는 실제 `public-actions` capability를 호출하고
매 apply 전에 같은 immutable snapshot의 세션을 타이머 밖에서 준비한다.
새 게임 전체 Position, 양측 공개 관측, 순서 있는 legal payload, 다음
Position identity·상태·RNG·이력이 해당 단계에서 모두 일치해야 비율을 기록한다.
미지원은 실제 kind/code/reason과 함께 `partial-parity-no-go`, 종료 코드 1로
반환한다. 측정 도구 자체의 구조 검사를 native 실행 성공으로 취급하지 않는다.

원본 envelope 깊은 복사, JCS, 이미 정규화한 byte의 SHA-256, JSON
encode/decode를 별도 진단한다. Rust host handle의 Arc 복사와 transaction의
깊은 상태 복사, host import/export, registry 세션 구성도 각각 기록한다.
이 비용은 주 호출에 겹쳐 있으므로 합산해 전체 시간을 계산하지 않는다.
JSON 변환과 wire 전송은 별개다. 선택적인 allocator 진단은 요청 횟수·byte를
기록하지만 live memory·RSS·누수를 판정하지 않고 계측 결과의 속도 비율을 비운다.

`newGameFreshSession`은 준비한 source에서 새 adapter·VM·게임을 만든다.
`observeWhiteFreshSession`에는 세션 구성·입력 검증·복원이 들어가고
`observeWhiteWarmSession`은 객체를 재사용하면서도 입력 검증·복원·projection을
수행한다. source 검증·파싱·컴파일, 실제 wire·IPC, 자식 프로세스 시작과
입력 역직렬화는 native와 JS의 주 비교 구간에서 제외한다. 원문 VM의
`restore`·`snapshot`과 cold preparation은 기존 JS 도구의 별도 진단이다.
양측 관측 전체는 사전 대조하지만 관측 시간 표본은 white 응답이다.

빌드 바이너리에 engine·runtime·contracts·harness 소스 지문과 compiler·target·
profile·flags digest·feature를 포함한다. 현재 소스와 다르거나 측정 도중 입력이
바뀌면 오류를 반환한다. 결과의 재사용 지문에는 schema·catalog·loader·parser,
fixture의 국면·행동·순서 있는 legal digest, 환경·자원 한도, 바이너리도 포함한다.
Git SHA는 실행 출처로 보존하고 입력 지문에서 제외한다. 주요 입력이 그대로인
다른 SHA에서는 같은 범위에서 성공한 영수증을 재사용할 수 있다. 원래 실행과
재사용 판정은 구분하고, 실패·partial·계측 자료는 새 gate의 성공으로 쓰지 않는다.
첫 국면의 일치는 카드 전 범위·조합·후속 턴·설치 wheel·전체 v7 GO와 별도다.

재사용은 빌드와 원문 기대값, native 검사 성공을 따로 판정한다. engine만
변경했다면 client·parser·oracle 전체 의존성·initializer/profile manifest·
bootstrap·catalog·schema와 config·seed·tape·입력 Position·action·callback
경계가 그대로인 원문 영수증은 재사용할 수 있다. 변경한 native 결과는
해당 기대값과 새로 대조해야 한다. 시간이나 상태에 영향을 주는 동결 manifest의
시각도 입력에 포함한다. 바이너리는 compiler·target·profile·flags·feature·
소스/lock 의존성·binary digest가 같아야 재사용한다. 검사 성공은 그 검사가
실제 읽는 입력과 범위가 같을 때만 재사용하고, 성능 자료에는 같은 환경·자원
한도도 필요하다. 각 자료의 원래 SHA·지문·범위와 현재 대조 결과를 기록한다.
하네스는 기존 영수증으로 계산을 건너뛰는 기능을 제공하지 않으며 실제 실행은
`reused: false`다. 의존성이 그대로인 문서 커밋은 성공한 자료를 재사용할 수
있지만 실패·partial·allocator 자료와 이전 profile은 새 gate의 성공이 아니다.

아래 2026-09-28 수치는 당시 loader/profile의 관측 기록이다. reviewed initializer와
공개 capability의 현행 소스 지문을 대조한 새 Rust 측정은 통합 담당자가 실행해야
하며, 아래 수치를 현행 전체 이식의 성능 검증으로 재사용하지 않는다.

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
