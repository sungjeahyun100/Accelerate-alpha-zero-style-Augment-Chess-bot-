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
