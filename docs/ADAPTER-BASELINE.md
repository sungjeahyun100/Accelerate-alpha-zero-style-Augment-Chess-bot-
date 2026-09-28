# 게임 어댑터의 공식 client 기준

## 동결한 원문과 버전

2026-09-28T07:41:32.828Z에 공식 사이트의 client를 별도 외부 캐시에 고정했다. 이후
사이트가 다시 바뀌어도 이 기준을 자동으로 갱신하지 않는다. 저장소에는 원문 JavaScript,
parser, raw 로그를 넣지 않는다.

| 파일 | 공식 URL | SHA-256 |
|---|---|---|
| 진입 HTML | https://augmentchess.org/ | `ef7575e12a2e3dcf16744acde8f2f2b2f8744a41bc0cf9bf3d3a075e20fe3f7b` |
| 게임 client | https://augmentchess.org/assets/main-OahWs0tU.js | `e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c` |
| AI worker | https://augmentchess.org/assets/aiWorker.js | `4ef7f4647a707fbbda10cefeab6cce37f16546e938b4a7090e76913e0241477a` |
| Acorn parser 8.15.0 | https://unpkg.com/acorn@8.15.0/dist/acorn.js | `fdb08546776ec6228b03e8d02b40d4ab3255bae5f401adba7ff5dad927ac5c9c` |

Windows의 외부 캐시 슬롯은 `%APPDATA%\Accelerate\cache\site-baseline-20260928-e5ed84fc`다.
CI에서는 `$RUNNER_TEMP/Accelerate/cache/site-baseline-20260928-e5ed84fc`를 사용한다.
manifest와 원문 SHA를 검사한 뒤에만 실행한다. 이전 `site-baseline` 슬롯을 덮어쓰지 않는다.
client 실행은 main과 parser에 의존하며 worker는 이번 어댑터 실행의 일부가 아니다.

규칙 버전은 `augment-site-20260928-e5ed84fcf8e72a24`, 실행 profile은
`accelerate-headless-semantic-v7`, 공개 관측 projection은 `source-visible-20260928-v1`이다.
이전 v6와 새 v7의 Position을 서로 받아들이지 않는다. 공개 카드 catalog hash
`yt63f_Lcq4xUPu4GbbdwAyv5IUTuC079nNxvF3IFrj4`는 동일하지만 원문 코드와
전이 의미가 달라졌으므로 catalog hash만으로 버전을 판정하지 않는다.

## 공식 설명과 source 대조

[공식 규칙](https://augmentchess.org/rules/)에 따르면 킹 포획과 행동 불능,
3수 동형 및 연장전의 별 비교가 종료에 영향을 준다. 일반·카오스는 시작·10수·20수에
드래프트하고, 그랜드는 시작 시 공용 카드 풀에서 덱을 구성한다. 패시브·액티브·OPENING
카드의 적용 시점이 다르며, 새 기물의 같은 턴 포획은 제한된다. 이는 검증 범위의
안내이며 실제 어댑터 전이는 위 고정 client가 기준이다.

고정 원문에서 공개 카드 metadata 256개와 semantic `CARD_DEFS` 257개의 필드는
이전 기준과 모두 같음을 기계적으로 비교했다. 이 중 `shotgun-king` 보조 정의가
공개 카드 수와 semantic 정의 수의 차이를 만든다. 기존 draft 상수·가중치는 새
source와 대조했고, 배제 조합에는 `summon-colossus` 관련 다섯 조합이 추가됐다:
`false-start`, `london-system`, `chess-344200`, `chess-n-pow-30`,
`chess-45-pow-30`과 각각 함께 나올 수 없다. 기존 일반 3장, 카오스 3묶음×2장,
그랜드 공용 28장·한쪽 덱 6장이라는 핵심 수량은 그대로다.

동일한 고정 시계·난수·headless 초기화 조건에서 이전/현재 client의 normal
초기 raw state 259개 필드를 비교했을 때 차이가 없었다. 초기 메타데이터는 새
rulesVersion으로 별도 보존한다. 새 v7 어댑터로 seed 11의 세 모드를 실제 시작하면
normal은 3개 선택지와 RNG cursor 122, chaos는 6개 선택지와 cursor 212,
grand는 28개 선택지와 cursor 112를 반환했다. 같은 seed의 v6/v7 시작 상태를
비교하면 각 모드에서 draft clock 시작 시각과 replay 시작 시각 네 경로만 달랐다.
두 동결본의 `frozenAt`이 서로 다른 데 따른 차이이며 RNG와 초기 history는 같았다.
세 모드의 이 시작 Position에서 양측 `observe`도 v7 policy hash로 검증을 통과했다.
이 한 seed의 초깃값 확인은 드래프트 완료 상태나 모든 초기 RNG 소비를 검증한
결과가 아니다.

client 선언 단위 비교에서 다음 행동 의미 차이를 확인했다.

| 영역 | 현재 client의 차이 | 확인할 전이 |
|---|---|---|
| 드래프트 | colossus 배제 조합과 RULE별 대형 OPENING 카드 제외 조건 추가 | 세 모드 카드 제시·선택·거부 |
| 돈 키호테 | 나이트 이동이 `knightDeltasForMove`를 따르고 자동 경로가 왕 포획 회피 경로를 먼저 시도 | 이동·연속 전이·왕 충돌·종료 |
| 시지램·카멜레온 | 경로 중 첫 적격 포획이 카멜레온 변형 근거가 될 수 있음 | 경로 포획·변형·후속 행동 |
| 도둑·트릭스터 | 방문·방향 상태 정리 범위와 wanted 기본값 변경 | 턴 경계·상태 복원·관측 |
| 기보·표시 | medium 기보 코드, 카멜레온 눈 표시, 돈 키호테 animation 처리 변경 | 공개 기록·표시 분리 |

`bridge/catalog/*-20260928.json`과 `bridge/schemas/runtime-site-20260928.schema.json`은
새 버전 묶음이다. 명시적으로
`createRuntimeContract({ baseline: "site-20260928" })`를 호출해 사용한다. 기존
기본 export는 이전 기준을 유지한다. schema도 각 Position rulesVersion과 관측
projectionVersion을 정확히 하나로 고정한다. 다른 버전의 관측·행동·Position을
새 기준으로 자동 변환하지 않는다.

공개 관측 정책의 필드와 값 schema는 이전 검토 결과를 **임시 계승**하되, 최신
원문에서 확인한 `othelloPending`은 턴 종료 재검사용 내부 latch로 별도 분류했다.
원문은 이를 화면에 직접 표시하지 않으며 변환된 보드와 사용한 카드는 기존
공개 관측에 남는다. 위 source의 다른 행동·표시 변경이 동적 관측에 미치는
영향은 계속 실행 비교가 필요하다. 현 정책의
`coverage.status`는 `partial-source-projection`이며, 이 문서와 metadata만으로
전체 규칙 지원이나 결과 parity를 선언하지 않는다. adapter parity 검사는
normal·chaos·grand의 드래프트/행동/관측/종료와 실제 RNG·history를 고정 source에
비교하고, 확인되지 않은 기능을 미검증으로 남긴다.

동결 원문이 준비된 환경에서는 다음 읽기 전용 검증으로 새 JSON·schema가 실제
원문과 일치하는지 확인한다.

```text
node infra/tools/site-parity/prepare-current-baseline.js --verify <외부-동결-디렉터리>
node --test bridge/tools/runtime-contract.test.js
```
