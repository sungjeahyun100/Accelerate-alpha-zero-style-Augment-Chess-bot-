# 프로젝트 독립 어댑터 wire 계약

이 패키지는 객체 선택, capability 발견, 요청·응답 및 오류의 언어 독립 JSON 표현을 정의한다. 첫 구현체는 `adapter-runtime` Rust crate다. 게임의 상태·기물·카드·행동과 모델 추론의 세부 의미는 각 프로젝트가 소유한다. 이 계약의 payload와 result는 프로젝트별 schema가 정의하며, 공유 schema는 그 내용을 임의로 해석하지 않는다.

`schemas/adapter-v1.schema.json`의 `AdapterDescriptor`, `AdapterRequest`, `AdapterResponse`, `AdapterError`, `AdapterOutcome`가 v1 wire 형태다. `manifest.json`의 SHA-256은 schema JSON을 재귀적으로 키 정렬하고 공백 없이 직렬화한 UTF-8 바이트의 digest다. 같은 의미의 줄바꿈과 공백 차이는 hash를 바꾸지 않지만, 필드·값 또는 키 순서에 영향을 받지 않는 canonical 문서 자체는 변경할 수 없다. 새 버전을 발행할 때 schema·manifest를 함께 갱신하고 `node packages/adapter-contract/verify.mjs`를 실행한다.

호출자는 `(projectId, adapterId, contractVersion.major, implementationVersion)`를 정확히 선택하고 capability의 request/response schema ID와 SHA-256을 모두 명시한다. runtime은 등록된 descriptor와 완전히 일치하는지 검사한다. minor 버전도 호출 시 일치해야 하며 자동 변환, 미등록 규칙 대체 또는 다른 객체 fallback은 없다. 각 capability의 `access`가 읽기 전용인지 host transaction이 필요한지를 선언한다. 읽기 전용 호출은 상태를 변형하지 않고, transaction 호출은 host가 관리하는 복제본에서 실행한 뒤 결과·한도·취소 검사가 성공해야 커밋한다. 외부 side effect는 이 계약이 허용하지 않는다.

`limits.maxWork`는 객체가 소비한 작업 단위의 상한, `maxResults`는 검증된 결과 수의 상한이다. 페이지 응답은 실제 검사한 수 `examined`, 계속할 때 불투명 `cursor`, 끝일 때 `exhausted: true`를 전달한다. 목록 순서는 프로젝트가 정의한 원문 순서다. 제한 때문에 목록을 끝내지 못했는데 `exhausted: true`로 보고하지 않는다. 취소 신호와 monotonic deadline은 호출 시 host context로 전달된다. wire 요청의 시간을 신뢰하여 deadline을 연장하지 않는다.

오류 `kind`는 `unsupported`, `invalid_input`, `stale_revision`, `limit_exceeded`, `cancelled`, `execution_failed` 중 하나다. `code`는 구체적이고 안정적인 원인, `message`는 개발자가 오류를 진단할 수 있는 설명이다. 실패를 성공 결과나 모호한 경고로 감추지 않는다. 비밀·개인정보·private 상태를 진단에 넣지 않는다. 경고가 있어도 계속할 수 있는 경우에는 성공 응답의 `diagnostics`에 정확한 코드와 설명을 남긴다.

이 패키지는 게임 프로젝트와 Accelerate 봇 프로젝트 어느 쪽에도 의존하지 않는다. 둘 모두 자신이 사용하는 domain schema와 adapter 구현 버전을 따로 고정해야 한다.
