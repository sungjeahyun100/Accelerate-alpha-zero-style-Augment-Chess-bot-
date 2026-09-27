# Accelerate: 알파제로 스타일 증강체스 봇

증강체스(augmentchess.org, 카드와 특수 기물이 있는 체스 변형)를 위한 AlphaZero 방식 AI를 만드는 팀 프로젝트입니다.

동결 사이트 client를 실행하는 offline oracle, 순수 Rust 규칙 엔진의 일부 기능,
PyO3/maturin 패키지, 공개 관측 인코딩과 ResNet·FiLM·LoRA·ONNX 추론이 구현됐습니다.
공개 정보 기반 탐색·replay·유한 CLI를 연결하는 중이며 전체 규칙 coverage와 최종 통합은
완료되지 않았습니다. 전체 판정은 **NO-GO**이고, 관측한 검사와 남은 코드 조건은
[docs/IMPLEMENTATION.md](docs/IMPLEMENTATION.md)에 기록합니다. 실제 학습은 이번 구현 범위에서 제외합니다.

## 아키텍처

검증 관계는 다음과 같습니다.

```text
infra/ (JS oracle) ── differential validation ──> rust-engine/
```

실제 봇과 self-play의 실행 흐름은 다음과 같습니다.

```text
python/ (AI) ──> bridge/ (PyO3 직접 호출, maturin 패키징) ──> rust-engine/
```

- JavaScript oracle은 Rust 포팅의 정확성을 검증하는 기준이며 실제 AlphaZero 탐색 루프의 엔진이 아닙니다.
- Rust 엔진은 Python·PyO3·신경망·ONNX runtime에 의존하지 않습니다.
- Python은 bridge의 immutable Position/Action과 직접 호출 경계를 통해 Rust 엔진을 사용합니다.
- JSON은 저장·교환·검증 계약이며 반복 호출은 PyO3 타입·owned 배열을 사용합니다.
- ResNet의 FiLM 조건은 ONNX 입력/그래프에 남고, static LoRA는 별도 어댑터로 보존하며 복사본에서 병합합니다.
- 운영 ONNX 추론은 Rust의 기본 `ort`와 명시 선택 `tract`를 사용합니다.
- 과거 JS fixture harness와 현재 동결 client↔Rust 비교는 범위가 다르며, 전체 Rust 정답 비교는 진행 중입니다.

자세한 내용은 [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)와 [docs/DECISIONS.md](docs/DECISIONS.md)를 참고하세요.

## 폴더 구조

| 경로 | 책임 | 현재 상태 |
|---|---|---|
| `bridge/` | 공통 JSON 계약·얇은 PyO3 연동·독립 ONNX runtime crate | v1 계약, Linux wheel과 실제 ort/tract 검사 checkpoint |
| `rust-engine/` | 봇 탐색과 self-play용 독립 규칙 엔진 | source 포팅 진행, 전체 catalog coverage 미완료 |
| `infra/` | JS oracle과 검증/실험 인프라 | 기존 코드와 동결 client adapter |
| `python/` | 공개 관측·ISMCTS·ResNet·LoRA·FiLM·replay·학습/평가 코드 | 단일 accelerate_chess 패키지에서 구현·통합 진행 |
| `tests/differential/` | JS oracle과 Rust 엔진의 동등성 검증 | fixture·하네스 존재, 실제 Rust 비교 전 |
| `pre_cpp_engine_code/` | Rust 포팅 참고용 C++ 초안 | 일부 규칙 골격·자체 검사 존재 |
| `docs/` | 아키텍처, 로드맵, 결정, 실험, 게임 규칙 문서 | 관리 중 |
| `.github/workflows/` | 구조·native 패키지·규칙 검증과 기존 실험 Actions | Windows/Linux native CI 추가, 최종 통과 미관측 |

새 생성물은 `%APPDATA%\Accelerate`에 모읍니다. WSL2 일반 개발을 허용하고 재생성
가능한 Linux 캐시·가상환경·중간 빌드만 별도 루트에 둡니다. 자세한 경로와 실행 기준은
[AGENTS.md](AGENTS.md)에 있습니다. 구조 검사(Node 22, 의도한 파일을 먼저 stage):

```text
node --test .github/scripts/check-repository-policy.test.mjs
node .github/scripts/check-repository-policy.mjs
```

## 기존 JavaScript 인프라 확인

현재 JavaScript oracle과 관련 도구는 경로 호환성을 위해 `infra/` 안에 그대로 둡니다.

```bash
cd infra
npm install                    # tfjs 기반 기존 학습 도구를 쓸 때만 필요
node smoke-merged.js           # 통과하면 ALL SMOKE CHECKS PASSED
node tools/ci/golden-eval.js   # 평가 함수 회귀 검사
```

각 파일의 역할과 주의사항은 [infra/FILE-GUIDE.md](infra/FILE-GUIDE.md)에 있습니다.

## 문서

- [CONTRIBUTING.md](CONTRIBUTING.md): Gitflow, PR, 리뷰 규칙
- [AGENTS.md](AGENTS.md): 에이전트 작업·생성물·WSL2 규약
- [docs/ENGINEERING-STANDARDS.md](docs/ENGINEERING-STANDARDS.md): NASA/JPL 원칙의 조정과 경계별 검증 기준
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md): 영역별 책임과 의존성 방향
- [docs/ROADMAP.md](docs/ROADMAP.md): 단계별 구현 순서
- [docs/DECISIONS.md](docs/DECISIONS.md): 합의된 결정과 이유
- [docs/IMPLEMENTATION.md](docs/IMPLEMENTATION.md): 코드 완료 조건·실제 checkpoint·남은 구현
- [docs/EXPERIMENTS.md](docs/EXPERIMENTS.md): 실험 기록
- [docs/GAME-RULES.md](docs/GAME-RULES.md): 게임 규칙 요약

## 프로젝트 목표와 팀

- 성공 기준: TODO — 팀 합의 후 구체화
- 팀과 역할: TODO — 팀 구성 후 기록

## 라이선스

저장소 전체의 라이선스는 아직 정하지 않았습니다. `infra/`의 출처와 원본 라이선스는 [NOTICE.md](NOTICE.md)를 반드시 확인하세요.
