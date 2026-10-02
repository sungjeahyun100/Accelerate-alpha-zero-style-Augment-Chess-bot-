# Augment Chess 브라우저 바인딩

기존 `augment-chess-engine::adapter::GameAdapterSession`을 Worker에서 호출하는
얇은 WASM transport다. 규칙·공개 투영·행동 승인·원자적 전이는 엔진에 남는다.
프로덕션 API는 전체 상태·RNG·비공개 이력의 import/export를 제공하지 않는다.

## API

- `BrowserGameSession.new_game(config_json, seed)`는 v7 `GameConfig` JSON과 u32 seed로
  독립 대국을 만든다. 생성 실패는 정확한 `AdapterError` JSON 문자열을 throw한다.
- `metadata()`는 `rulesVersion`, `catalogVersion`, `protocolVersion`, `projectionVersion`,
  `observationPolicyHash`, `implementationVersion`, `executionProfileVersion`,
  `executionProfileSha256`, `descriptors`를 JSON으로 반환한다. 동결 카탈로그·관측
  정책의 hash와 실제 등록 descriptor를 사용한다.
- `revision()`, `decision_actor()`, `result()`는 현재 revision, 다음 결정 담당 색,
  `white`/`black`/`draw` 또는 `null` 결과만 반환한다.
- `invoke_json(request_json, timeout_ms)`는 공통 `AdapterRequest<GameAdapterPayload>`를
  받고 공통 `AdapterOutcome<GameAdapterValue>` JSON을 반환한다. descriptor·schema
  hash·revision을 정확히 지정해야 하며 실패도 `ok:false`와 원래 오류를 보존한다.
- wasm-bindgen의 `free()`로 대국을 해제한다. 사용 중인 객체를 재사용하지 않는다.

## 실행 한도와 상태 보존

요청은 1 MiB, 응답은 8 MiB 이하이며 timeout은 1~30,000 ms의 유한한 정수다.
각 descriptor의 작업량·결과 한도와 모든 기존 승인 검사는 그대로 적용한다.
쓰기 호출은 별도 엔진 세션에서 검증·전이하고 응답 직렬화·크기·최종 deadline까지
확인한 뒤 Worker의 세션을 교체한다. 실패하면 원래 상태와 RNG가 남는다.
읽기 호출은 동일 registry를 유지하므로 불투명 페이지 cursor를 이어서 사용할 수 있다.

일반 Web Worker의 `postMessage`는 동기 WASM 호출 도중 실행되지 않는다. timeout은
브라우저 `Performance.now()`에 기반한 협력적 checkpoint로 처리한다. 강제 취소는
Worker 종료와 해당 세션 폐기로 처리해야 한다. 취소된 쓰기가 적용되었다고 가정하거나
취소 후 이전 Worker의 늦은 응답을 채택하면 안 된다. 네이티브 host는 기존 `std::time::Instant`
타입과 취소 의미를 유지한다.

## 빌드·검증

crate는 `cdylib`과 `rlib`을 제공한다. WASM은 `wasm32-unknown-unknown` target에서
빌드하고 의존성과 동일한 `wasm-bindgen-cli 0.2.126`으로 `--target web`을 생성한다.
산출물은 저장소 외부의 기존 생성물 슬롯에 두고 정적 배포 묶음으로 복사한다.

네이티브 테스트는 같은 입력의 직접 엔진 호출과 브라우저 transport 결과를 비교하고,
세 스타일의 생성·관측·드래프트 전이, revision·결과 한도 실패의 rollback, 페이지 cursor,
오류 원문·비공개 입력 거부를 검사한다. 이 성공은 실제 WASM/browser 성공을 대신하지
않는다. 실제 WASM artifact를 로드하는 브라우저 검증은 예제 CI가 별도로 실행한다.

승급·순서 선택·대형 기물·비공개 투영은 별도 `browser-test-fixtures` feature의
고정 네 가지 case로 검사한다. 이 테스트 빌드만 `browser_test_case(case_id)`를
내보내며 임의 상태·envelope 입력은 받지 않는다. 기본 production WASM에는 이
export가 없어야 한다. 테스트와 배포 산출물 슬롯을 구분하고 테스트 artifact를
HF 배포 묶음으로 복사하지 않는다. 같은 feature로 `browser-fixtures`를 실행하면
기본 세 스타일 case에 고정 네 case를 더해 native 기준 자료를 생성한다.

현재 부모 엔진의 고정 2×2 bigRook case에서 `(3,1) → (2,1)` 겹치는 이동은 공개
후보로 승인되지만 적용 시 `invalid_game_state` / `invalid state: conflicting identity
test-large-rook`로 실패한다. 테스트는 이 원문과 rollback을 native/WASM 양쪽에서
비교하고, 별도로 겹치지 않는 이동의 성공 경로를 확인한다. 오류 parity 성공을
해당 이동의 정상 지원으로 보고하지 않는다. 이 재현은 부모 엔진의 후속 항목이며
예제에 규칙 우회를 넣지 않는다. 부모 엔진이 수정되면 알려진 실패 기대와
`knownIntegrationLimit`을 재검토하고 성공하는 원래 이동을 다시 검증한다.
