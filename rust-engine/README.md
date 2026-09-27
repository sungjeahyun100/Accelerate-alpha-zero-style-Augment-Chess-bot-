# 독립 Rust 규칙 엔진

`accelerate-engine`은 2026-09-27에 동결한 Augment Chess 본체 규칙을 순수 Rust로
포팅하는 crate다. Python, PyO3, 신경망, ONNX runtime 또는 JS subprocess에 의존하지
않는다. 전체 256개 공개 카드·27개 RULE·84개 기물의 포팅은 진행 중이며 컴파일·커널 검사
성공을 전체 규칙 지원의 GO 판정으로 사용하지 않는다.

`Position`은 `Arc<GameState>`를 가진 불변 snapshot이다. `apply`는 새 position을
반환하며 거부·오류 시 원래 position을 보존한다. 다중 칸 기물은 같은 identity를
공유한다. typed `Action`의 내부 position key는 이전 position의 행동을 새 position에
적용할 때 `StaleAction`으로 거부하는 근거다.

주요 API는 다음과 같다.

- `Position::new_game(GameConfig, seed)`와 `from_state(GameState)`
- `from_snapshot_value(Value)`, `export_state()`, `from_json`, `to_json`
- `with_metadata(RngState, Vec<Value>)`: v1 transport 바깥 RNG·이력 연결
- `actor()` / `decision_actor()`, `legal_actions()`, `apply()`, `result()`
- `action_stream()` / `next_page(limit)`: 불변 position에서 순서가 안정된 행동 페이지
- `bind_payload(Value)`, `validate_action(&Action)`: 선택한 행동을 직접 검증·연결
- `try_observe(viewer)`: 공개 board·카드·효과·이력·강조 좌표와 SHA-256 정보 상태 키
- `sample_initial_public(config, observation, independent_seed)`: 공개 초기 관측을 조건으로
  사이트에서 가능한 초기 world를 생성하며 미래 RNG는 독립 seed를 유지
- `condition_public_identities(observation)`: 기존 카드 identity와 내부 참조만 함께 연결
- `public_intent(&Action)`, `bind_public_intent(Value)`: 공개 클릭 좌표와 내부 실행 flags 분리

언어 연동과 신경망 입력은 실패를 전달하는 `try_observe`를 사용한다. 기존 infallible
`observe`는 강조 좌표를 제외한 Rust 내부 projection용 호환 경계이며 전체 공개 frame의
증거로 사용하지 않는다. 아직 미지원 규칙이 활성인 상태의 공개 강조 좌표를 빈 목록으로
대체하지 않고 엄격 관측 경계에서 `UnsupportedFeature`로 거부한다.

공개 초기 조건화는 현재 grand의 공개 28개 pool과 `draftDelete:true`의 세 mode에서
검사했다. 공개 카드 정의·카테고리 수·서로 배타적인 조합·opaque ID 가능성을 검사하고
독립 future RNG를 보존한 채 공개 frame 전체와 정보 상태 hash를 다시 비교한다.
normal·chaos의 초기 weighted draw와 공개 offer 역조건화도 같은 경계를 사용한다. identity 연결은
카드 타입·단계·사용 상태·slot을 바꾸지 않는다. 정상 공개 frame과 샘플의 의미가 맞지
않으면 `ConditioningMismatch`로 후보를 제외할 수 있으며 잘못된 schema·hash와 미지원
규칙은 별도 오류다. 이동 intent에는 공개 `from`·`destination` 좌표만 들어가고 앙파상
포획 위치 등 내부 flags는 실제 position에서 source UI의 첫 일치 행동으로 해결한다.

행동 페이지의 한도는 1~4096이며 `exhausted`는 실제 후보 소진 여부다. 현재 지원하는
기물·카드 family는 전체 후보를 한꺼번에 저장하지 않고 한 family씩 생성한다.
`apply`와 payload binding은 전체 행동 목록을 재생성하지 않는다. 아직 미구현인
premove의 순서 있는 복합 계획은 이 API가 있다는 이유로 지원 완료로 간주하지 않는다.
기물의 `PieceColor`는 white·black·neutral이고, 실제 결정을 하는 `Color`는 white·black이다.
중립 wall은 공개 장애물이며 어느 플레이어의 이동·효과 소속에도 포함되지 않는다.

원시 카드 payload의 승인은 실제 사이트 실행 wrapper를 따른다. 일부 대형 기물의
outpost처럼 화면 후보 밖의 표적도 이 wrapper가 승인하는 경우가 있다. 공개 행동과
`public_intent` / `bind_public_intent`는 실제 화면 선택 범위를 추가로 검사한다.
화면 후보 중 실제 effect가 거부하는 표적은 실행 행동 열거에서 제외한다. 이 두
경계를 하나의 후보 목록으로 취급하지 않는다.

외부 snapshot import는 원문 필드의 존재·null·빈 카드 slot을 보존한다. typed 기본값은
내부 실행에 사용하고 실제 규칙 변화만 source state에 반영한다. 원문 필드의 보존은
그 필드의 규칙 실행 지원을 증명하지 않는다. 아직 포팅하지 않은 기물, 활성 효과,
카드 또는 단계는 `UnsupportedFeature`로 명시한다.
같은 ID의 대형 기물은 모든 칸에서 속성이 같아야 한다. 원문 exile이 일부 칸만
옮겨 만든 비연속 footprint도 source restore처럼 보존하며, 배치 행동의 2×2 조건을
snapshot 자체의 허용 조건으로 강제하지 않는다.

관측은 `bridge/catalog/observation-20260927.json` 공개 allowlist와 사이트의 실제
`hiddenFrom`, camouflage, hallucination 의미를 따른다. 획득한 상대 카드도 사이트에서
공개되므로 `revealedOpponentCards`에 포함한다. 미래 RNG·선택 전 상대 offer와 내부
행동 flags는 관측에 포함하지 않는다. 실행 이력은 `accelerate-game-event-v1`의 정확한
private action과 색별 `public` 전이를 함께 저장하고 관측에는 viewer의 공개 전이만
제공한다. 포획 목록은 사이트 표시처럼 마지막 12개 타입·색·시각 변형만 공개한다.

현재 검사한 커널은 표준 초기 이동, 포획, 캐슬링, 앙파상, 승격 선택, 무료 카드 행동,
한 턴 안의 여러 행동, 대형 기물 identity, snapshot 보존과 공개 관측 경계다. 반복
판정은 사이트의 board position key·Map을 사용하고 별 합계가 낮은 쪽이 승리한다.
별 제한과 연장전은 양측 완료 턴, 폰 이동·포획·액티브 카드 사용의 진행 규칙을 쓴다.

초기화는 canonical reset defaults와 seeded `lcg32-v1` RNG를 사용한다. `draftDelete:true`
인 normal·chaos·grand의 32개 초기 piece identity와 RNG 순서, grand의 28개 offer identity·
정렬 순서는 실제 본체와 비교했다. grand의 초기 선택과 공개 카드 slot 전이도 구현했다.
카드 획득 표기 ID에 쓰이는 난수도 이후 게임 난수 순서에 영향을 주므로 소비 순서를
보존한다. 비교는 `accelerate-headless-semantic-v1`에서 수행한다. 이 profile은 potion
정리와 terminal replay microtask·이력 난수를 유지하며 DOM 애니메이션과 update-log
난수는 제외한다. 실제 브라우저 전체 실행의 future RNG equality 근거로 쓰지 않는다.
normal·chaos의 초기 weighted draw는 카드 draw predicate의 trolley·black-box shuffle,
opening weight, 배타적 카드 묶음 교체를 포함해 6개 seed씩 실제 본체와 카드 종류·순서·
identity 및 RNG 전체가 일치했다. source predicate 자체에 있는 bounded trolley subset
열거·score당 240개 cap도 유지한다. normal·chaos의 선택 후 인스턴스 복제·slot·획득 순서·
다음 선택자 전환을 구현했고 지원하는 자동 패시브로 실제 normal 드래프트 종료를
비교했다. 시계는 해당 profile의 고정 논리 시간에서 시작·일시 정지·완료 턴 increment를
보존한다. normal seed37의 corner-kick→reaper 획득까지 양측 전체 공개 frame·RNG·시계가
일치했고 첫 폰 이동 후에는 white 공개 frame·RNG·시계가 일치했다. reaper의 실제 효과와
black 카드 target hint까지의 전체 정산은 별도 미완료다. reaper 변환을 포함한
54개 카드와 추가 17개 카드 및 Judgment 임시 추방 분기의 primitive effect·화면
target·거부 경계는 실제 source 167개 경우로 검사했다. 승인된 143개 경우의 전체
primitive 상태·RNG·이력과 167개 UI 행동·첫 선택 좌표·불변 검증 결과가 일치했다.
정상 거부 23개와 원문 예외 1개를 구분한다. 이 검사는
공통 finishCard·hazard·종료 정산이나 변환 후 모든 기물 이동의 완료를 뜻하지 않는다.
초기 public conditioning은 세 mode
각 viewer에서 독립 future RNG를 유지하고 JCS 숫자 표현을 포함해 공개 frame 전체를
검증한다. 전체 자동 패시브, grand의 최종 정산, 전체 카드·RULE·특수 이동·예약 전이
및 큰 조합 행동의 lazy 열거는 진행 중이다. 초기 draw 지원은 그 카드의 효과 지원을
뜻하지 않으며 기본 normal 설정을 표준 play로 임의 대체하지 않는다. 선택·효과·활성
상태의 미구현 의미는 명확한 오류이며 전체 256개 지원의 GO 근거로 사용하지 않는다.

동결 metadata는 `bridge/catalog/`에서 compile time에 공유한다. 이 데이터는 초기값,
카탈로그·공개 정책의 단일 근거이며 실행 결과를 fixture에서 찾아 반환하지 않는다.
`accelerate-engine-json`은 읽는 중 16 MiB 한도를 적용하는 줄 단위 검증 adapter이며
규칙은 동일 crate API로 실행한다.

재생 기록은 replay 카드의 향후 동작에 사용되므로 `replay.rs`가 notation·재생 delta·
이동 rollback 상태를 유지한다. 원문의 `JSON.stringify` 비교에는 객체 삽입 순서가
영향을 주어 serde JSON과 기물·카드·색별 map에서 그 순서를 보존한다. position key와
공개 정보 상태 hash에는 계속 JCS를 사용한다. 표준 초기 이동 6개의 실제 전이는
전체 source state·RNG·실행 이력이 일치했고, 오프닝 효과 8개도 카드 정의의
name/text/art만 제외한 동일 비교를 통과했다. 원시 로그와 중복 전이 fixture는
저장소에 추가하지 않는다. 변형 기물의 기본 이동은 별도 순수 kernel 657개 경우에서
좌표·배열 순서·전체 flags가 일치했으며, 전역 이동 제한과 실제 실행 flags 정산의
완료는 별도 판정한다.

관측 v2는 renderer의 badge·공개 숫자·지형 표시·관계·quantum overlay·공개 카드 결과를
투영하고 동결 rulesVersion과 별도로 projectionVersion 및 전체 정책의 JCS SHA-256을
묶는다. 139개 공개 상태 필드의 중첩 schema와 기물·카드·선택창·이력의 공개 표면을
엄격히 검사한다. 내부 deadline·기물 ID·난수 window ID를 원형으로 전달하지 않는다.
대기 중 트롤리 window ID는 exact private action에만 남고 관측·공개 이력에는 없다.
트롤리 실행 family 자체는 아직 미완료다. 현재 source 관측 130개 경우에서 전체 JSON과
정보 상태 SHA-256이 일치했다. 초기 play의 공개 이동 강조도 이 비교에 포함하며,
대부분의 추가 상태 조합은 draft 단계의 renderer 표면 비교다. 실제 DOM 표시 88개
경우의 별도 검증 근거와 구분한다. 모든 활성 RULE의 fog·특수 강조·의사결정 전환이
완료되었다는 의미로 사용하지 않는다. Rust 1.96에서 36개 커널 검사와 1개 입력 한도
검사, 전체 target의 엄격 clippy 및 format 검사를 통과한 중간 checkpoint다.

Linux 검사는 저장소 밖 고정 빌드 슬롯을 사용한다.

```sh
CARGO_TARGET_DIR="$HOME/.cache/accelerate/build/full-stack-implementation/engine-check" cargo +1.96.0 test --locked -p accelerate-engine
```

Windows에서는 `%APPDATA%\Accelerate\build\windows\full-stack-implementation\cargo`를
사용한다. 현재 호스트에서 proc-macro DLL은 OS 정책에 의해 차단되어 Linux 검사를
사용했다. f41e45d 기반 Windows·Linux native CI의 전체 실행 성공은 확인되었고 각
OS 설치 검사 34개는 skip 없이 통과했다. 이후 진행 중인 replay·카드·관측 v2 변경의
검증과 전체 catalog differential 완료는 그 CI 결과와 구분한다. 원시 실행 로그와
일회성 비교 결과는 Git 밖 reports에 둔다.

이 167개 비교는 UI 후보를 먼저 실행한 reused oracle의 renderer cache가 남아 있는 환경이다.
후속 fresh-source probe에서 babyBear·grappler·Judgment의 animatedPieceIds가 snapshot 밖
activePieceAnimationUntil module Map에 따라 달라졌다. 기존 전체 상태 일치는 이 context에
한정하며 독립 snapshot 전이의 완전한 증거로 사용하지 않는다. 명시적인 실행 context와
profile version을 정한 뒤 전체 상태·RNG를 다시 검증해야 한다. 상태 field는 비교에서 제외하지 않는다.
