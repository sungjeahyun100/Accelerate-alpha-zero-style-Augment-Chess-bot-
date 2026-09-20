# Accelerate: 알파제로 스타일 증강체스 봇

증강체스(augmentchess.org, 카드와 특수 기물이 있는 체스 변형)를 위한 AlphaZero 방식 AI를 만드는 팀 프로젝트입니다.

현재는 구현에 앞서 언어별 책임 경계와 검증 방향을 확정한 **Phase 0** 단계입니다. Rust 엔진, Python AI, 언어 간 bridge는 아직 구현되지 않았으며, 기존 JavaScript 코드는 규칙의 기준이 되는 oracle/reference implementation으로 보존합니다.

## 아키텍처

검증 관계는 다음과 같습니다.

```text
infra/ (JS oracle) ── differential validation ──> rust-engine/
```

실제 봇과 self-play의 실행 흐름은 다음과 같습니다.

```text
python/ (AI) ──> bridge/ ──> rust-engine/
```

- JavaScript oracle은 Rust 포팅의 정확성을 검증하는 기준이며 실제 AlphaZero 탐색 루프의 엔진이 아닙니다.
- Rust 엔진은 Python AI를 알지 않는 독립적인 규칙 엔진으로 설계합니다.
- Python은 공통 bridge 계약을 통해 Rust 엔진을 호출합니다.
- 같은 `GameState`와 `Action`을 JS oracle과 Rust 엔진에 입력하는 differential test로 동등성을 검증할 계획입니다.

자세한 내용은 [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)와 [docs/DECISIONS.md](docs/DECISIONS.md)를 참고하세요.

## 폴더 구조

| 경로 | 책임 | 현재 상태 |
|---|---|---|
| `bridge/` | JavaScript, Rust, Python 사이의 공통 상태·행동·요청/응답·직렬화 계약 | 문서와 빈 구조만 존재 |
| `rust-engine/` | 실제 봇 탐색과 self-play에 사용할 고성능 규칙 엔진 | 구현 전 |
| `infra/` | 기존 JS oracle과 검증/실험 인프라 | 기존 코드 보존 |
| `python/` | AlphaZero, MCTS, policy/value network, self-play, training, NNUE 연구 | 문서와 빈 구조만 존재 |
| `tests/differential/` | JS oracle과 Rust 엔진의 동등성 검증 | 계획 문서만 존재 |
| `docs/` | 아키텍처, 로드맵, 결정, 실험, 게임 규칙 문서 | 관리 중 |
| `.github/workflows/` | 기존 JS oracle과 실험 인프라용 GitHub Actions | 기존 경로 유지 |

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
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md): 영역별 책임과 의존성 방향
- [docs/ROADMAP.md](docs/ROADMAP.md): 단계별 구현 순서
- [docs/DECISIONS.md](docs/DECISIONS.md): 합의된 결정과 이유
- [docs/EXPERIMENTS.md](docs/EXPERIMENTS.md): 실험 기록
- [docs/GAME-RULES.md](docs/GAME-RULES.md): 게임 규칙 요약

## 프로젝트 목표와 팀

- 성공 기준: TODO — 팀 합의 후 구체화
- 팀과 역할: TODO — 팀 구성 후 기록

## 라이선스

저장소 전체의 라이선스는 아직 정하지 않았습니다. `infra/`의 출처와 원본 라이선스는 [NOTICE.md](NOTICE.md)를 반드시 확인하세요.
