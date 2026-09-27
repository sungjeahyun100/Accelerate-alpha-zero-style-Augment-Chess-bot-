# 기여 방법

에이전트 작업은 [AGENTS.md](AGENTS.md), 개발·검증과 예외 기준은
[ENGINEERING-STANDARDS.md](docs/ENGINEERING-STANDARDS.md)를 함께 따릅니다.

## 브랜치 (Gitflow)

- `main`: 안정 버전. 직접 커밋하지 않습니다.
- `develop`: 다음 배포를 준비하는 통합 브랜치입니다.
- `feature/*`: 일반 기능이나 문서 작업 브랜치입니다. `develop`에서 분기하고 PR로 `develop`에 병합합니다.
- `release/*`: 배포 직전 정리 브랜치입니다. 완료 후 `main`과 `develop`에 병합합니다.
- `hotfix/*`: `main`의 긴급 수정 브랜치입니다. 완료 후 `main`과 `develop`에 병합합니다.

일반 개발 흐름은 다음과 같습니다.

```text
develop
   └─> feature/*
          └─> PR ──> develop
```

영역이 드러나는 브랜치 이름을 권장합니다.

```text
feature/rust-engine-*
feature/bridge-*
feature/python-*
feature/infra-*
feature/docs-*
```

## 영역별 책임

- `bridge/`: 언어 간 데이터·호출 계약과 얇은 PyO3 연동. maturin은 빌드·패키징을 담당합니다. 게임 규칙이나 탐색·학습 로직을 넣지 않습니다.
- `rust-engine/`: 실제 봇과 self-play용 독립 규칙 엔진. PyO3·신경망·ONNX runtime·Python 학습을 의존하지 않습니다.
- `infra/`: JS oracle 및 기존 검증·실험 도구. 기존 경로와 동작을 보존합니다.
- `python/`: AlphaZero/MCTS/신경망/self-play/training 연구 코드. 규칙을 별도로 재구현하지 않습니다.
- `tests/differential/`: JS oracle과 Rust 엔진의 동등성 검증만 둡니다.
- `pre_cpp_engine_code/`: Rust 포팅 참고용 C++ 초안입니다.
- `docs/`, `.github/`: 설계·개발 규약과 저장소 CI입니다.

PR 하나에는 가능한 한 한 영역의 변경만 포함하세요. 여러 영역의 계약을 함께 바꿔야 한다면 변경 이유와 영향 범위를 PR 본문에 명시하세요.

새 최상위 경로는 기존 영역에서 수용할 수 없는 책임과 소유 범위를 설명하고
`.github/repository-policy.json`에 등록합니다. 실험별 복제 폴더나 책임 없는 공통
폴더를 늘리지 않습니다. 함수·파일 분리는 줄 수보다 변경 이유와 인터페이스를 기준으로 합니다.
SRP는 기능별 응집도와 독립 책임에 적용하며 파일 길이 검사나 작은 함수별 파일 분할로
강제하지 않습니다. 관련 구현은 함께 두고, 불필요한 파일 파편화를 피합니다.

## 작업 순서

1. `develop`을 최신으로 받고 작업 영역에 맞는 `feature/*` 브랜치를 만듭니다.
2. 변경하고, 무엇을 왜 바꿨는지 드러나는 커밋 메시지를 작성합니다.
3. 관련 검사를 로컬에서 실행하고, 의도한 파일만 stage한 뒤 구조 검사도 실행합니다.
4. 푸시하고 `develop` 대상 PR을 엽니다.
5. 리뷰 승인을 받은 뒤 병합하고 feature 브랜치를 정리합니다.

리뷰 승인 인원 수는 팀 합의 후 확정합니다.

## 커밋 크기와 원격 공유 빈도

일반 PR은 **1~3개 논리 커밋**을 기본으로 합니다. 구현과 그 구현의 검사·문서를 함께
묶고, 큰 다단계 작업은 검증된 단계당 한 커밋으로 늘리며 PR에 분리 이유를 적습니다.
줄 수나 파일 수로 기계적으로 나누지 않고 리뷰·되돌리기 가능한 변경 이유를 기준으로 합니다.
파일별·저장별 커밋, 같은 수정을 반복하는 임시 커밋, 개수만 맞추는 빈 커밋을 피합니다.

원격 반영이 작업 범위에 포함되면 짧은 작업은 최종 검사 후 **1회 push**합니다.
1시간을 넘는 작업은 완결된 중간 단계와 최종 결과를 각각 공유합니다. 장기 작업에서는
**60~90분마다 공유 가능한 완료 단위를 확인**하고 새로 검증된 커밋을 묶어 push합니다.
미완성·실패 상태를 시간에 맞춰 강제로 올리지는 않습니다. 원격에 올릴 단계가 없다면
사용자에게 진행 범위와 남은 검사를 설명합니다. PR에는 완료·진행 중인 범위를 기록합니다.

push는 feature 브랜치에 하고 원격 SHA를 확인합니다. 공유한 커밋은 force-push로
재작성하지 않습니다. 후속 수정은 의미 있는 수정 커밋으로 묶습니다. 작은 변경을
재빨리 쌓기보다 공동 작업자가 실제로 확인할 수 있는 검증된 진행을 공유합니다.

## 리뷰 기준

- 변경 이유, 책임 영역, 확인한 내용이 PR에 적혀 있는가
- 불필요하게 여러 영역이나 기존 경로를 함께 변경하지 않았는가
- bridge 계약 변경이라면 각 언어 소비자와 differential test에 미칠 영향을 설명했는가
- `infra/engine-merged.js`를 고쳤다면 결과 동등성을 `infra/tools/perf/ab-cards.js` 등으로 확인했는가
- 구현 정확성은 변경 영역의 계약·오류·수치 검사로 확인했는가. 문서·바인딩·모델 형식 변경에 승률 기준을 일괄 적용하지 않는가
- 새 회귀 검사·fixture가 반복 가능한 중요한 계약을 지키며 기존 자료로 부족한 이유가 있는가. 일회성 버그의 큰 snapshot·로그·데이터 복사본을 영구 누적하지 않는가([검사 유지비 기준](docs/ENGINEERING-STANDARDS.md#기능-단위-구성과-검사-유지비))
- 기존 infra의 평가/모델 **승격**이라면 `infra/tools/lab/lab.js`에서 95% 신뢰구간 하한이 50%를 넘고 독립 재실행(`seed_offset`)에서도 같은 방향인가. 새 AlphaZero 승격 기준은 Phase 10에서 별도 결정한다
- 새 디렉터리와 생성물 예외에는 책임·이유가 있고, 규약 조정에는 규칙·이유·보완 검사·재검토 조건이 있는가
- 로컬 검사, CI 요청, 관측한 CI 성공, 실제 모델 성능을 구분했는가

## 생성물·WSL2·구조 검사

새 Windows 생성물은 `%APPDATA%\Accelerate`의 `build`, `cache`, `datasets`, `models`,
`runs`, `reports`, `tmp`에 모읍니다. CI는 `$RUNNER_TEMP/Accelerate`를 사용합니다.
WSL2 일반 개발은 허용하며 재생성 가능한 캐시·가상환경·중간 빌드는
`${XDG_CACHE_HOME:-$HOME/.cache}/accelerate`에 둘 수 있습니다. 보존할 산출물은 호스트
APPDATA를 변환한 Windows 루트로 내보내며 OS별 환경과 바이너리는 공유하지 않습니다.
장시간 작업의 종료 조건·자원·자식 프로세스 소유권과 안전한 정리는 개발 기준을 따릅니다.

Node 22로 다음 검사를 실행합니다(패키지 설치 불필요).

```text
node --test .github/scripts/check-repository-policy.test.mjs
node .github/scripts/check-repository-policy.mjs
git diff --cached --check
```

검사기는 stage된 정책과 Git index 메타데이터를 검사합니다. 로컬에서는 먼저 의도한
파일만 stage해야 합니다. 새 루트·생성물·10 MiB 초과 파일을 거부하며, 정확한 경로·이유를
등록한 작은 fixture 등의 예외만 허용합니다. 모든 파일 쓰기를 감독하는 도구는 아닙니다.

## 기존 infra 작업 시 주의사항

- `infra/` 내부 파일은 상대 경로로 연결되어 있으므로 꼭 필요한 경우가 아니면 이동하거나 이름을 바꾸지 않습니다.
- 손으로 만든 테스트 보드에는 `turnsTaken`, `actionsRemaining`, `moveCount`, `castlingCanceled`를 채웁니다.
- 클라우드 동시 작업은 최대 20개입니다. 무거운 워크플로는 수동으로 시작하고 자원을 조율합니다.
