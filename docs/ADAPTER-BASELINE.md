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
`accelerate-headless-semantic-v7-faithful-init-v1`, 공개 관측 projection은 `source-visible-20260928-v2`다.
이전 v6와 새 v7의 Position을 서로 받아들이지 않는다. 공개 카드 catalog hash
`yt63f_Lcq4xUPu4GbbdwAyv5IUTuC079nNxvF3IFrj4`는 동일하지만 원문 코드와
전이 의미가 달라졌으므로 catalog hash만으로 버전을 판정하지 않는다.

v2는 같은 동결 client의 `reversal` 공개 owner flag를 `statePublicFields`에 포함한다.
원문의 `applyCardEffect`는 아군 마이너 희생 뒤 `{white: false, black: false}`에서
현재 색을 `true`로 설정하고, `getLegalMoves`는 비숍·룩의 ray 방향을 이 값으로
교환한다. `completeTurnAfterMove`는 이동한 색을 `false`로 초기화한다. 원문의
`aiWorkerStateSnapshot`과 `applyBetaFriendlyCardAuthority`도 이 값을 `clonePlain`으로
보존한다. 기물 ID·좌표·미래 RNG를 포함하지 않는 색별 공개 활성 효과이며 정책은
white/black boolean만 허용하고 추가 nested key를 거절한다. v1 당시 검증을 이 입력
계약의 성공으로 승계하지 않는다. 최초 CI 실패와 v2 변경 후 재검증의 실제 관측 범위는
[v7 인수 장부](V7-ACCEPTANCE-CLOSURE.md) 및 PR #32 본문을 따른다.

v2·PR #36 로컬 통합 뒤 Node 6개 파일·63개 검사와 8/8 카드 표면 shard의
256개 카드×3스타일 768개 bounded cell이 PASS였다. weighted source 재생성은
동결 원문의 원래 helper를 그대로 복사해 실행하고 `publicAfter` 외 raw 필드가
완전히 같음을 guard로 확인했다. 새 weighted 입력과 내부21 입력의 SHA·필터 결과는
인수 장부에 결속하며 이전 v1 자료도 보존한다. 공유 JCS 비교기는 실제 차이를 엄격히
검사한다. 국소 결과와 별도로 v2·PR #36 통합 후 core와 새 full105 실행을 완료했고
105/105 PASS·실패0·컴파일 경고0·모든 job source digest 전후 동일을 관측했다. 계획·요약의
SHA와 보존한 최초 실패 기록은 인수 장부를 따른다. 같은 head의 원격 CI·병합 결과는
PR #32 본문의 실제 관측에 결속하며 로컬 성공으로 원격 결과를 대신하지 않는다.

현재 `execution-profile-20260928.json`은 최상위 initializer 175개와 제외 168개,
원문 순서·dependency/bootstrap digest와 replay labels·codes 각 79개/frameKeys 222개를
고정한다. manifest SHA-256은
`d811f0232ac38af4e45e0e4f93e89c49712142dfd0f2b5fe57d36f63cd05a29f`이고,
현재 composite `catalogVersion`은
`f80ebcd21759df179bccfb301415e672194de67691a6383beafc549538ffae7c`다.
공개 source catalog hash와 이 composite identity는 별도 값이다. 같은 원문 bytes라도
선언 전용·23-initializer/labels 30 프로필의 영수증을 faithful175 성공으로 옮기지 않는다.
최신 생성·native 비교·남은 경계는 [v7 인수 장부](V7-ACCEPTANCE-CLOSURE.md)에 둔다.

## 공식 설명과 source 대조

[공식 규칙](https://augmentchess.org/rules/)에 따르면 킹 포획과 행동 불능,
3수 동형 및 연장전의 별 비교가 종료에 영향을 준다. 일반·카오스는 시작·10수·20수에
드래프트하고, 그랜드는 시작 시 공용 카드 풀에서 덱을 구성한다. 패시브·액티브·OPENING
카드의 적용 시점이 다르며, 새 기물의 같은 턴 포획은 제한된다. 이는 검증 범위의
안내이며 실제 어댑터 전이는 위 고정 client가 기준이다.

이전 초기화 범위에서 공개 카드 metadata 256개와 semantic `CARD_DEFS` 257개의 필드는
이전 기준과 같다는 비교가 나왔다. 이는 초기화 누락을 고치기 전의 조사 기록이며,
현재 faithful175의 phase·stars·weight·openingWeight 값 검증을 대신하지 않는다.
이 중 `shotgun-king` 보조 정의가
공개 카드 수와 semantic 정의 수의 차이를 만든다. 기존 draft 상수·가중치는 새
source와 대조했고, 배제 조합에는 `summon-colossus` 관련 다섯 조합이 추가됐다:
`false-start`, `london-system`, `chess-344200`, `chess-n-pow-30`,
`chess-45-pow-30`과 각각 함께 나올 수 없다. 기존 일반 3장, 카오스 3묶음×2장,
그랜드 공용 28장·한쪽 덱 6장이라는 핵심 수량은 그대로다.

당시 동일한 고정 시계·난수·headless 초기화 조건에서 이전/새 client의 normal
초기 raw state 259개 필드를 비교했을 때 차이가 없었다. 초기 메타데이터는 새
rulesVersion으로 별도 보존했다. 당시 v7 어댑터로 seed 11의 세 모드를 실제 시작하면
normal은 3개 선택지와 RNG cursor 122, chaos는 6개 선택지와 cursor 212,
grand는 28개 선택지와 cursor 112를 반환했다. 같은 seed의 v6/v7 시작 상태를
비교하면 각 모드에서 draft clock 시작 시각과 replay 시작 시각 네 경로만 달랐다.
두 동결본의 `frozenAt`이 서로 다른 데 따른 차이이며 RNG와 초기 history는 같았다.
세 모드의 이 시작 Position에서 양측 `observe`도 v7 policy hash로 검증을 통과했다.
이 한 seed의 초깃값 확인은 드래프트 완료 상태나 모든 초기 RNG 소비를 검증한
결과가 아니다.

23-initializer 로더로 재조사했던 seed 19에서도 grand의 후보 28개·RNG cursor
112, chaos의 후보 6개·cursor 212라는 수량은 유지된다. 그러나 grand는
21번째 후보부터 일부 ID가 달라지고, chaos도 4·6번째 후보 ID가 달라진다.
더 이른 로더에서 얻은 후보 ID·그에 따른 선택 경로와 이 재조사 결과는 각각 당시
프로필의 근거로 남긴다. 현재 faithful175에서는 새 identity로 자료를 생성·대조한다.

client 선언 단위 비교에서 다음 행동 의미 차이를 확인했다.

| 영역 | 현재 client의 차이 | 확인할 전이 |
|---|---|---|
| 드래프트 | colossus 배제 조합과 RULE별 대형 OPENING 카드 제외 조건 추가 | 세 모드 카드 제시·선택·거부 |
| 돈 키호테 | 나이트 이동이 `knightDeltasForMove`를 따르고 자동 경로가 왕 포획 회피 경로를 먼저 시도 | 이동·연속 전이·왕 충돌·종료 |
| 시지램·카멜레온 | 경로 중 첫 적격 포획이 카멜레온 변형 근거가 될 수 있음 | 경로 포획·변형·후속 행동 |
| 도둑·트릭스터 | 방문·방향 상태 정리 범위와 wanted 기본값 변경 | 턴 경계·상태 복원·관측 |
| 기보·표시 | medium 기보 코드, 카멜레온 눈 표시, 돈 키호테 animation 처리 변경 | 공개 기록·표시 분리 |

`projects/augment-chess/contracts/catalog/*-20260928.json`과 `projects/augment-chess/contracts/schemas/runtime-site-20260928.schema.json`은
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
node projects/augment-chess/oracle/tools/site-parity/prepare-current-baseline.js --verify <외부-동결-디렉터리>
node --test projects/augment-chess/contracts/tools/runtime-contract.test.js
```
