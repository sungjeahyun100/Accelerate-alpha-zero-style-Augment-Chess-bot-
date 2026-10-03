# 어댑터·전이 성능 측정

동결된 v7 클라이언트와 Rust 게임 어댑터의 시작·공개 관측·첫 플레이
`legal_actions`·`apply` 경로를 유한한 반복 횟수로 측정한다. 정확한 의미가
대조되지 않은 단계의 결과는 속도 비교로 승격하지 않는다. 이 도구는 성능
조사용이며 CI의 통과 수치 기준은 아니다.

`measure.cjs`는 공식 클라이언트 SHA-256을 검증한 뒤 normal·chaos·grand 각각의
seed 37, `draftDelete: true` 첫 플레이 국면과 첫 합법 행동을 메모리에서 만든다.
JS 쪽은 새 세션 생성·첫 게임, 이미 만든 국면을 사용하는 새 세션 공개 관측,
기존 세션 공개 관측, `GameAdapter.actions`, 이력이 포함된
`GameAdapter.apply`를 단계별로 측정한다. 선택적으로 전달한 Rust 바이너리는
동일한 단계의 호스트 내부 호출을 측정한다. 외부 source 로딩, 프로세스 시작,
stdin/stdout 전송·사전 입력 국면 역직렬화는 주 측정 구간에서 제외한다. 새 세션 관측에는
세션 구성 비용이 포함된다. 시작·관측의 응답 전체가 일치한 단계에만
`observedP50Ratio`와 `observedP95Ratio`가 보고된다. 전이 단계는 **순서 있는
전체 행동 payload, 다음 Position의 identity·상태·RNG·이력**이 모두 일치해야 비율이 보고된다.
틀리면 필드와 실제 오류를 기록하고 그 단계의 비율은 비워 둔다. 일부 단계가
`unsupported`이면 그 단계를 건너뛰되 이미 일치한 다른 단계의 측정은 보존하고
전체 판정은 `partial-parity-no-go`로 두고 종료 코드 1과 실제 오류 원인을 반환한다.
국소 성공은 해당 단계의 참고 자료로 남는다.

현행 계약은 `accelerate-headless-semantic-v7-faithful-init-v1`이다.
`contracts/catalog/execution-profile-20260928.json`의 원문 선언·binding,
채택 initializer 175개·제외 initializer 168개, 원래 실행 순서,
bootstrap·replay metadata digest를 검증한다. 해당 manifest의 digest는
`catalogVersion`에도 묶인다. 같은 client SHA라도 이전
`accelerate-headless-semantic-v7` profile의 결과를 현행 계약의 성공으로
재사용하지 않는다. headless presentation 경계는 manifest에 고정되며,
브라우저 DOM이 있는 실행의 미래 RNG 일치까지 주장하지 않는다.

Rust 호출은 `GameAdapterSession`의 `public-observation/observe`와
`public-actions/legal-actions`·`bind-public-intent`·`apply-public-intent`를 사용한다.
adapter·capability ID와 descriptor의 schema·version을 정확히 선택한다.
공개 capability의 v7 play 지원이 닫혀 있으면 `transition.parity: unsupported`,
오류 kind/code/reason, `decision: NO-GO`를 반환한다. 현재 단계별 구현을
하네스가 임의로 승격하지 않는다. mutable apply는 매 호출 전에 같은 원본
snapshot으로 새 세션을 준비하고 그 준비와 세션 종료를 타이머 밖에 둔다.
선택·admission·transaction·응답 생성은 apply 타이머 안에 있다.

`boundaryDiagnostics`에는 source envelope의 깊은 복사, JCS 직렬화, 준비한
JCS byte의 SHA-256, JSON encode/decode를 별도로 기록한다. Rust에서는
Arc 기반 host handle 복사, transaction의 `GameState` 깊은 복사,
host import/export, 새 registry 세션 구성도 분리한다. 이 항목들은 실제
호출에 겹쳐 들어 있으므로 합산해 전이 시간을 예측하지 않는다. JSON 변환의
시간과 실제 wire 전송 시간을 구분하며, 진단 항목에는 언어 간 속도 비율을
붙이지 않는다. Node 메모리는 타이머 밖에서 읽은 process snapshot과 전체
최고 RSS만 기록한다. native 자식의 순간 peak를 이 값으로 주장하지 않는다.

| 확인할 비용 | 측정 구간과 판정 범위 |
|---|---|
| 첫 게임과 새 객체 | `newGameFreshSession`은 준비한 source를 이용한 새 JS adapter·VM·게임 생성과 새 native session을 측정한다. source 검증·파싱·컴파일은 제외한다. |
| 새 객체와 기존 객체의 관측 | `observeWhiteFreshSession`과 `observeWhiteWarmSession`을 나눈다. 기존 객체도 입력 Position의 검증·복원·공개 projection을 수행하며 결과 cache hit를 뜻하지 않는다. 시간 표본은 white 관측이고 사전 대조는 양측 응답 전체다. |
| 행동 조회와 전이 | `legalActions`는 전체 순서 있는 행동을 반환한다. `applyWithHistory`는 첫 합법 행동과 이력·공개 응답을 포함한다. 후보 생성·admission을 생략하지 않는다. |
| 객체 복사와 상태 복사 | source envelope 복사, native Arc handle 복사, transaction 상태 깊은 복사, host import/export, 세션 구성은 `boundaryDiagnostics`의 독립 진단값이다. |
| 정규화와 전송용 변환 | JCS, 준비한 canonical byte의 digest, JSON encode/decode를 나눈다. 실제 wire·IPC·자식 프로세스 시작 비용은 측정하지 않는다. |
| 원문 VM 복원과 준비 | 아래의 별도 JS 도구에서 `restore`, `snapshot`, `sourceVerifyParseCompile`, `createRuntime`, `adapterConstruction`을 측정한다. 두 도구의 서로 다른 GC·표본 방법을 전후 개선 비율로 섞지 않는다. |

Python·PyO3·설치 wheel·GPU·후속 턴·자가대국은 이 실행 범위에 없다.

`--features allocation-probe`로 빌드한 바이너리는 같은 경계의 allocator
요청 횟수·요청 byte를 별도 기록한다. 이는 live memory나 RSS가 아니며
결과의 drop 비용을 포함하지 않는다. 계측 allocator는 시간에도 영향을
주므로 이 모드의 모든 속도 비율은 비워 두고 `NO-GO`로 기록한다.

Rust 바이너리는 별도의 가짜 읽기 전용 객체를 통한 공통 registry 호출과 동일한
산술 연산의 직접 호출도 측정한다. 이는 계약 계층 호출 비용의 국소 참고값이지
게임 객체의 실제 호출 비용이 아니다. 실제 게임 호출은 위 공개 capability
측정에 포함한다.
동결 원문 없이 registry 계측만 확인하려면 바이너리를 `--synthetic-only`로 실행한다.

통합 담당자가 빌드·테스트·측정을 순차 실행한다. worker는 1개, GPU는
사용하지 않는다. 각 작업자가 별도 Cargo target을 누적하지 않고 OS별 공유
슬롯을 사용한다. 관련 소스의 통합과 차분 검증이 끝난 checkpoint에서
아래 작은 검사와 빌드를 먼저 실행한다. 이미 지정한 같은 OS의 공유 target은
유지하며, 별도 target을 만들지 않는다. Windows PowerShell의 예시는 다음과 같다.

```powershell
$env:CARGO_BUILD_JOBS = '1'
if (-not $env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR = Join-Path $env:APPDATA 'Accelerate\build\shared-windows' }
$perfSource = Join-Path $env:APPDATA 'Accelerate\cache\site-baseline-20260928-e5ed84fc'
$perfReports = Join-Path $env:APPDATA 'Accelerate\reports\adapter-transition-perf'
node --test projects/augment-chess/perf/measure.test.cjs
if ($LASTEXITCODE -ne 0) { throw '성능 하네스 검사가 실패했습니다. 반환된 오류를 확인하세요.' }
cargo build --release --offline --manifest-path projects/augment-chess/perf/native/Cargo.toml
if ($LASTEXITCODE -ne 0) { throw '성능 바이너리 빌드가 실패했습니다. 반환된 오류를 확인하세요.' }
$perfBinary = Join-Path $env:CARGO_TARGET_DIR 'release\augment-chess-perf.exe'
node --max-old-space-size=4096 projects/augment-chess/perf/measure.cjs --source-root $perfSource --native-bin $perfBinary --style normal --samples 1 --warmups 0 --iterations 1 --timeout-ms 60000 --output (Join-Path $perfReports 'smoke.json')
```

WSL에서는 Windows의 APPDATA를 확인해 `wslpath`로 변환한 외부 생성물 루트를
`ACCELERATE_ARTIFACT_ROOT`에 전달한다. 변환에 실패하면 진행하지 않는다.
동일 OS의 공유 target에서 빌드하며 아래 명령은 checkout의 최상위에서 실행한다.

```sh
export CARGO_BUILD_JOBS=1
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/accelerate/build/root-adapter}"
node --test projects/augment-chess/perf/measure.test.cjs || exit "$?"
cargo build --release --offline --manifest-path projects/augment-chess/perf/native/Cargo.toml || exit "$?"
timeout --signal=TERM --kill-after=5s 70s node --max-old-space-size=4096 projects/augment-chess/perf/measure.cjs --native-bin "$CARGO_TARGET_DIR/release/augment-chess-perf" --source-root "$ACCELERATE_ARTIFACT_ROOT/cache/site-baseline-20260928-e5ed84fc" --style normal --samples 1 --warmups 0 --iterations 1 --timeout-ms 60000 --output "$ACCELERATE_ARTIFACT_ROOT/reports/adapter-transition-perf/smoke.json"
```

작은 실행은 1개 표본으로 입력·반환 계약을 확인하며 p95 안정성의 근거가
아니다. 종료 코드와 외부 `smoke.json`을 함께 확인한다. `status`가
`bounded-parity-and-timings`, `decision`이 `bounded-case-pass-only`이고
native의 측정 단계가 모두 일치할 때만 아래 세 모드 측정으로 진행한다.
`reference-only`와 `allocation-diagnostic-only`는 종료 코드가 0이어도
native 성능 검증의 성공이 아니다. partial·mismatch·measurement-error는
정확한 오류와 미지원 경계를 확인한 뒤 멈춘다. 입력·시간 한도·golden을
자동 변경해 통과시키지 않는다.

```powershell
node --max-old-space-size=4096 projects/augment-chess/perf/measure.cjs --source-root $perfSource --native-bin $perfBinary --style all --samples 7 --warmups 2 --iterations 1 --timeout-ms 120000 --output (Join-Path $perfReports 'report.json')
```

```sh
timeout --signal=TERM --kill-after=5s 130s node --max-old-space-size=4096 projects/augment-chess/perf/measure.cjs --native-bin "$CARGO_TARGET_DIR/release/augment-chess-perf" --source-root "$ACCELERATE_ARTIFACT_ROOT/cache/site-baseline-20260928-e5ed84fc" --style all --samples 7 --warmups 2 --iterations 1 --timeout-ms 120000 --output "$ACCELERATE_ARTIFACT_ROOT/reports/adapter-transition-perf/report.json"
```

두 실행은 같은 seed 37·`draftDelete: true`와 기본 rule card 설정을 사용한다.
전체 실행도 세 모드의 첫 일반 이동 범위다. 카드·규칙·관측 continuation을
추가한 사례의 지원은 별도의 차분 검증으로 확인한다.

Node heap 한도는 4 GiB이며 실제 V8 heap 한도를 결과에 기록한다. native의
OS 메모리 경계는 실행 환경에서 설정하고 OOM을 숨기거나 횟수를 자동으로
줄여 재시도하지 않는다. 기본 시간 예산은 120초다. Node는 동기 표본 사이에서
확인하고 native child에는 남은 시간의 timeout을 적용한다. 단일 Node 호출의
중단은 외부 `timeout` process group이나 기존 Job Object 경계가 맡는다.
수동 취소는 소유한 실행의 `Ctrl+C`이며 해당 자식까지 종료된 것을 확인한다.
빌드 프로세스 자체의 시간은 측정 예산에 포함되지 않는다.

원문 VM의 별도 경계를 조사할 때는 기존 JS 도구를 같은 checkpoint에서
유한한 표본으로 실행한다. 아래 WSL 예시는 세 모드·각 3개 표본,
source 생성 1개 표본과 도구의 기본 warmup 1회를 사용한다.
이 도구에는 자체 전체 시간 예산이 없으므로 외부 130초 경계로 중단한다.
Windows에서는 같은 옵션과 외부 reports 슬롯을 사용하고 소유한 실행의
기존 Job Object 또는 수동 취소 경계를 기록한다.

```sh
timeout --signal=TERM --kill-after=5s 130s node --expose-gc --max-old-space-size=4096 projects/augment-chess/tests/site-adapter/bench/performance.cjs --source-root "$ACCELERATE_ARTIFACT_ROOT/cache/site-baseline-20260928-e5ed84fc" --stage all --style all --samples 3 --cold-samples 1 --label current-vm-boundaries --output "$ACCELERATE_ARTIFACT_ROOT/reports/adapter-transition-perf/vm-boundaries.json"
```

`--stage source` 보고서는 rules/catalog/profile을 `null`로 둔다. 기존
비교기는 initializer manifest·`reviewed-initializers.js`의 변경을 별도
비교 키로 확인하지 않으므로 source-only의 자동 비교만으로 이전 profile과
현행 profile을 같은 실행으로 판단하지 않는다. 원문 준비 비용의 전후 비교는
실제 loader·initializer·bootstrap 의존성과 semantic profile의 변화까지
기록해야 한다. 이 JS 진단은 native 차분 검증이나 성능 판정을 대신하지 않는다.

원문 캐시는 기본적으로 `%APPDATA%/Accelerate/cache/site-baseline-20260928-e5ed84fc`
(CI는 `$RUNNER_TEMP/Accelerate/cache/site-baseline-20260928-e5ed84fc`)에서
읽는다. 다른 외부 슬롯이라면 `--source-root`에 절대 경로를 전달한다. 어느
경로든 동결된 클라이언트 SHA를 확인한다. `--native-bin`을 빼면 JS 기준값만
남기고 상태를 `reference-only`로 기록한다.
`--samples`(1..100, 기본 7), `--warmups`(0..20, 기본 2),
`--iterations`(1..100, 기본 1), `--timeout-ms`(1000..600000, 기본 120000),
`--style`과 `--output`으로 범위를 조정할 수 있다. 전체 외부 호출·진단 반복은
보수적인 50,000회 상한을 적용한다. 엔진 안의 후보 순회는 별도 계약 한도를 따른다.
native 요청은 4 MiB, 응답은 8 MiB 상한을 적용한다. source 후보의 기본
작업 한도는 100,000이며 microtask 기본 한도는 256이다.
기본 보고서는 `%APPDATA%/Accelerate/reports` 또는
`$RUNNER_TEMP/Accelerate/reports` 아래에 저장한다. Git 안의 보고서 경로는 거부한다.
보고서에는 Git HEAD, 엔진·공통 런타임·게임 계약·oracle 어댑터 소스 digest,
실행 환경, 국면·행동·순서 있는 legal payload digest, 표본과 분포, 의미 대조 결과가 들어간다. 원시
국면과 클라이언트 번들은 보관하지 않는다.

바이너리에는 빌드 당시 engine·runtime·contracts·native harness의 소스 지문과
compiler·target·profile·flags digest·feature가 포함된다. `--build-info`로
이를 읽고 현재 소스와 다르면 측정을 시작하기 전에 재빌드를 요구한다. 실행
도중 관련 소스가 바뀌어도 오류로 반환하고 비율을 무효화한다.
`reuseEvidence.inputFingerprintSha256`은 Git SHA를 제외한 실제 입력 지문이다.
다른 SHA에서도 engine·schema·catalog·loader·parser·harness·compiler·target·
profile·binary·fixture·환경·자원 한도가 같고 이전 결과가 같은 범위에서
성공했으면 그 영수증을 재사용할 근거가 된다. 원래 실행 SHA와 재사용 여부를
분리해 기록하며, 이 하네스는 현재 실행을 수행하고 `reused: false`로 기록한다.
partial·실패·allocator 계측 자료는 미검증 gate를 통과시키지 않는다.
하네스의 성공도 세 모드 첫 국면 범위이며 전체 v7 규칙을 입증하지 않는다.

재사용 여부는 자료가 검증한 책임별로 판정한다. 이 도구는 이전 영수증을
읽어 계산을 건너뛰는 cache를 구현하지 않는다. 아래 조건은 통합 담당자가
성공 기록과 입력 의존성을 확인할 때 적용한다.

| 자료 | 재사용 조건과 다시 확인할 경계 |
|---|---|
| 동결 client·parser 파일 | 파일 byte와 manifest의 checksum이 같으면 기존 외부 cache를 사용한다. 검증을 생략하지 않는다. |
| 준비한 immutable source 객체 | loader·initializer manifest·bootstrap·parser·client·semantic profile이 같은 프로세스에서 준비한 컴파일 객체를 재사용한다. mutable VM이나 이전 세션의 RNG·관측 상태를 새 reference로 재사용하지 않는다. |
| 성공한 원문 전이·관측 영수증 | client·parser·oracle loader와 adapter 전체 의존성, initializer/profile manifest·bootstrap·catalog·schema, 상태나 시간에 영향을 주는 동결 시각, config·seed·tape·입력 Position·action·callback 경계가 같아야 한다. engine만 변경했다면 원문 기대값은 재사용할 수 있어도 새 native 결과의 대조는 필요하다. |
| native 바이너리 | engine·공통 runtime·contracts·native harness와 manifest/lock 의존성, compiler·target·profile·flags·feature 및 binary digest가 일치해야 한다. 소스 지문이 달라진 기존 바이너리는 재빌드한다. |
| 성공한 검사와 성능 결과 | 그 검사가 실제 읽는 모든 입력과 동일 검사 범위가 같아야 한다. 성능 결과는 `inputFingerprintSha256`의 fixture·바이너리·환경·자원 조건도 같아야 한다. 문서 변경처럼 의존성이 그대로인 다른 Git SHA는 재사용할 수 있다. 실패·partial·allocator 결과는 성공한 검사의 대체 자료가 아니다. |

원문 자료와 native 자료를 따로 재사용한 경우 원래 성공의 SHA·입력 지문·
검사 범위와 현재 native 대조 결과를 함께 기록한다. 실행 중 소스가 변경되어
무효화된 결과, 다른 Node·OS·CPU·GC·worker·메모리 한도의 시간 수치는
같은 조건의 전후 성능 기준선으로 사용하지 않는다.

두 런타임은 언어와 호출 경계가 달라 관측된 비율만으로 엔진 hot path 전체의
속도 향상이나 제품 성능을 주장할 수 없다. 병렬 작업이 진행 중이면 CPU 경쟁을
기록하고, 같은 환경·fixture·소스 입력에서 다시 측정한다. 이 범위는 첫 행동의
국소 검증이며 카드 전 범위·후속 턴·자가대국 성능을 대변하지 않는다.
