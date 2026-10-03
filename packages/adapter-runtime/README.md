# 프로젝트 독립 Rust 어댑터 런타임

이 crate는 [공통 wire 계약](../adapter-contract/README.md)의 첫 Rust 구현체다. 공유 타입에는 게임 상태·기물·카드·보드·학습 타입이 없다. 등록과 호출은 프로젝트별 `S`(작업 상태), `P`(요청 payload), `R`(응답 result) 타입으로 고정하며, 여러 기능은 프로젝트가 정의한 typed enum이나 struct로 표현할 수 있다. 실행 중 JSON 직렬화 또는 `Any` downcast를 사용하지 않는다.

`AdapterRegistry<S, P, R>`에 객체를 등록하고 세션 시작 전에 `seal()`한다. 호출은 `AdapterRequest<P>`가 고른 `(projectId, adapterId, contractVersion.major, implementationVersion)`과 전체 계약 버전·capability·요청/응답 schema ID 및 SHA-256이 등록 descriptor와 일치할 때만 시작한다. 미등록 구현이나 다른 버전으로 대체하지 않는다. Rust의 `selection` 필드는 JSON에서 최상위 요청 필드로 펼쳐진다. 성공·실패 wire envelope는 `AdapterOutcome<R>`의 `{ok:true,response}`와 `{ok:false,error}`다.

`AdapterObject<S, P, R>`는 descriptor와 capability별 읽기 또는 쓰기 진입점, 프로젝트 결과 schema를 검증하고 실제 결과 수를 돌려주는 `validate_output`을 구현한다. `CapabilityAccess::ReadOnly`는 `AdapterHost::read_state()`만 빌리고 transaction을 시작하지 않는다. `Transactional`은 host 작업본에서 실행하고 결과·페이지·진단·호출 한도·취소·revision이 통과한 뒤에만 `commit_transaction`을 부른다. 실패하면 `rollback_transaction`을 부른다.

host는 `begin_transaction`에서 원본과 RNG·history·event를 격리해야 한다. `commit_transaction`은 성공 시 원자적으로 새 상태를 공개하고, 실패 시 원본을 유지한 채 소유한 transaction을 오류와 함께 반환해야 한다. 새 `CommittedRevision`은 원본 변경 전에 생성한다. 외부 서비스·파일 같은 되돌릴 수 없는 side effect는 객체 호출 안에서 수행하지 않는다. `AdapterError.code`에는 구체적인 원인을, `message`에는 개발자가 진단할 수 있는 설명을 넣는다. 계속 진행 가능한 경고는 `AdapterDiagnostic`에 남긴다.

호출 객체는 실제 작업마다 같은 `CallMeter`의 `consume_work`와 `checkpoint`를 사용한다. 후보를 검사하는 페이지 호출에는 `consume_examined`를 사용해야 `PageInfo.examined`와 정확히 대조할 수 있다. 결과를 축적할 때 `consume_results`로 한도를 먼저 확인해야 하며 `validate_output`의 실제 결과 수와 계수 값이 정확히 같아야 한다. 중첩 작업도 같은 meter를 전달한다. 취소 신호와 monotonic deadline은 `InvocationControl`로 host가 호출 시 전달하며 wire 요청이 임의로 연장하지 못한다. 페이지가 실제로 끝났는지는 프로젝트의 source 차분 검사로 검증한다.

두 개의 서로 다른 상태 타입을 쓰는 ledger transaction과 catalog 읽기·페이지 예시는 `tests/registry.rs`에 있다. 집중 검사는 `cargo test --manifest-path packages/adapter-runtime/Cargo.toml`, 정적 검사는 `cargo clippy --manifest-path packages/adapter-runtime/Cargo.toml --all-targets -- -D warnings`다.
