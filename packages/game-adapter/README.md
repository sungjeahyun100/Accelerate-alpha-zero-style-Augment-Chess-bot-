# 게임 어댑터 소스 프로젝트

이 private workspace는 고정된 공식 클라이언트를 headless로 실행하는 어댑터의
소스 소유 영역이다. 원문 번들, parser 사본, 실행 로그와 모델은 Git에 넣지 않는다.
Rust 규칙 엔진이나 Python 탐색·학습 구현을 포함하지 않는다.

`FrozenClientSource`는 외부 baseline manifest와 파일의 SHA-256을 확인하고
parser와 클라이언트를 준비한다. 호출자는 의도한 클라이언트 SHA-256을
`expectedClientSha256`으로 지정할 수 있다. `GameAdapter`는 이 source와
선택한 runtime contract를 명시적으로 받는다. 계약에 등록된 원문 파일명과
SHA-256이 다르면 VM 실행 전에 생성이 실패한다. 새 규칙 버전은 호출자가
그에 맞는 계약과 baseline을 함께 선택해야 한다.

각 `GameAdapter`는 자체 VM·RNG·실행 한도를 소유한다. `newGame`, `observe`,
`publicHints`, `actions`, `apply`, `result`, `actionStream`, `dispose`가 공개
진입점이다. `actionStream`은 Position 복사본과 별도 VM을 소유하는
`ActionCursor`를 반환한다. cursor는 `nextPage`와 `dispose`를 제공한다.
예외가 발생한 어댑터 VM과 cursor는 재사용하지 않는다. 개발·호환 검사에
필요한 원문 `evaluate` 접근은 기존 `infra` 경로에만 남겨 둔다.

루트 private workspace는 이 소스 프로젝트를 발견하기 위한 구성이다.
npm pack·publish·설치 산출물과 registry 배포 흐름은 이 프로젝트에 없다.
기존 `infra/tools/site-parity/` 실행 경로는 얇은 호환 진입점으로 유지한다.
