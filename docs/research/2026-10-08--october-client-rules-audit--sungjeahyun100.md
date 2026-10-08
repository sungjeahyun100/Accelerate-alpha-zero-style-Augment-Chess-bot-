# 10월 7일 클라이언트 규칙 차이 조사 (진행 중)

## 식별과 출처

| 항목 | 기록 |
| --- | --- |
| 작성 시점 | 2026-10-08 04:26 UTC |
| 마지막 정정 시점 | 해당 없음: 최초 조사 |
| GitHub 작성자·공동 작성자 | [sungjeahyun100](https://github.com/sungjeahyun100); 공동 작성자 없음 |
| 관련 PR·이슈·후속 기록 | [PR #44](https://github.com/sungjeahyun100/Accelerate-alpha-zero-style-Augment-Chess-bot-/pull/44): 구현·검증 진행 중 |
| 저장소·기준 commit SHA | `sungjeahyun100/Accelerate-alpha-zero-style-Augment-Chess-bot-`, `3552b96fb276dd59e805bc09cf80c975a4525474` (`origin/develop`) |
| 미커밋 변경 | `projects/augment-chess/reference/infra/main-Dm4wrmOx.js`는 별도 checkout의 입력 원본; 이 문서 초안만 조사 worktree에 존재 |
| 자료 유형 | 규칙 비교·실패 범위 분석 |
| 관측 근거의 범위 | 로컬 소스 조사와 선언 실행; CI·Rust 전이·모델 성능 미측정 |

## 목적과 범위

- 연구 질문: 최신 본체와 9월 28일 동결 규칙 사이에 실제 상태 전이 차이가 있는가?
- 비교 기준: 두 원본 SHA, 선언 AST, 공개 카드 정의 및 별도 규칙 게이트.
- 판단 기준: UI 차이를 제외한 카드·이동·턴·확률·관측 변화의 근거를 분리한다.
- 범위: 카드 표면과 대표적인 10월 규칙 경로의 1차 조사. 전체 전이 차분은 미실행.

## 재현 설정

| 항목 | 기록 |
| --- | --- |
| 입력·fixture | `main-OahWs0tU.js` 공개 사이트 자산과 `projects/augment-chess/reference/infra/main-Dm4wrmOx.js`; SHA는 아래 기록 |
| 모델·어댑터·규칙·catalog | 모델 없음; 기존 `execution-profile-20260928.json`; 새 본체 hash는 아래 표 |
| seed·옵션 | 선언 AST 파싱·카드 필드 비교에는 RNG seed 없음. 대국 비교 미실행 |
| OS·도구 | Linux, Node.js와 시스템 Acorn AST 파서; 실행 스레드 1개 |
| CPU·GPU·메모리 | 소스 파싱만 하여 성능 해석에 사용하지 않음 |
| 작업 한도·종료 | 유한한 두 번들만 파싱, 각 VM 실행 30초 제한, 자식 프로세스 종료 |

작업 디렉터리는 저장소 루트였다. 기존 자산은 `https://augmentchess.org/assets/main-OahWs0tU.js`에서 취득하고 SHA를 확인했다. 원본을 저장소에 복사하지 않았다. 이어서 두 원본을 Acorn `sourceType:module`로 파싱하고 top-level 선언만 브라우저 API가 비활성화된 Node VM에서 실행해 `CARDS$1`의 ID·필드를 비교했다. 실제 대국 초기화 문장은 실행하지 않았다. 조사 스크립트와 원시 출력은 임시 공간에만 두었다.

## 질문과 출처

9월 28일 동결 본체 `main-OahWs0tU.js`(SHA-256 `e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c`)와 새로 받은 `projects/augment-chess/reference/infra/main-Dm4wrmOx.js`(SHA-256 `958e8e6787d8d107152e4c07e45736d2ffbf05ad8fad4e7de63558c3c70d024c`)를 비교했다. 이전 번들은 공개 사이트 자산에서 다시 받아 SHA를 확인했다. 새 번들은 로컬 미추적 원본이며 Git에 포함하지 않았다. 원본 코드나 원시 로그는 이 문서에 복사하지 않는다.

기존 Rust와 JS 오라클은 `projects/augment-chess/contracts/catalog/execution-profile-20260928.json`의 이전 SHA와 초기화 경계에 묶여 있다. 새 소스를 기존 profile로 실행하거나 이전 `rulesVersion`으로 표시하는 것은 잘못된 규칙 증거가 된다.

## 확인한 차이

아래의 `구현`은 현재 저장소 기준이다. 새 본체의 선언만 VM에서 실행해 카드 표면을 비교했으며, 새 본체의 실제 대국 전이를 검증한 것은 아니다.

| 분류 | 항목 | 기존 동작 | 새 `main.js` 동작 | 대응·근거 |
| --- | --- | --- | --- | --- |
| A | 공개 카탈로그 | `yt63f_Lcq4xUPu4GbbdwAyv5IUTuC079nNxvF3IFrj4` | 10월 7일 hash `disfTpO_11gGrXKr6Q_AO_SsQHVecw5XIXQ0mJExQ4k`; 중간 5개 hash도 별도 게이트 | 신규 ruleset이 필요하다. 새 본체 692~762행 `usesOctober7Balance` 등. 기존 profile을 재사용하지 않는다. |
| A | 카드 수·ID | 공개 카드 256개 | 공개 카드 256개, ID 추가·제거 없음 | 유지. 새 본체 `CARDS$1` 43747행, 선언 실행에서 양쪽 ID 집합 비교. 새 카드가 없다는 사실은 효과가 같다는 뜻이 아니다. |
| A | 드래프트 가중치 | 점수별 1.125~0.5, OPENING 배율 | 희귀도 common 85, rare 70, epic 58, legendary 44를 8로 나눈 값. OPENING 배율을 사용하지 않음 | 변경 필요. 새 본체 295~308, 143554~143559행. `draft.rs` 149~255, 310~346행의 이전 가중치 표와 조건화 확률도 함께 갱신해야 한다. |
| A | `completeRandom` 드래프트 | 셔플 후 배타성에 맞는 첫 카드 선택 | 가중치로 반복 추첨하고 선택 카드를 풀에서 제거한 뒤 배타성 검사 | 변경 필요. 이전 본체 `drawCompatibleDraftCards` 67490행, 새 본체 143511~143524행. 결과를 단순 균등분포로 계산하면 안 된다. |
| A | 카드 표면 변경 | 기존 9월 28일 값 | `chimera`의 대상이 나이트/비숍에서 퀸으로, 별점 6/2에서 8/2로 변경; `overwhelm` 9/2→6/2, `switcheroo` 3/2→4/2 | 변경 필요. 새 본체 728~744, 43747~43769행. 설명만 바뀐 항목과 실제 대상·확률 변경을 구분한다. |
| A | `holdout` | 14수 후 승격 | 28수 후 승격 | 변경 필요. 새 본체 713행 `holdoutPromotionTurns`, 170753행 `resolveHoldoutPromotions`. |
| A | `monster` | 흑 차례마다 이동 | 완료한 반수가 3의 배수일 때 이동 | 변경 필요. 새 본체 725행 `monsterMoveDue`; 실제 호출 경로도 검증 필요. |
| A | `switcheroo` | 왕이 아군 폰 위치로 이동하고 폰 제거 | 왕과 폰 위치 교환 | 변경 필요. 새 본체 716행 `usesSwitcherooSwap`, 183267행 카드 결과, 이동 경로 확인 필요. |
| A | `reaper` | 왕 인접 행마와 기존 영혼 조건 | 왕·알필 행마, 아군 포획 2회 승리 조건 | 변경 필요. 새 본체 21963, 68071, 71475, 185775행. 사망 원인·주체 필터를 포함해 비교 필요. |
| A | `chimera` | 나이트/비숍에 부여, 결정론적 변신 | 퀸에 부여, 이동할 때 무작위 메이저 기물 변신 | 변경 필요. 새 본체 728행, 12448, 72992~73002행. 결과 집합과 가중치를 원문 함수로 확인해야 한다. |
| A | `othello` | 사용 즉시와 턴 종료 판정 | 턴 종료 시 판정 | 변경 필요. 새 본체 19621, 59434행 및 클라이언트 `othello` 실행 경로. |
| A | `high-ground`, `crown`, `scarecrow`, `frontline-response`, `d4`, `overwhelm`, `trolley`, `revolving-door` | 기존 9월 규칙 | 각각 지형·점유·예약 보호·아군 진영 제한·왕 제한·포획 금지 대상·점수 범위·회전문 포획 경계 변경을 표시 | 설명 텍스트만으로 동작을 확정하지 말고 새 본체의 실제 apply/합법 행동에서 대조해야 한다. 728~744행은 조사 출발점이다. |
| B | 숨은 기물 대상 | 기존 공개 관측 allowlist | 새 규칙에서 관측자별 stealth 상호작용과 숨은 대상 거부 경로가 추가됨 | 공개/비공개 경계 변경 가능. 새 본체 56514, 60122, 78446, 78622행. 아직 관측 차분 미검증. |
| C | 카드 희귀도 장식·드래프트 연출 | 화면 전용 | `cardRarityEnamelHtml`, `DRAFT_REVEAL_*` 등 | 시각 요소는 포팅 대상에서 제외. 새 본체 309~322행; 확률에 쓰이는 `cardRarityWeightUnits`만 규칙 대상. |
| C | DOM·오디오·계정·로그 | 봇 상태 전이에 불필요 | 새 번들에 다수 추가 | 순수 엔진에 제외. 단, 이 함수가 규칙 함수를 호출하는 경로는 별도 확인한다. |
| D | 구 버전 호환 코드 | 동결 9월 분기 | 새 번들에 이전 catalog hash와 legacy 경로가 공존 | 새 ruleset 구현에 복사하지 않고, 구 버전 검증은 기존 profile에 남긴다. |

## 확률·상태 경계

새 본체의 `drawWeightedMixedCards`는 매 선택 후 `localExcluded`에 ID를 더하므로 한 offer 안에 중복 ID가 없다(143480~143490행). `draftPoolForCategories`는 소유 카드, 이미 뽑힌 카드, RULE 제한, 상호 배타 그룹, 색별 draw 조건을 매번 다시 적용한다(143492~143509행). `completeRandom`은 이 경로와 별도로 `drawCompatibleDraftCards`에서 가중치 추첨 후 선택 카드를 제거하며, 호환되지 않는 카드도 제거된다는 점이 분포에 영향을 준다(143511~143524행). 이 조건부 순서를 포함하지 않은 확률 표는 검증 근거가 아니다.

기존 Rust 엔진 자체가 9월 28일 카드·RULE·특수 이동 전체를 지원하지 않는다(`projects/augment-chess/engine/README.md`). 새 버전의 완전한 규칙 지원을 기존 v7 성공 검사에서 추론할 수 없다. 새 profile, source 기반 실제 전이 오라클, 공개 관측 계약, Rust ruleset을 함께 승인해야 한다.

## 가지치기 판단과 남은 검증

DOM, HTML/CSS, 애니메이션, 사운드, 입력, 계정, 저장소 설정, 성능 로그는 게임 상태나 관측에 나타나지 않는 한 포팅하지 않는다. 희귀도 장식은 제외하되 같은 함수군의 추첨 가중치는 유지한다. `monster`의 붉은 예고 효과처럼 표시가 공개 관측을 바꾸는 경우는 무조건 제외하지 않고 관측 계약과 대조한다.

현재까지는 소스 SHA 확인, 선언 AST 파싱, 카드 ID·필드 비교만 수행했다. 최신 본체를 새 headless profile로 실행한 합법 행동·전이·확률·정보 분리 검사는 미완료다. 따라서 이 문서는 규칙 동기화 완료나 배포 승인 근거가 아니다.

## 관측 결과와 한계

| 비교 | 상태 | 결과 | 근거 |
| --- | --- | --- | --- |
| 번들 SHA | 성공 | 이전 `e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c`, 새 `958e8e6787d8d107152e4c07e45736d2ffbf05ad8fad4e7de63558c3c70d024c` | 원본 bytes SHA-256 |
| 선언 AST | 성공 | 이전 top-level 이름 7,262개, 새 10,890개; 동일 이름 5,194개, 신규 3,642개, 같은 이름의 소스 변경 2,054개 | Acorn top-level 선언 hash 비교. 신규·변경 수에 UI 코드도 포함됨 |
| 공개 카드 ID | 성공 | 256개에서 256개, 추가·삭제 없음 | 선언 실행 후 `CARDS$1` 비교 |
| 공개 카드 필드 | 성공 | 모든 카드에 `draftWeightUnits`가 추가되고, 14개 카드에 추가 설명·별점·대상·art 변경 | 선언 실행 후 필드별 비교. 설명 변경을 모두 실행 규칙으로 보지 않음 |
| 신규 합법 행동·전이·확률 | 미실행 | 새 headless 초기화 profile과 상태 fixture 없음 | 9월 profile은 새 SHA를 거부함 |
| Rust·JS 회귀 및 CI | 미실행 | 아직 두 구현을 변경하지 않음 | 이번 문서는 조사 초안 |

경고·오류: 처음 Git fetch는 작업 checkout의 `.git` 쓰기 제한으로 거부되었다. 관리 worktree에서 승인된 정상 Git fetch로 재시도해 `origin/develop`의 SHA가 기존과 같음을 확인했다. 규칙 비교의 나머지 단계에는 관측한 오류가 없다. 새 본체를 선언만 실행한 결과를 실제 대국 oracle 성공으로 해석하지 않는다.

원시 번들은 추적하지 않는다. 새 원본은 위 저장소 상대 경로에만, 이전 원본은 공개 사이트 자산으로 다시 확인할 수 있다. 조사 임시 출력은 공유 산출물이 아니며 장기 보존을 전제하지 않는다.

## 정정과 후속 기록

| 시점 | 변경·후속 기록 | 기존 결론에 미치는 영향 |
| --- | --- | --- |
| 해당 없음 | 최초 조사 | 해당 없음 |

## 공유 전 점검

- [x] 기준 SHA·입력과 조사 범위를 기록했다.
- [x] 선언 실행과 대국 전이 미실행을 구분했다.
- [x] 절대 경로·로컬 식별 정보·비밀을 공유 본문에 포함하지 않았다.
- [x] 공개 GitHub 로그인명을 확인했다.
- [x] 원시 자료와 임시 로그를 복사하지 않았다.

## 2026-10-08 후속: 새 원본의 제한된 실제 전이 실행

`projects/augment-chess/contracts/catalog/execution-profile-20261007-probe.json`과
`projects/augment-chess/oracle/tools/site-parity/october-source-probe.js`를 추가했다.
새 원본 SHA-256 `958e8e6787d8d107152e4c07e45736d2ffbf05ad8fad4e7de63558c3c70d024c`,
공개 catalog hash `disfTpO_11gGrXKr6Q_AO_SsQHVecw5XIXQ0mJExQ4k`,
Acorn 8.16.0 parser SHA-256 `24974706ffc00984a334f9ee085cc3cb2bf0a0ec80787ea9374ab5b5b6535681`,
기존 headless bootstrap SHA-256 `1ae94ede71d44f517e9e0ba3ed9c364a65fb58bc15163ee582d93e41fe93969a`를 검사한다.
새 번들은 원문 선언 10,885개와 import 문장 3개를 실행하며, 다른 최상위 문장 359개는 부작용과 의존성 검토 전까지 제외한다.
임의의 카드 규칙 함수를 재구현하지 않고 원문 `resetGame`, `beginInitialGameFlow`, 드래프트 후보 함수,
`finishDraft`/`finishChaosDraftBundle`, `completeDraftStep`/`completeGrandDraftStep`을 호출한다.
원본 번들은 Git에 추가하지 않았다.

실행 명령(원본과 파서 경로는 실행 환경의 절대 경로를 넣는다):

```sh
for style in normal chaos grand; do
  node projects/augment-chess/oracle/tools/site-parity/october-source-probe.js \
    "${SOURCE_MAIN}" "${ACORN_PARSER}" "$style"
done
```

실제 실행 3건 모두 성공했다. 각 모드에서 8×8 보드와 `draft` 상태를 만들고,
원문이 제시한 합법 선택 중 첫 항목을 적용해 `ok: true`와 다음 드래프트 actor/단계 전이를 관측했다.
`normal`은 첫 카드 `sacrifice` 후 white→black, `chaos`는 첫 묶음
`sacrifice,ghost` 후 white→black, `grand`는 첫 카드 `princess` 후 black→white였다.
`normal`에서는 드래프트를 한 번 더 완료해 `play`에 진입했다. 원문 `collectValidAiActions`의
비완전 카드 후보 모드에서 27개 행동을 얻고, 첫 일반 수인 white 폰 (6,0)→(5,0)을
원문 `applyAiAction`으로 적용했다. 결과는 `ok: true`, moveCount 1, 다음 actor black이며
목적지에는 폰이 있다. 원문 `pieceVisibleToColorAt`로 센 공개 보드 기물은
이 수 전후 white·black 각각 32개였다. 이는 전체 공개 observation schema 대조가 아니다.
정해진 host RNG seed `0x6d2b79f5`를 사용했으며 이 결과로 확률 분포 일치를 주장하지 않는다.

이 프로필은 **제한된 실행 조사용**이다. 기존 9월 adapter의 bootstrap을 해시로 고정해 재사용했으나
10월 원본에 대한 359개 최상위 문장의 의존성·부작용 검토, 실제 player별 observation,
전체 합법 행동과 카드 선택·적용, chance outcome 열거, 확률·조건부 분포, Rust 대조는 미완료다.
따라서 Phase A 전체 또는 Phase B~E 완료로 분류하지 않는다. Rust 엔진과 기존 v7 catalog는 변경하지 않았다.
원본 범위가 검증되기 전에는 10월 `rulesVersion`을 운영 계약에 등록하지 않는다.

### 후속 수정 파일과 검증 상태

| 구분 | 파일·상태 |
| --- | --- |
| 새 조사 코드 | `projects/augment-chess/oracle/tools/site-parity/october-source-probe.js` |
| 새 조사 프로필 | `projects/augment-chess/contracts/catalog/execution-profile-20261007-probe.json` |
| 실제 실행 | 세 모드 초기화·드래프트 첫 선택: 3/3 성공; normal 일반 수 생성·적용: 1/1 성공; 변조 원본 SHA 거부: 1/1 성공 |
| 미검증 | 완전 행동 목록·카드 효과·확률·전체 공개 관측·Rust 교차검증·9월 회귀·CI |

## 2026-10-08 후속: `completeRandom` 소규모 정확 분포

`projects/augment-chess/oracle/tools/site-parity/october-draft-distribution.js`에서
10월 원본의 `draftCardWeight`, `hasLatestMutuallyExclusiveDraftCard`,
`drawCompatibleDraftCards`를 실행했다. 입력은 원본 공개 정의의
`metal`, `qxe1`, `taunt` 세 카드만 담은 **합성 후보 풀**, 추첨 목표 2장,
기존 선택 없음이다. 원본 가중치는 각각 정수 단위 70, 85, 85이며
`metal`과 `qxe1`은 상호 배타다. 추첨된 카드가 호환되지 않아도 남은 풀에서
제거되는 원문 순서를 그대로 열거했다. 6개 가중 추첨 경로마다 구간 내부의
난수 값을 원문 `drawCompatibleDraftCards`에 공급해 카드 결과와 난수 소비 순서를 확인했다.

```sh
node projects/augment-chess/oracle/tools/site-parity/october-draft-distribution.js \
  "${SOURCE_MAIN}" "${ACORN_PARSER}"
```

| 선택 순서 | 정확한 확률 |
| --- | ---: |
| metal, taunt | 7/24 |
| qxe1, taunt | 17/48 |
| taunt, metal | 119/744 |
| taunt, qxe1 | 289/1488 |

합계는 1이다. 첫 선택이 `taunt`임을 조건으로 하면 두 번째가 `metal`일
확률은 14/31, `qxe1`일 확률은 17/31이다. 이 조건은 **선택 ID를 안다는
수학적 조건**이다. 해당 ID가 각 플레이어에게 공개되는지, 원본의 전체
`completeRandom` 후보 풀과 색별 제한, Rust의 대응 결과가 일치하는지는
아직 검증하지 않았다. 따라서 Phase B 전체 완료로 분류하지 않는다.

## 2026-10-08 후속: 최상위 의존성 분류와 합성 카드 전이

기준 commit은 PR #44의 `b83c2bf2fa768e324e529460ed4ca1481e0c8ccc`이다.
원본 SHA, 공개 catalog hash, probe `rulesVersion`은 위 기록과 같다.
Node.js 22.20.0, Acorn 8.16.0을 사용했고 네트워크가 차단된 VM에서
각 호출을 최대 15초, 선별한 최상위 문장을 각각 최대 1초로 제한했다.
`node:vm`은 적대적인 원본에 대한 완전한 보안 격리가 아니다. 원본 SHA가
다르면 실행을 거부한다.

`projects/augment-chess/contracts/catalog/october-top-level-review.json`은
선언·import를 제외한 359개 문장의 0 기반 순번을 빠짐없이 분류한다.
분류 수는 규칙 12, 간접 의존성 119, 공개 관측 56, UI/DOM 129,
네트워크·계정·저장소 42, 미확정 1이다. 분류는 원본 SHA와 AST 순서에
묶여 있다. 규칙·표시에 필요한 99번, 127~168번, 171~194번 문장을 원문
순서로 실행하고, 브라우저 시작의 `resetGame`(331번)은 모드 선택 후
명시적으로 호출한다. 109번 AI worker 등록이 로컬 후보 순서에 미치는
영향은 미확정이다. 선언부의 다른 데이터·검증 문장은 참조 여부를
분류했으나 실행하지 않았으므로 전체 실행 의존성의 동적 증명은 아니다.

재현 명령:

```sh
OCTOBER_SOURCE_MAIN="${SOURCE_MAIN}" OCTOBER_ACORN_PARSER="${ACORN_PARSER}" \
  node projects/augment-chess/oracle/tools/site-parity/october-source-probe.test.js
```

`SOURCE_MAIN`은 추적하지 않은 SHA 고정 원본,
`ACORN_PARSER`는 SHA가 기록된 Acorn 파일의 절대 경로다. 12개 테스트가
성공했다. normal/chaos/grand 모두 원본 선택으로 첫 드래프트부터
`play`까지 진행해 합법 일반 수를 적용했고 다음 턴으로 전이했다
(각각 2·2·12회 선택, 첫 행동 후보 28·58·12개). 잘못된 행동과 원본
SHA 불일치는 거부됐다. 기존 9월 profile과 오라클 소스는 변경하지 않았다.

| 기능 | normal | chaos | grand |
| --- | --- | --- | --- |
| 원본 초기화·모드별 보드 | 실행 성공 | 실행 성공 | 실행 성공 |
| 드래프트 행동·단계 완료 | 2회 선택 성공 | 2회 선택 성공 | 12회 선택 성공 |
| 첫 합법 일반 수·턴 전이 | 실행 성공 | 실행 성공 | 실행 성공 |
| 공개 행동 전체·카드 선택 전체 | 미검증 | 미검증 | 미검증 |
| 플레이어별 전체 공개 observation | 미검증 | 미검증 | 미검증 |
| 종료·승리·확률 전이 전체 | 미검증 | 미검증 | 미검증 |

아래 카드 fixture는 정상 초기화와 원문 드래프트 완료 후 **원본
`addCardToPlayerDeck`으로 대상 카드를 합성 지급한 fixture**다. 그 후
원본 `collectValidAiActions`의 카드 행동을 선택하고 `applyAiAction`으로
적용해 사용 완료 상태도 확인했다. 합성 지급은 실제 드래프트에서
해당 시점에 카드를 얻을 수 있다는 증거가 아니다.
조건과 결과는 `october-source-probe.test.js`의 assertion으로 재실행한다.

| P0 카드 | 원본 실행에서 확인한 결과 | 남은 검증 |
| --- | --- | --- |
| `switcheroo` | 원본 효과 후 생성된 왕→아군 폰 합법 행동을 `applyAiAction`으로 적용. 두 기물 ID를 보존해 위치를 교환하고 1반수를 사용했다. | 카드 소유·사용 경로, 부가 상태 전체 |
| `holdout` | 원본 효과의 `readyTurn=28`. 합성 공유 턴 27에서는 폰, 28에서는 원본 자동 승격 함수로 퀸. | 28수 실제 행동 재생과 턴 종료 호출 경계 |
| `chimera` | 나이트 대상 거부, 퀸 대상 허용. 원본 `chimeraMajorTypes`와 `chooseChimeraNextType`의 모든 난수 구간에서 결과 종류를 열거했다. | 실제 퀸 이동 후 변신, 각 분포와 공개 관측 |
| `monster` | 소환 후 정상 행동 세 번째 반수 종료 때 동일 ID 괴물이 이동했다. 0·1·2·4·5에서는 조건 거짓, 3·6에서 참. | 포획·보호·승리와 다양한 seed |
| `reaper` | 퀸을 사신으로 변형, 나이트 거부, 목표 영혼 수 2, 아군 피해·적 포획자만 집계하는 필터 확인. | 실제 포획 사건, 승리·종료 전이 |

`chimeraMajorTypes(state)`는 이 초기 상태에서 서로 다른 20종
(`queen`, `rook`, `herald`, `primeMinister`, `amazon`, `jester`,
`hook`, `man`, `assassin`, `reaper`, `windmill`, `bear`,
`magicGirl`, `berserker`, `siren`, `undead`, `hedgehog`,
`princess`, `octopus`, `grappler`)을 반환했다. 원본
`randomChoice`의 `floor(random * 20)`을 각 구간의 중점으로
실행했으므로 균등 RNG 가정 아래 각 유형은 1/20이다. 실제 이동
전체에서 난수 호출 순서와 변신 후 상태는 아직 검증하지 않았다.

은신 효과도 소유권 우회 합성 상태에서 원본 함수로 실행했다. 동일한
보드에서 white의 숨은 흑 비숍 관측은 `null`, black 관측에는 비숍이
있고 `hiddenFrom` 내부 필드는 노출되지 않았다. 양측의 드래프트 선택,
상대 카드, 이력, 종료 메시지까지 아우르는 공개 observation 계약은
미검증이다.

기존 9월 회귀 테스트는 이 checkout에 동결 baseline 파일이 없어 시작하지
못했다. `RUNNER_TEMP=/tmp`로 실행해도
`cache/site-baseline/baseline.json` 부재(`ENOENT`)가 원인이었다.
이 후속 기록은 **제한된 probe와 일부 합성 카드 전이**의 근거다. 정식
execution profile, P0 전체 카드 행동 fixture, 관측 계약 및 Rust 교차
검증은 아직 완료하지 않았다. 다음 단계는 9월 baseline을 마련해 회귀를
실행하고, 10월 원본 카드 보유→선택→적용 경로와 양측 observation을
재생 가능한 fixture로 고정하는 것이다.

## 2026-10-08 후속: 실제 획득 4종·양측 보드 투영·9월 client 복구

### 범위와 변경 파일

- `projects/augment-chess/oracle/tools/site-parity/october-acquisition.test.js` 신규: 원본 normal 드래프트의 조건부 획득, 원본 행동 적용, 제한된 양측 보드 투영.
- 이 연구 영수증 갱신. 10월 probe와 9월 실행 profile, Rust 엔진, 원본 JS 번들은 수정하거나 추적하지 않았다.

입력은 10월 원본 `main-Dm4wrmOx.js` SHA-256 `958e8e6787d8d107152e4c07e45736d2ffbf05ad8fad4e7de63558c3c70d024c`, Acorn 8.16.0 SHA-256 `24974706ffc00984a334f9ee085cc3cb2bf0a0ec80787ea9374ab5b5b6535681`이다. 기존 probe의 SHA·최상위 문장 분류·bootstrap 게이트를 그대로 통과한 뒤 실행한다. 원본 드래프트 추첨 함수를 바꾸지 않고 xorshift32 난수 seed를 주입했다. 아래 seed는 **특정 카드가 이미 후보로 나오는 조건부 재생**이며 무조건부 등장 확률 측정이 아니다.

| 카드 | seed (10진) | 원본 첫 후보 | 실제 획득·효과·턴 확인 | 남은 경계 |
| --- | ---: | --- | --- | --- |
| `holdout` | 74 | `severance, holdout, campfire` | white 후보 선택·덱 슬롯 반영, 원본 카드 행동으로 폰 지정, `readyTurn=28`, 원본 일반 수 후 black·1반수 | 실제 28수 재생·자동 승격과 공개 결과 미검증. 기존 합성 27/28 경계만 유지 |
| `switcheroo` | 169 | `castling, hook, switcheroo` | white 획득·원본 카드 행동, 원본 왕→폰 합법 행동으로 ID 보존 교환, black·1반수 | 상대 관측의 세부 이력 미검증 |
| `chimera` | 17 | `nullification, checker, chimera` | white 획득·원본 카드 행동으로 퀸에 효과 부여, 원본 일반 수로 black·1반수 | 퀸의 **실제 이동 후** 무작위 변신·관측 미검증. 기존 합성 fixture가 유형 집합만 열거 |
| `reaper` | 66 | `siege-ram, martyrdom, reaper` | white 획득·원본 카드 행동으로 퀸을 사신으로 변경, 원본 일반 수로 black·1반수 | 실제 아군 포획 2회 및 원본 승리 전이 미검증 |
| `monster` | 해당 없음 | normal 드래프트 대상이 아님 | 원본 `CARD_CATEGORY_BY_ID`는 `RULE`; `ruleCardPool()`에 존재하지만 일반 `draftPoolForCategories`에는 없음 | 시작 RULE 이벤트의 정상 설정·선택·발동부터 자동 이동까지 연결한 fixture 미완료 |

카드 후보 여부는 원본 `draftPoolForCategories`(143493행), 선택과 덱 획득은 `finishDraftSelection`(143250행), 행동은 `collectValidAiActions`(160514행)와 `applyAiAction`(161275행)을 사용했다. `monster`는 `maybeApplyOpeningRuleEvent`(142296행)에서 선택되는 별도 시작 RULE 경로다. 합성 지급한 기존 fixture를 실제 획득 성공으로 재표시하지 않는다. 드래프트 4종의 `winner`는 선택 직후 `null`이었다. 이 한 상태로 종료 조건 전체를 검증했다고 보지 않는다.

### 공개 관측 계약: 관측한 부분과 미검증 부분

아래 `__publicPieceView`는 **9월 headless bootstrap을 SHA 고정하여 재사용한 투영**이다(`game-adapter.js` 128행). 10월 원본 자체의 모든 viewer 전용 API를 검증한 것은 아니다. 원본 `pieceVisibleToColorAt`(187718행)와 은신 카드 실행을 함께 사용했다. 내부 `state` 또는 로컬 UI에 값이 있다는 사실만으로 공개를 인정하지 않는다.

| 필드·결과 | JS 출처 | viewer와 공개 조건 | 숨김·마스킹 조건 | fixture·결과 |
| --- | --- | --- | --- | --- |
| 8×8 보드의 비은신 기물 타입·색 | 원본 `pieceVisibleToColorAt`; bootstrap `__publicPieceView` | white·black, 해당 칸이 viewer에게 보일 때 | 보이지 않는 기물은 `null`; `hiddenFrom` 같은 내부 필드는 제거 | 새 양측 은신 검사 1/1 성공. 양측의 흑 퀸은 동일하게 보임 |
| 은신 흑 비숍 | 원본 `stealth`; 위 보드 투영 | 소유자 black에게 비숍이 보임 | 상대 white에게 해당 칸 `null`; black 투영에도 내부 `hiddenFrom` 필드는 없음 | 새 양측 은신 검사 1/1 성공; 두 viewer 가시 기물 수 차이 1 |
| 카드 덱·선택 후보 | 원본 `startDraft`·`finishDraftSelection`·`playerDeck` | 내부 후보·덱 반영만 확인 | 상대 공개 시점·숨김 정책 미확정 | 실제 획득 4종 검사 성공, **공개 계약 미검증** |
| 턴·단계 | 원본 `completeDraftStep`·`applyAiAction` | 내부 draft→play, white→black 전이 확인 | viewer별 공개 형태 미확정 | 실제 획득·일반 수 검사 성공, **투영 미검증** |
| 공개 행동 이력·포획·카드 결과·자동 효과 | 원본 `recordBoardHistory`(166762행), `recordOnlineEvent`(132854행) 및 개별 효과 | 공개 시점·필드 미확정 | 내부 선택·난수·deferred 값의 마스킹 미확정 | **미검증** |
| 승리·종료·확률 전이 | 원본 `applyAiAction` 및 효과별 판정 | 공개 결과 schema 미확정 | chance seed·숨은 선택 공개 여부 미확정 | **미검증** |

은신 결과의 양측 보드 투영만 확인했으며, 이를 **전체 observation이나 public action의 비누출 증거로 확대하지 않는다**. `trolley`의 상대 비공개 선택·제거 결과, 다른 deferred/randomized 카드, 카드 후보와 이력, 종료·승리 projection은 아직 대조하지 않았다. 원본 내부 선택을 상대의 공개 행동으로 열거하지 않는다.

### 9월 28일 동결 client 복구와 회귀

공식 고정 URL의 `main-OahWs0tU.js`와 Acorn 8.15.0을 외부 `${ARTIFACT_ROOT}/cache/site-baseline-20260928-e5ed84fc` 슬롯에 다시 받았다. SHA-256은 각각 `e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c`, `fdb08546776ec6228b03e8d02b40d4ab3255bae5f401adba7ff5dad927ac5c9c`였고 카탈로그의 바이트 수 12,892,256·241,575와 일치했다. `site-20260928.json`의 원래 `frozenAt=2026-09-28T07:41:32.828Z`와 파일 메타데이터로 **client 전용** manifest를 만들었다. 오늘의 HTML·worker를 9월 자료로 둔갑시키지 않았다. 저장소 추적 파일에 baseline을 추가하지 않았다.

`prepare-current-baseline.js --verify`는 9월 `rulesVersion=augment-site-20260928-e5ed84fcf8e72a24`, profile `accelerate-headless-semantic-v7-faithful-init-v1`, projection `source-visible-20260928-v2`, 256 공개 카드와 257 정의를 확인했다. `latest-client.test.cjs`의 34/34 검사가 실제 실행되어 세 모드 초기화·드래프트·관측·전이, profile 거부, 종료 등 기존 회귀가 통과했다. 이는 **client 전용** 복구다. 원본 9월 worker·HTML의 완전한 묶음과 `offline-oracle.test.js`는 복구하거나 실행하지 않았다. 기존 `cache/site-baseline` 전체 baseline 부재와 이 제한을 구분한다.

재현(저장소 루트, `${ARTIFACT_ROOT}`는 외부 절대 경로):

```sh
node projects/augment-chess/oracle/tools/site-parity/prepare-current-baseline.js --verify \
  "${ARTIFACT_ROOT}/cache/site-baseline-20260928-e5ed84fc"
ACCELERATE_SITE_BASELINE_LATEST="${ARTIFACT_ROOT}/cache/site-baseline-20260928-e5ed84fc" \
  node projects/augment-chess/tests/site-adapter/parity/latest-client.test.cjs
OCTOBER_SOURCE_MAIN="${SOURCE_MAIN}" OCTOBER_ACORN_PARSER="${ACORN_PARSER}" \
  node projects/augment-chess/oracle/tools/site-parity/october-source-probe.test.js
OCTOBER_SOURCE_MAIN="${SOURCE_MAIN}" OCTOBER_ACORN_PARSER="${ACORN_PARSER}" \
  node projects/augment-chess/oracle/tools/site-parity/october-acquisition.test.js
```

| 검사 | 실제 결과 | 입력·seed |
| --- | --- | --- |
| 9월 `prepare-current-baseline --verify` | 성공 1/1 | 9월 원본 main·Acorn SHA와 `site-20260928` catalog |
| 9월 `latest-client.test.cjs` | 34/34 성공, 실패 0 | 위 9월 client 전용 manifest; 테스트 내부 seed는 해당 파일의 12345 등 |
| 기존 10월 `october-source-probe.test.js` | 12/12 성공, 실패 0 | 10월 SHA·parser SHA; 고정 난수 및 기존 fixture |
| 신규 10월 `october-acquisition.test.js` | 5/5 성공, 실패 0 | 10월 SHA·parser SHA; 위 4개 xorshift32 seed, 은신 검사는 seed 1 |
| 10월 SHA 불일치 거부 | 기존 프로브 검사 1/1 성공 | 원본이 아닌 입력 거부 |
| 9월 전체 `offline-oracle.test.js`, 10월 CI·Rust 차분·성능 | 미실행 | 9월 전체 worker baseline 없음; 이번 작업 범위 밖 |

### 결론 변경과 Rust 동기화 차단 요인

이전에는 P0 5종 모두 **합성 카드 지급 후 효과**만 검증했다. 현재는 그중 normal 드래프트 카드 4종의 **실제 후보→선택→덱→원본 효과 행동**을 검증했다. `monster`는 일반 드래프트 카드가 아니라 시작 RULE 경로임을 확인했다. 9월 회귀는 `ENOENT`에서 client 전용 34/34 성공으로 바뀌었다. 전체 10월 공개 observation, `monster` 정상 시작 RULE 발동, `chimera` 실제 이동 변신, `reaper` 실제 포획 승리, `holdout` 28수 실제 승격과 9월 전체 worker oracle은 완료로 표시하지 않는다. 이 경계를 해결하고 10월 정식 실행 profile 및 양측 비누출 계약을 확인하기 전에는 Rust 동기화 착수 근거가 부족하다.

추가 구조 검사 명령 `node --test .github/scripts/check-repository-policy.test.mjs`는 14/14 성공, `node .github/scripts/check-repository-policy.mjs`는 통과(403 indexed files), `git diff --cached --check`는 오류 없이 통과했다. 처음 제한된 sandbox에서는 Git 하위 프로세스 `EPERM`으로 구조 검사 두 개가 실패했고, 같은 stage 입력을 정상 Git 접근 권한으로 재실행해 성공했다. 이 중간 환경 오류를 코드 실패로 취급하지 않는다. 원격 CI는 아직 요청·관측하지 않았다.


## 2026-10-08 후속: 시작 RULE과 실제 퀸 이동 전이

기준 브랜치는 기존 PR #44의 `feature/october-rules-sync`이고, 조사 시작 SHA는
`2a37eaf7f8b8058caaeaaa70159592e43cbae982`이다. 미추적 원본 번들은
보존하고 Git에 포함하지 않았다. 번들·Acorn SHA는 앞의 10월 프로브와 동일하다.

`october-acquisition.test.js`에 원본 함수 경로 두 건을 추가했다. 첫째,
`resetGame(false, [])`로 headless 상태를 만든 뒤 원본
`maybeApplyOpeningRuleEvent`를 호출하고 `beginInitialGameFlow`로 드래프트를
시작했다. `ruleOpeningEnabled=true`, `ruleSelectionEnabled=true`,
`selectedRuleCardIds=['monster']`에서 원본 이벤트 `hit`, 적용 카드 `monster`,
소환 기물 1개를 확인했다. 원본이 제시한 드래프트 선택을 끝내고 합법 수
3반수를 `collectValidAiActions`와 `applyAiAction`으로 실행했다. 1·2반수에는
괴물 위치가 유지되고 3반수에 동일 기물 ID가 다른 칸으로 이동했다.
이 검사는 headless 시작 경계에서 RULE 함수를 명시 호출한다. 브라우저의
`resetGame(true, …)` 전체 비동기·UI 흐름을 검증한 것으로 표시하지 않는다.

둘째, seed 17의 원본 normal 드래프트에서 `chimera`를 획득하고 원본 카드
행동으로 퀸에 부여했다. 원본 합법 행동으로 백 폰 d2→d3, 흑 일반 수,
백 퀸 d1→d2를 적용했다. 이동 뒤 같은 퀸 ID와 `chimera=true`, 3반수,
다음 차례 black을 확인했고 기물 종류가 원본 `chimeraMajorTypes(state)`의
결과 집합에 속했다. 이 단일 재생은 실제 변신 경로의 증거다. 앞서 확인한
20개 균등 구간의 종류 열거와 구분하며, 이동 전체의 정확한 확률 분포나
양측 공개 결과의 증거로 합산하지 않는다.

`holdout`은 합성 `turnsTaken` 경계 검사와 실제 첫 수까지만 확인했다.
추가로 원본 행동을 임의 선택해 32반수까지 진행한 조사에서는 게임이
white 승리로 끝나 `turnsTaken={white:16,black:16}`이었다. 따라서 28수
승격에 도달하지 못했다. 이 종료 경로를 승격 실패나 성공으로 해석하지
않는다. `reaper`의 실제 아군 포획 2회·승리, `trolley` 비선택자 정보 차단,
전체 viewer별 이력·덱·자동 효과·종료 projection도 미검증이다.

프로브 manifest에 실제 호출 경계와 미지원 범위를 적었다. 그 파일의
`supportLevel`은 계속 `bounded-source-probe-only`다. 정식 10월 headless
profile, Rust ruleset, JS↔Rust 차분 일치를 선언하지 않는다.


### 이번 후속의 실행 검사와 남은 작업

| 검사 | 이번 실행 결과 | 범위 |
| --- | --- | --- |
| 10월 `october-source-probe.test.js` | 12/12 성공 | 세 mode 초기화·드래프트·선별 전이·합성 P0 경계 |
| 10월 `october-acquisition.test.js` | 8/8 성공 | 실제 획득 4종, 선택 RULE `monster`, 실제 `chimera` 퀸 이동, `holdout` 28수, 제한된 보드 투영 |
| 9월 client baseline `--verify` | 1/1 성공 | 9월 main·parser SHA, catalog, profile 검증 |
| 9월 `latest-client.test.cjs` | 34/34 성공 | client 전용 회귀; 전체 worker oracle은 미실행 |
| Rust `cargo test -p augment-chess-engine --lib --offline` | 727 성공, 0 실패, 53 ignored | 기존 9월 엔진 회귀; 10월 차분 검사 아님 |
| repository-policy 테스트·검사, staged diff | 14/14 성공, 정책 통과, 공백 오류 없음 | stage된 세 파일 기준 |

Rust 첫 시도는 worktree 기본 `target`의 읽기 전용 파일시스템 때문에
실행 전에 실패했다. `${ARTIFACT_ROOT}/build/cargo`로 출력 경로를 지정해
동일한 library 검사를 재실행했고 위 결과를 얻었다. repository-policy
테스트도 제한된 Git 접근에서 한 차례 실행되지 않았으나 stage 후 정상
Git 접근으로 14/14 통과했다. 두 환경 오류를 코드 테스트 실패로 세지 않는다.
원격 CI는 이번 후속에서 관측하지 않았다.

남은 순서는 `reaper`의 실제 아군 포획 2회 승리, 전체 viewer별 공개
관측과 `trolley` 비누출, 10월 정식
headless profile, Rust 규칙 버전 분리·P0/P1 구현, 동일 조건의 JS↔Rust
행동·전이·분포 차분, 9월 전체 worker oracle이다. 이 입력이 없으므로
10월 동기화 완료 판정을 보류한다.


### 정정: `holdout`의 실제 28수 자동 승격 재생

앞의 32반수 조사는 임의 행동 중 흑 왕이 잡혀 조기 종료된 사례였다.
그 뒤 동일한 원본 획득 seed 74에서 deathmatch를 끄는 정상 게임 설정을
적용하고, 원본 `collectValidAiActions`가 제시한 **비포획 합법 수**를
재생했다. 중간 드래프트도 원본 후보와 `finishDraft`·`completeDraftStep`으로
완료했다. `turnsTaken`을 직접 수정하지 않았다. 각 행동은
`applyAiAction` 성공을 확인했고 전체 경로는 140 단계 상한 안에서
56반수에 도달했다. 공유 턴 27에는 지정한 폰이 폰으로 남았고,
white·black 각각 28턴을 완료한 직후 같은 기물 ID가 `queen`으로
바뀌며 `promotedFromPawn=true`, `winner=null`이었다. 이 사례는
`october-acquisition.test.js`의 8번째 검사로 고정했으며 8/8 통과했다.
앞 문단의 `holdout` 미도달 설명은 첫 탐색 경로의 결과로만 읽어야 한다.
이 한 경로로 다른 자동 효과·종료 조건과의 모든 조합을 검증한 것은 아니다.
