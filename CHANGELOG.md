# 변경 이력

> 배포(`release/*`)할 때마다 갱신합니다. 최신이 위입니다.

## 미배포

- JS oracle, Rust engine, Python AI, bridge로 책임을 나눈 프로젝트 구조와 아키텍처 문서 추가
- `bridge/`, `rust-engine/`, `python/`, `tests/differential/`의 구현 전 skeleton 추가
- `infra/` 추가(기존 엔진 프로젝트의 재사용 도구)
- GitHub Actions 워크플로 추가
- CI를 `develop` 브랜치 push에도 적용, `CODEOWNERS`/`.gitignore` 추가
- JS↔Rust differential test 하네스 추가(`infra/tools/fixtures/run-differential.js`, `.github/workflows/differential.yml`) -- `rust-engine/`에 후보가 생기면 자동으로 실제 교차 검증 시작
- Dependabot 설정 추가(npm/github-actions/cargo)
- `pre_cpp_engine_code/`(Rust 포팅용 스케치)에 컴파일 체크 CI 추가
- `docs/CPP-ENGINE-GAPS.md` 추가: C++ 엔진 스케치에서 비어 있는 것, 사이트 동작 조사, 카드/기물 틀 제안, 다음 단계(문서만)
