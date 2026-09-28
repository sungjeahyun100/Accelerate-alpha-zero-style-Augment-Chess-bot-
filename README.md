# Accelerate: 알파제로 스타일 증강체스 봇

증강체스(augmentchess.org, 카드와 특수 기물이 있는 체스 변형)를 위한 AlphaZero 방식 AI를 만드는 팀 프로젝트입니다.

현재 JS oracle, JSON bridge 초안·예시 검사, 비교 fixture·하네스와 C++ 포팅 참고 초안이 있습니다.
Rust 엔진·Python AlphaZero·PyO3/maturin 패키지·ResNet/ONNX는 구현 전입니다.
저장소·에이전트 규약과 새 ML 연동 설계를 추가하며, 설계 채택과 구현 완료를 구분합니다.

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
- Rust 엔진은 Python AI를 알지 않는 독립적인 규칙 엔진으로 설계합니다.
- Python은 공통 bridge 계약을 통해 Rust 엔진을 호출합니다.
- JSON은 저장·교환·검증 계약으로 보존하고 반복 호출은 PyO3 타입·배열을 사용하도록 설계합니다.
- ResNet은 FiLM 조건화와 별도 LoRA 적응을 사용하고, 검증용 정적 병합과 ONNX export를 계획합니다. FiLM 조건은 ONNX 입력입니다.
- differential 하네스는 존재하지만 Rust 후보가 없어 현재 CI는 JS 자체 회귀입니다.

자세한 내용은 [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)와 [docs/DECISIONS.md](docs/DECISIONS.md)를 참고하세요.

## 폴더 구조

| 경로 | 책임 | 현재 상태 |
|---|---|---|
| `bridge/` | 공통 상태·행동·JSON 계약과 얇은 PyO3 연동 | JSON 초안·스키마·예시 검사 존재, 바인딩 구현 전 |
| `rust-engine/` | 실제 봇 탐색과 self-play에 사용할 고성능 규칙 엔진 | 구현 전 |
| `infra/` | 기존 JS oracle과 검증/실험 인프라 | 기존 코드 보존 |
| `python/` | AlphaZero, MCTS, policy/value network, self-play, training, NNUE 연구 | 문서와 빈 구조만 존재 |
| `tests/differential/` | JS oracle과 Rust 엔진의 동등성 검증 | fixture·하네스 존재, 실제 Rust 비교 전 |
| `pre_cpp_engine_code/` | Rust 포팅 참고용 C++ 초안 | 일부 규칙 골격·자체 검사 존재 |
| `docs/` | 아키텍처, 로드맵, 결정, 실험, 게임 규칙 문서 | 관리 중 |
| `.github/workflows/` | 기존 JS oracle과 실험 인프라용 GitHub Actions | 기존 경로 유지 |

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

- [monitoring/README.md](monitoring/README.md): 모델 loss 모니터 설치·입력 형식·실행·결과 확인
- [CONTRIBUTING.md](CONTRIBUTING.md): Gitflow, PR, 리뷰 규칙
- [AGENTS.md](AGENTS.md): 에이전트 작업·생성물·WSL2 규약
- [docs/ENGINEERING-STANDARDS.md](docs/ENGINEERING-STANDARDS.md): NASA/JPL 원칙의 조정과 경계별 검증 기준
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md): 영역별 책임과 의존성 방향
- [docs/ROADMAP.md](docs/ROADMAP.md): 단계별 구현 순서
- [docs/DECISIONS.md](docs/DECISIONS.md): 합의된 결정과 이유
- [docs/EXPERIMENTS.md](docs/EXPERIMENTS.md): 실험 기록
- [docs/research/TEMPLATE.md](docs/research/TEMPLATE.md): 공동 연구 영수증의 범용 Markdown 템플릿
- [docs/GAME-RULES.md](docs/GAME-RULES.md): 게임 규칙 요약
- [docs/CPP-ENGINE-GAPS.md](docs/CPP-ENGINE-GAPS.md): C++ 엔진(`pre_cpp_engine_code/`)에서 비어 있는 것과 다음 단계 제안

## 프로젝트 목표와 팀

- 성공 기준: TODO — 팀 합의 후 구체화
- 팀과 역할: TODO — 팀 구성 후 기록

## 라이선스

저장소 전체의 라이선스는 아직 정하지 않았습니다. `infra/`의 출처와 원본 라이선스는 [NOTICE.md](NOTICE.md)를 반드시 확인하세요.
