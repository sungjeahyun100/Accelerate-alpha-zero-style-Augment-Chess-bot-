# Accelerate: 알파제로 스타일 증강체스 봇

증강체스(augmentchess.org, 카드와 특수 기물이 있는 체스 변형)를 위한 알파제로 방식 AI를 만드는 팀 프로젝트입니다.

> 이 문서는 틀입니다. `TODO`로 표시된 부분은 팀이 채워 주세요.

## 프로젝트 목표

- TODO: 이 봇이 달성하려는 목표(예: 사이트의 기존 AI를 이기는 것)와 성공 기준

## 현재 상태

- TODO: 지금 어디까지 됐는지(예: 규칙 엔진 연결, 정책 설계 중)

## 폴더 구조

| 경로 | 내용 |
|---|---|
| `infra/` | 기존 증강체스 엔진 프로젝트에서 가져온 재사용 도구(규칙 엔진, 자가대국, 인코딩, 대전/판정, 검증). 각 파일 설명은 [infra/FILE-GUIDE.md](infra/FILE-GUIDE.md) |
| `docs/` | 설계, 로드맵, 실험 기록, 결정 기록, 게임 규칙 요약 |
| `.github/workflows/` | 클라우드 자동화(GitHub Actions) |

## 시작하기

```bash
cd infra
npm install                    # 학습 코드(tfjs)를 쓸 때만 필요
node smoke-merged.js           # 통과하면 ALL SMOKE CHECKS PASSED
node tools/ci/golden-eval.js   # 평가 함수 회귀 검사
```

## 문서

- [CONTRIBUTING.md](CONTRIBUTING.md): 작업 방식(git flow, PR, 리뷰)
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md): 알파제로 설계
- [docs/ROADMAP.md](docs/ROADMAP.md): 단계별 계획
- [docs/EXPERIMENTS.md](docs/EXPERIMENTS.md): 실험 기록
- [docs/DECISIONS.md](docs/DECISIONS.md): 결정 기록
- [docs/GAME-RULES.md](docs/GAME-RULES.md): 게임 규칙 요약

## 팀

| 이름 | 역할 |
|---|---|
| TODO | TODO |

## 라이선스

TODO: 팀이 라이선스를 정한 뒤 작성. `infra/`의 출처와 원본 라이선스는 [NOTICE.md](NOTICE.md)를 참고하세요.
