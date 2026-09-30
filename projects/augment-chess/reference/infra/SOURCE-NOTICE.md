# NOTICE — 라이선스 적용 범위와 제3자 저작물

이 저장소는 augmentchess.org와 무관한 **비공식 개인 프로젝트**입니다. 저장소에는 성격이 다른 자료가 섞여 있어서,
[`LICENSE`](LICENSE)(CC BY-NC-ND 4.0)가 어디에 적용되는지 여기에 정리합니다.

## 1. `LICENSE`가 적용되는 부분 (직접 작성한 것)

| 경로 | 내용 |
|---|---|
| `tools/` | CI 검사, 사이트 대조, 속도 검증 도구 |
| `nnue/` | 학습·평가·대전 스크립트, 설정, 모델 가중치 |
| `extension/` 중 `content.js`, `analysis.js`, `hybrid.js`, `ai-override.js`, `ext-bridge.js`, `nnue.js`, `searchWorker.js`, `style.css`, `manifest.json`, `icons/` 등 확장 프로그램 코드 | 확장의 화면·연동 코드와 직접 만든 분석 아이콘 |
| `.github/` | 워크플로 |
| `selfplay-run-merged.js`, `selfplay-worker-merged.js`, `smoke-merged.js` | 자기대국 실행기와 테스트 |
| 문서 (`README.md`, `PROJECT.md`, `TODO.md`, `HANDOFF.md`, `NOTICE.md`) | 설명 문서 |

조건 요약: 출처 표시, **비영리**, **수정 없이 원본 그대로**만 공유할 수 있습니다.
(GitHub의 서비스 약관에 따라 공개 저장소를 포크해 보는 것은 GitHub 안에서 별도로 허용되지만, 수정본을 배포하는 것까지 이 라이선스가 허락하는 것은 아닙니다.)

## 2. `LICENSE`가 적용되지 **않는** 부분

### augmentchess.org에서 파생된 자료
사이트의 이용약관 제7조는 게임 시스템, 규칙, 카드, UI, 디자인, 이미지, 코드에 관한 권리가 운영자 또는 정당한 권리자에게 있다고 정하고 있습니다.
아래 자료는 사이트의 코드·규칙·데이터에서 파생되었거나 그 사본이므로, 이 저장소의 라이선스가 아니라 **해당 권리자의 권리**를 따릅니다.

| 경로 | 설명 |
|---|---|
| `engine-merged.js`, `extension/engine.js` | 사이트 AI 워커 코드를 바탕으로 규칙을 맞추고 수정한 엔진 |
| `legacy/engine.optimized.js` 및 `legacy/`의 관련 스크립트 | 옛 엔진 |
| `site-oracle/` | 사이트 번들의 사본과 그것을 Node에서 실행하기 위한 보조 파일 |
| `audit-data/` | 사이트의 카드·기물 목록을 대조한 자료 |
| 카드 이름·기물 이름·규칙 목록 | `nnue/encode.js`, `extension/nnue.js` 안의 목록 등 |

**운영자의 동의**: `site-oracle/`의 번들 사본(사이트 AI 워커 포함)을 오라클로 보관하면서 GitHub 저장소에 올리는 것, 그리고 이 문서로 출처와 권리 관계를 밝히는 것에 대해 운영자(개발자)에게 문의했고 디스코드에서 괜찮다는 답을 받았습니다(2026-09-19). 이 저장소의 엔진은 그 번들의 AI 워커를 바탕으로 만든 것이며, 비슷한 사례(사이트 엔진을 추출해 다른 언어로 이식해 GitHub에 게시)에서도 운영자가 동의한 바 있습니다. 이 동의는 사이트 자료에 대한 권리가 운영자에게 있다는 점과 그 자료가 이 저장소의 `LICENSE` 대상이 아니라는 점을 바꾸지 않습니다.

### Stockfish (GPL-3.0)
`extension/stockfish/`의 Stockfish.js 18은 **GNU GPL v3**로 배포되는 별개의 프로그램이며, 이 프로젝트의 라이선스와 무관합니다.
라이선스 전문은 `extension/stockfish/COPYING-GPL-3.0.txt`에 있고, 소스는 아래에서 받을 수 있습니다.

- Stockfish.js: https://github.com/nmrugg/stockfish.js
- Stockfish: https://github.com/official-stockfish/Stockfish

### 체스 기물 아이콘
사이트가 쓰는 대부분의 기물 아이콘은 Colin M.L. Burnett의 작품(CC BY-SA 3.0)이며 일부는 CC BY-SA 4.0입니다
([사이트의 아이콘 라이선스 안내](https://augmentchess.org/iconlicense/)). `site-oracle/`의 번들 사본에 이 아이콘이 포함되어 있고, 각 아이콘은 위 라이선스를 따릅니다.

### 의존 패키지
`package.json`의 패키지(TensorFlow.js 등)는 각자의 라이선스를 따릅니다.
