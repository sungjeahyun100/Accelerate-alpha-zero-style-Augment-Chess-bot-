# HF Static Space 엔진 시험 예제

Svelte 화면과 Rust WASM 엔진을 정적 파일로 제공한다. 실제 대국 상태는 게임 Worker가
소유하고 화면은 공개 관측과 엔진이 확정한 조작 결과를 표시한다. 서버·계정·저장소의
비밀 없이 실행하며, [원본 게임](https://augmentchess.org/)의 화면 구성을 참고한 새 표현을 사용한다.
저장소의 [작업 규약](../../../AGENTS.md)과 [개발 기준](../../../docs/ENGINEERING-STANDARDS.md)을 따른다.

## 준비 상태와 범위

일반·혼돈·그랜드 스타일의 수동 시험을 제공한다. 게임 엔진, AI 백엔드, 학습 모델의
준비 상태는 각각 확인한다. PR #28이 브라우저 AI 실행 방식을 결정하기 전에는
`backend-pending`, 학습 모델이 없으면 `model-missing`을 표시한다. 사람 대 AI 인터페이스는
공개 관측·공개 이력·독립 탐색 seed·유한한 예산에 결속하며 실제 AI 완료와 모델 성능을
이 예제의 수동 대국 검증만으로 주장하지 않는다.

`BrowserEngineClient`가 화면의 유일한 게임 진입점이다. 각 요청은 요청 ID·대국 ID·현재
revision에 결속하고 이전 대국의 응답과 오래된 revision의 입력을 거부한다. 게임 Worker의
cooperative deadline은 5초, client의 hard timeout은 15초다. 메시지는 8 MiB 이하,
commit된 조작은 대국당 512개 이하로 제한한다. 엔진이 확정한 응답을 받은 조작만
화면 상태와 in-memory journal에 반영한다.

hard cancel은 소유한 게임 Worker를 종료한다. 초기 설정·seed와 acknowledged 공개 행동
의도를 in-memory journal에서 새 Worker에 다시 적용한 뒤 마지막 관측과 position revision이
일치하는지 확인한다. 재구성 실패는 원래 오류로 반환하고 성공 상태로 대체하지 않는다.
이 내부 journal과 비공개 seed·상태는 조사 자료로 내보내지 않는다.

현재 AI 경계는 `BrowserBotDriver` 계약과 pending driver다. 백엔드가 준비되지 않은
상태에서는 AI Worker를 실제로 spawn하지 않는다. 이후 backend 활성화 때 초기화·지원 기능·
공개 정보에 따른 행동 선택·취소·해제를 연결한다. 학습 모델은 고정 revision의 manifest,
파일 SHA256, encoder·규칙 계약을 확인해야 하며 현재 예제에는 검증된 모델이 포함되지 않는다.

기보와 현재 관점의 공개 관측 기반 자료를 JSON으로 내려받을 수 있다. 이 자료는 화면과
조작 조사용이며 비공개 전체 상태의 완전 재현을 보장하지 않는다. 기록 불러오기·재생과
보드 편집기는 이번 예제의 범위에 포함하지 않는다. 경고·오류의 코드와 원문은 진단
패널과 개발 검사에 보존한다.

## 개발과 빌드

Node 22.12 이상인 22 계열, Rust 1.96.0, `wasm32-unknown-unknown` target,
`wasm-bindgen-cli` 0.2.126이 필요하다. CLI와 browser crate의 `wasm-bindgen` 버전은
정확히 같아야 한다. 프런트엔드는 Svelte 5·TypeScript·Vite로 구성하고 Python·ONNX
runtime 의존성은 브라우저 AI 결정 이후 해당 실행 경계에 추가한다.

저장소 루트에서 설치한다. demo는 root npm workspace와 독립된 package/lock을 사용한다.

```text
cd projects/accelerate/demo
npm ci --workspaces=false
cd ../../..
rustup target add wasm32-unknown-unknown --toolchain 1.96.0
cargo install wasm-bindgen-cli --version 0.2.126 --locked
npm --prefix projects/accelerate/demo --workspaces=false run wasm
npm --prefix projects/accelerate/demo --workspaces=false run check
npm --prefix projects/accelerate/demo --workspaces=false test
npm --prefix projects/accelerate/demo --workspaces=false run build
npm --prefix projects/accelerate/demo --workspaces=false run dev
```

`wasm`은 engine·계약을 재사용한 얇은 `augment-chess-browser` crate를 빌드한다.
`wasm-inputs.json`에 엔진·공통 계약·바인딩·실행 프로필의 소스 fingerprint와 compiled
JS/WASM 두 파일의 SHA256을 남긴다. `build`와 `dev`는 이 근거가 현재 입력과 같을 때
생성된 파일과 동결 공개 카탈로그를 알려진 경로에서 복사한다.
WASM이 준비되지 않았으면 파일 오류를 그대로 반환한다. 구현되지 않은 엔진 대역을
배포본에서 대신 실행하지 않는다. Vite는 `.env` 파일을 자동으로 읽지 않는다.

Windows 빌드가 애플리케이션 제어에 차단되면 정확한 오류와 종료 코드를 보존하고,
Linux CI에서 실행한 결과를 별도로 확인한다. 반복 재시도나 보안 설정 변경을 하지 않는다.

## 생성물 경로

`npm --prefix projects/accelerate/demo --workspaces=false run paths`로 현재 생성물 슬롯을 확인한다.
Windows 기본 루트는 `%APPDATA%/Accelerate`, CI는 `$RUNNER_TEMP/Accelerate`다.
OS와 checkout 경로에서 계산한 고정 ID로 슬롯을 나눈다. 이 로컬 ID와 절대 경로는
HF 묶음의 manifest에 넣지 않는다.

| 용도 | 루트 아래 경로 |
|---|---|
| WASM·공개 자산·정적 화면·HF 묶음 | `build/hf-static-demo/<checkout-id>/<os>/` |
| Cargo와 Vite 캐시 | `cache/hf-static-demo/<checkout-id>/<os>/` |
| 빌드 근거·브라우저 진단 | `reports/hf-static-demo/<checkout-id>/<os>/` |

Linux에서 재생성 가능한 빌드·캐시는 `${XDG_CACHE_HOME:-$HOME/.cache}/accelerate`를
사용할 수 있다. WSL의 보존할 보고서·배포 묶음은 호스트 APPDATA를 확인한 뒤
`ACCELERATE_OUTPUT_ROOT`에 변환한 Windows 생성물 루트를 지정해 내보낸다. 변환 실패 시
다른 보존 경로를 임의로 정하지 않는다. root override는 checkout과 겹칠 수 없다.
Windows에서 등록한 worktree를 WSL에서 빌드할 때 `.git`의 Windows 경로를 Linux Git이
해석하지 못하면 `ACCELERATE_GIT_EXECUTABLE=git.exe`로 기존 Windows Git을 사용한다.
Git 설정이나 worktree 등록 파일을 고쳐 다른 checkout에 영향을 주지 않는다.
알려진 생성물 슬롯을 다시 만들기 전에 최종 경로와 link/junction 경계를 검사한다.

## 실제 엔진과 브라우저 검증

native binding 검사는 Rust 엔진으로 공개 요청과 결과를 생성하고, 같은 입력을 WASM에
적용해 관측·행동·revision·결과·정확한 오류를 비교한다. fixture는 Git에 쌓지 않고
현재 실행의 외부 reports 슬롯에 만든다. 브라우저 검사는 실제 빌드 화면과 Worker를
사용한다. 개발 환경에서 Playwright browser 설치가 필요할 때는 출력·캐시 위치를 먼저 정한다.

```text
npm --prefix projects/accelerate/demo --workspaces=false run test:native
npm --prefix projects/accelerate/demo --workspaces=false run wasm-test
npm --prefix projects/accelerate/demo --workspaces=false run test:wasm
npm --prefix projects/accelerate/demo --workspaces=false run test:browser
```

`test:native`는 실제 Rust 테스트를 실행한 뒤 `browser-fixtures`의 표준 출력을
reports 슬롯의 `native-fixtures.json`에 저장한다. `test:wasm` 전에 실행한다. stderr의 원래 진단은
보존하고 실패 시 JSON을 성공 근거로 사용하지 않는다. 검사 순서와 정확한 환경은 아래 CI가 정의한다.
`wasm-test`는 고정 최소 재현 사례를 여는 `browser-test-fixtures` feature를 별도
`wasm-test` 슬롯에 빌드한다. 이 파일은 native/WASM 비교 검사에서만 사용한다.
배포용 `wasm`은 `--no-default-features`로 빌드하며 공개 자산 복사는 feature가 비어 있는
생산용 stamp만 허용한다. 시험용 binding·fixture는 HF 묶음에 포함하지 않는다.
생산용 binding에는 고정 시험 사례 생성 export가 없어야 하며 실제 WASM 비교 검사가
이 조건을 확인한다. 시험용 compiled 파일은 별도 CI artifact로만 보존한다.
Chromium 자동 검사, Chrome·Edge의 관측한 실행, HF iframe에서의 관측한 실행을 구분해 보고한다.

native 조건화 통합의 CHAOS 완료 검사는 공개 드래프트에서 수동으로 활성화하는
`switcheroo`/`royal-command` 묶음을 선택한다. 같은 2개 드래프트와 양쪽의 이동,
입자 2개·제안 16개·전이 후보 4096개 한도, 전체 공개 관측·이력 비교와 재구성을 유지한다.
이는 시험 입력 정책이며 게임 규칙이나 source-prior 알고리즘을 바꾸지 않는다.
첫 묶음의 `otherworld`를 선택한 원래 경로는 첫 이동의 무작위 변화와 독립 RNG가
일치하지 않아 유한한 제안 예산을 소진할 수 있다. 실제 Rust 검사는 균등 확률
`p=q=1/8`, 가중치 1, 불일치 거절과 원본 snapshot 보존을 별도로 확인한다.
이 완료 검사로 임의의 확률적 공개 이력을 16개 제안 안에 재구성한다고 보장하지 않는다.
더 효율적인 확률 전이 제안과 그 `p/q` 근거는 부모 PR의 AI 후속 범위다.

## 수동 CI와 근거 재사용

[Static browser demo validation](../../../.github/workflows/static-demo.yml)은 상시 유지하는
workflow다. 관련 엔진·계약·demo·CI 입력의 PR 변경에서는 `all` 검사를 실행하며,
수동 `scope=bindings`는 첫 체크포인트의 native/WASM 검증만 수행하고,
`scope=all`은 프런트엔드 검사와 binding 빌드를 병렬 실행한 뒤 실제 브라우저와 HF 묶음을
검증한다. 같은 브랜치에서는 한 실행만 진행하고 기존 실행을 취소하지 않는다.
Rust 빌드 worker 2개와 job별 timeout을 명시하며 임시 artifact는 14일 보존한다.

bindings와 frontend의 성공 근거는 입력 SHA256, 공통 계약·엔진·브라우저 바인딩·카탈로그·
실행 프로필·검사·lock·workflow·실행 명령·OS·runner image·Node/npm/Rust 버전에 결속한다.
SHA가 달라도 이 입력이 동일한 경우에만 재사용한다. marker와 실제 검사 로그·fixture,
보존한 compiled WASM의 파일 SHA256을 확인한다. 캐시 hit 자체를 검사 성공으로 보고하지 않는다.
실제 브라우저와 현재 소스 commit의 배포 묶음은 각 `all` 실행에서 다시 확인한다.

## HF Static 묶음과 수동 업로드

검토·커밋한 깨끗한 checkout에서 정적 빌드를 실행한 뒤 묶음을 만든다. 소스 fingerprint와
빌드 파일 SHA256이 빌드 시점과 달라졌으면 묶음 생성을 거절하고 다시 빌드하도록 안내한다.

```text
npm --prefix projects/accelerate/demo --workspaces=false run build
npm --prefix projects/accelerate/demo --workspaces=false run bundle
npm --prefix projects/accelerate/demo --workspaces=false run verify-bundle
npm --prefix projects/accelerate/demo --workspaces=false run test:bundle
```

`test:bundle`은 최종 묶음을 `/static-demo/` 하위 경로에서 제공해 상대 자산 경로와
manifest의 소스 커밋을 확인한다. JS·WASM 응답 변조 시 각각 정확한 무결성 오류를
표시하고 새 게임을 비활성화하는지도 검사한다. 이 검사는 배포 manifest가 없는
개발 미리보기의 `test:browser`와 별도로 CI에서 항상 실행한다.

묶음에는 `sdk: static`, `app_file: index.html`인 `README.md`, 화면·Worker·WASM·동결 공개
카탈로그, `NOTICE.md`, `source-manifest.json`을 담는다. manifest는 Git commit·공개 소스
저장소·계획된 HF 소유자와 상대 자산 경로별 크기·SHA256을 기록한다. checkout 절대 경로와
로컬 계정 이름을 기록하지 않는다. 경로 탈출·link·비밀
파일 형식·누락·변조를 거부하며 최대 512개, 파일당 32 MiB, 총 128 MiB로 제한한다.
학습 모델은 이후 고정 revision·해시·인코더 계약을 확인하는 지연 로드 경계에서 제공한다.

CI의 `hf-static-demo-<sha>` artifact 또는 로컬 `hf-bundle` 슬롯이 업로드 대상이다.
계획된 HF 소유자는 **`daejunnom`**이며 GitHub 저장소 소유자와 별개다.
Space 이름은 아직 지정하지 않았으며 GitHub 원격에서 HF namespace를 추론하지 않는다.
묶음 manifest의 `deploymentPlan`에 이 소유자와 이름 미정 상태를 기록한다.
현재 작업에서는 Space 생성·업로드·게재를 진행하지 않는다. 이후 대상 Space와
게재 작업을 지정한 뒤 기존 인증을 이용해 수동으로 업로드한다.
인증 토큰을 소스·명령·문서에 넣지 않는다.

```text
hf upload daejunnom/<space-name> <verified-hf-bundle-directory> . --type space --commit-message "Verified static engine demo"
```

이 PR의 workflow는 Space 생성·업로드·모델 게시를 실행하지 않는다. 실제 게시 후에는
Space의 iframe과 직접 `*.hf.space` 접속을 모두 확인한다. 단일 Worker 기반 WASM은
공유 메모리나 COOP/COEP를 요구하지 않는다. 향후 실제 실행 경로가 이를 요구하면
HF 설정과 iframe 동작을 함께 검증한다.

배포 방식은 [HF Static Spaces](https://huggingface.co/docs/hub/spaces-sdks-static),
[HF Space 설정](https://huggingface.co/docs/hub/spaces-config-reference),
[HF CLI 업로드](https://huggingface.co/docs/huggingface_hub/guides/cli)를 따른다.
