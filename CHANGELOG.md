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
- rust-engine/src/lib.rs에 pre_cpp_engine_code/engine.cpp 초안 포팅 추가 (Cargo.toml은 의도적으로 보류, 이유는 rust-engine/README.md 참고)
