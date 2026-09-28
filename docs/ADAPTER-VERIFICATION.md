# 최신 공식 클라이언트와 게임 어댑터 검증

## 기준과 범위

2026-09-28에 고정한 공식 클라이언트 `main-OahWs0tU.js`의 SHA-256은
`e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c`다.
계약은 `site-20260928`, headless profile은
`accelerate-headless-semantic-v7`을 사용한다. 원문 번들과 parser는
저장소 밖의 검증된 캐시에서 읽으며 Git에 넣지 않는다. 이 검증은
**로컬 오프라인 8×8의 normal, chaos, grand**에 한한다.

공식 [규칙](https://augmentchess.org/rules/)과
[게임 방법](https://augmentchess.org/how-to-play/)은 왕 포획, 행동 불능,
반복 및 연장전, 세 모드의 드래프트, HP 기물의 직접 공격을 설명한다.
실제 카드 효과는 바뀔 수 있어 고정된 원문 실행 결과를 현재 버전의
행동 기준으로 사용한다.

## 자동 검증

`node --test tests/site-adapter/parity/latest-client.test.cjs`는 다음을 확인한다.

- 원문 manifest SHA와 객체 불변성. 검증 이후 파일 메타데이터를 바꾸어
  다른 원문을 실행할 수 없다.
- 세 스타일의 원문 초기 state/RNG와 어댑터 초기 state/RNG 일치,
  초기 드래프트 완료, 양측 공개 관측 검증, 실제 첫 기물 수의 전체
  다음 state/RNG 일치.
- RULE `acceleration` 적용과 원문 선택 풀에 없는
  `capture-the-flag`의 명시적 거부, 잘못된 행위자·오래된 action 거부,
  원본 Position 불변성 및 다음 호출 복구.
- 관측에서 새 원문 state 필드가 분류되지 않으면 해당 이름을 포함한
  명시적 오류로 실패하고, 후속 정상 호출은 복구된다.
- 서로 다른 action cursor의 페이지 순서·격리와 dispose,
  `newGame` 이후에도 사용자 지정 microtask 한도가 유지되는지 확인.
- 동일 Position과 action을 새 어댑터에서 실행한 결과와, 조회·거부·
  성공한 호출 이후 기존 어댑터에서 다시 실행한 결과를 비교한다.
  특히 grand의 `summon-colossus`를 포함한다.
- 실제 원문 행동으로 **상대 왕 포획 승리**, **상대 행동 불능 승리**,
  **콜로서스 HP 3→2와 공격 기물 제자리 유지**,
  **실제 왕 이동에 따른 3회 동형반복·별 동률 무승부**를 만들고
  어댑터의 전체 state/RNG 및 결과와 직접 대조한다.
  기본 연장전 설정 45수·10수를 확인한 뒤, 1수·1수로 축약한 설정에서
  실제 왕 이동으로 연장전 진입과 무진전 별 동률 종료를 검사한다.
  `gameover`만 주입하는 검사는 사용하지 않는다.

직접 원문 probe와 어댑터 호출은 각각 독립된 VM에서 시작한다.
원문 probe의 임시 전역 상태를 같은 VM의 다음 호출에 남기면 거짓
불일치가 발생하기 때문이다. 공개 어댑터의 연속 호출은 별도 검사한다.

`othello`의 실제 합법 전이는 원문 state에 색별 턴 종료 재평가 latch인
`othelloPending`을 만든다. 이 raw 필드는 양측 `publicState`에
노출되지 않으며, 변환된 보드와 사용된 카드가 기존 공개 표면으로
표현되는지 양측 관측 검증에서 확인한다. v7 관측 정책은 이 필드를
`internalBookkeeping`으로 분류한다.

## 카드 표면 조사

`infra/tools/site-parity/audit-card-surface.js`는 카탈로그의 256개 카드
각각에 원문 ID·카테고리·효과와 세 모드별 조사 셀을 만든다. CI에서는
`--baseline=site-20260928 --shard=0/8`부터 `7/8`까지 서로 다른
샤드를 병렬 실행할 수 있다. `ACCELERATE_SITE_BASELINE` 또는
`ACCELERATE_SITE_BASELINE_LATEST`로 원문 캐시 경로를 지정한다.
결과 JSON은 `$RUNNER_TEMP/Accelerate/reports/site-adapter` 또는
Windows 사용자 생성물 루트의 대응 `reports/site-adapter`에 쓴다.

원문 `ruleCardPool()`이 실제로 제공하는 RULE 26개는 해당 스타일의
게임 시작 시 직접 설치하여 관측까지 확인한다. 카탈로그에 있는
`capture-the-flag` 1개는 현재 선택 풀에 없어 비가용으로 따로
분류한다. 명시 요청은 오류가 되고 조용히 무시되지 않는다.

다른 카드는 정상 드래프트를 마친 원문 8×8 상태에 특정 카드 한 장을
손패로 주입한 **합성 표면 조사**다. 고정 시드의 정상 원문 드래프트에서
백은 두 번째, 흑은 첫 번째 제공 선택지를 고른다. 백의 첫 선택지
`last-stand`는 폰 전부를 fanatic으로 바꾸므로, 폰 조건 카드의
후보 여부를 혼동하지 않도록 원문이 제공한 대안을 명시적으로 택한다.
대안이 제공되지 않으면 조사 준비가 실패한다. 카드별 최대 두 후보를
원문 함수와 어댑터에서 독립 VM으로 적용해 수락 여부, 전체 state/RNG,
양측 관측을 비교한다. 카드 후보가 0개인 셀은 해당 한 국면의 결과일
뿐 해당 카드의 구현 오류나 사용 불가능성을 뜻하지 않는다. 후보가
더 있는 셀은 접두 부분만 조사한 것이다. 보고서의
`completeRuleCoverage`는 의도적으로 `false`다.

8개 샤드의 256개 카드 × 3개 모드, 총 768셀을 조사했다.
오류와 미조사 셀은 각각 0개였다. 후보가 없던 27셀은 모두 아래
9개 카드의 세 모드에서 나타났다:
`emergency-evacuation`, `exile`, `homecoming`, `joker`,
`judgment`, `miracle`, `necromancy`, `othello`, `outpost`.
이 27셀은 드래프트 직후의 단일 보드에서 대상 전제조건이 충족되지
않은 것으로 분류하며, 실제 경기 전체에서 사용할 수 없다는 판정이
아니다.

국면별 대상 전제조건이 중요한 카드에는 추가 합성 국면을
구성했다. `frenzy`·`log`·`holdout`은 백 폰이 살아 있는
실제 드래프트 결과, `exile`은 귀환 가능한 적 기물,
`othello`는 아군 둘 사이 적 기물, `judgment`는 포획 최다
기물, `emergency-evacuation`은 후퇴 가능 기물,
`homecoming`은 빈 시작 칸, `outpost`는 전진한 기물,
`miracle`은 잡을 대상이 있는 비숍, `necromancy`는 포획 기록에
있는 아군 비폰 기물, `joker`는 사용한 액티브 카드를 둔다.
각각 원문 후보 생성, 실제 적용 후 전체
state/RNG, 양측 관측을 검사한다. 이 국면들도 전체 조합의
증명은 아니다.

이 조사는 카드별 다른 보드, 전체 대상 조합, 카드·RULE 상호작용,
중·후반 드래프트와 OPENING 자동 적용을 증명하지 않는다. 특히
`collectValidAiActions`와 어댑터의 후보 열거가 브라우저 UI의
**모든** 합법 선택을 대표하는지 별도 검증이 필요하다. 최신 source와
구버전의 카탈로그 ID 수가 같다는 사실만으로 전이 의미의 일치를
주장하지 않는다.

## 남은 안전성 검증

기본 45수·10수 장기 진행 전체와 별 비동률 종료, 장기 다중 턴의 모든 분기,
특수 기물 84종의 모든 상태, 카드 256종의 모든 전제조건 및 조합,
원문 선택 가능 RULE 26종의 장기 전이, fog·은폐 상태의 공개 정보 누출
여부는 아직 완전하게 증명되지 않았다. 브라우저 DOM·애니메이션·
타이머·온라인·캠페인도 이 headless profile의 범위 밖이다.
로컬 검사 통과는 원격 CI 관측이나 전체 게임 규칙의 안전성 완료를
뜻하지 않는다.
