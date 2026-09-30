# v6 읽기와 제한된 v7 자료 변환

이 crate는 동결된 v6 `accelerate-position-v1` JSON을 **읽기 전용**으로 수용한다. v6 규칙 엔진을 연결하거나 실행하지 않는다. 입력 한도는 8 MiB, JSON 깊이 64, 노드 100,000개다. 중복 객체 키와 JavaScript의 안전한 정수 범위를 넘는 수를 거절한다.

`read_v6_position`은 최상위 일곱 필드의 정확한 목록, v6 rules/catalog/protocol 버전, `positionId`의 RFC 8785/JCS SHA-256, 8×8 보드와 기물 기본 필드, 행동자와 phase, RNG 경계, 이력의 v1 event 표식을 검사한다. `state`의 다른 필드와 이력 event의 다른 필드는 그대로 보존한다. 이 검사는 저장 형식과 기본 구조의 검증이다. 카드 효과, 기물 이동, 과거 이력의 규칙 적법성까지 증명하지 않는다.

`convert_verified_initial_template`은 다음 조건을 모두 만족하는 입력만 v7 **자료 envelope**로 바꾼다.

1. 전체 `state`가 동결된 v6 초기 템플릿과 정확히 같다.
2. 동결된 v6·v7 초기 템플릿의 전체 `state`가 서로 같고, 검토된 canonical SHA-256과 일치한다.
3. `history`가 비어 있으며 RNG cursor가 0이고 tape가 비어 있다. `rng.state`는 원본 값을 보존한다.

v6의 공개 catalog 식별자와 v7의 composite catalog 식별자를 버전별로 검사한다.
v7 초기 자료의 `faithful-init-v1` 실행 profile과 포함된 manifest의 JCS SHA-256도
검증한다. 성공 시 대상 rules·catalog 버전을 결속하고 `positionId`를 새로 계산하며,
원본 `positionId`와 검증 범위를 `ConversionEvidence`로 돌려준다. 그 외 상태는
`Unsupported`에 실패 위치와 이유를 담아 반환한다. 이 결과는 실행 가능한 v7
게임이라는 판정이 아니다. 실행하기 전에 v7 host가 별도로 상태와 규칙을 검증해야
한다. v6 이력을 실행·재생하거나 v7 이력으로 간주하지 않는다.

집중 검사: `cargo test -p augment-chess-v6-migration --locked`.
