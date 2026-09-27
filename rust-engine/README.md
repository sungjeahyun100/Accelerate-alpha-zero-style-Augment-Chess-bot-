# 독립 Rust 규칙 엔진

`accelerate-engine`은 2026-09-27에 동결한 Augment Chess 본체 규칙을 순수 Rust로
포팅하는 crate다. Python, PyO3, 신경망, ONNX runtime 또는 JS subprocess에 의존하지
않는다. 전체 256개 공개 카드와 84개 기물의 포팅은 진행 중이며 컴파일·커널 검사
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
normal·chaos의 초기 weighted draw와 그 역조건화는 진행 중이다. identity 연결은
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

외부 snapshot import는 원문 필드의 존재·null·빈 카드 slot을 보존한다. typed 기본값은
내부 실행에 사용하고 실제 규칙 변화만 source state에 반영한다. 원문 필드의 보존은
그 필드의 규칙 실행 지원을 증명하지 않는다. 아직 포팅하지 않은 기물, 활성 효과,
카드 또는 단계는 `UnsupportedFeature`로 명시한다.

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
보존한다. 현재 headless oracle의 UI stub은 모든 렌더링의 난수·규칙 부작용까지
동일하다는 증명이 없어 실제 브라우저 전체 실행과의 parity는 추가 확인이 필요하다.
normal·chaos의 초기 weighted draw는 카드 draw predicate의 trolley·black-box shuffle,
opening weight, 배타적 카드 묶음 교체를 포함해 6개 seed씩 실제 본체와 카드 종류·순서·
identity 및 RNG 전체가 일치했다. source predicate 자체에 있는 bounded trolley subset
열거·score당 240개 cap도 유지한다. normal·chaos의 선택 후 획득/자동 패시브/다음 선택자
전환과 역조건화, grand의 최종 자동 패시브 정산, 전체 카드·RULE·특수 이동·예약 전이
및 큰 조합 행동의 lazy 열거는 진행 중이다. 초기 draw 지원은 그 카드의 효과 지원을
뜻하지 않으며 기본 normal 설정을 표준 play로 임의 대체하지 않는다. 선택·효과·활성
상태의 미구현 의미는 명확한 오류이며 전체 256개 지원의 GO 근거로 사용하지 않는다.

동결 metadata는 `bridge/catalog/`에서 compile time에 공유한다. 이 데이터는 초기값,
카탈로그·공개 정책의 단일 근거이며 실행 결과를 fixture에서 찾아 반환하지 않는다.
`accelerate-engine-json`은 줄 단위 검증 adapter이며 규칙은 동일 crate API로 실행한다.

Linux 검사는 저장소 밖 고정 빌드 슬롯을 사용한다.

```sh
CARGO_TARGET_DIR="$HOME/.cache/accelerate/build/full-stack-implementation/cargo" cargo test -p accelerate-engine
```

Windows에서는 `%APPDATA%\Accelerate\build\windows\full-stack-implementation\cargo`를
사용한다. 현재 호스트에서 proc-macro DLL은 OS 정책에 의해 차단되어 Linux 검사를
사용했다. Windows 실제 빌드·CI 검증과 전체 catalog differential 검사는 별도 완료
증거가 필요하며 원시 실행 로그와 일회성 비교 결과는 Git 밖 reports에 둔다.
