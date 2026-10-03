# 변경 이력

> 배포(`release/*`)할 때마다 갱신합니다. 최신이 위입니다.

## 미배포

- 개발 환경의 정확한 경고·오류 전달, 제품 메시지와 상세 진단 구분, Fail-Operational 복구·영향 보고 기준 강화
- CI의 job·matrix 병렬화·자원 예산과 빌드·성공 검사 재사용, SHA 간 영향 입력 비교·변경 검사 재실행 기준 추가
- 일회성 CI의 공동 목표당 workflow·동시 실행 각 1개, 소유·정리 책임과 관측 근거 보존 기준 추가
- 일회성 CI의 일반 종료 run 최근 5개와 임시 artifact 기본 14일, 연구 근거의 별도 보존 기준 명시
- 작업 브랜치 재사용, 원격 보존된 완료 로컬 브랜치 정리와 PR 병합 후 원격 head 삭제 기준 명시
- 공동 연구 영수증의 GitHub 작성자·이름·공유·보존 기준과 범용 Markdown 템플릿 추가
- 저장소·에이전트 작업 규약, NASA/JPL 원칙 조정표와 메타데이터 구조 검사/CI 추가
- 논리 커밋·원격 공유 빈도, 기능 단위 SRP와 회귀 검사·fixture 유지비 기준 명시
- Windows 생성물 `%APPDATA%\Accelerate`, WSL2 재생성 캐시 예외와 실행 소유권 기준 정의
- PyO3/maturin 직접 호출·JSON 기록 계약과 ResNet/LoRA/FiLM·ONNX·Hypernetwork 확장 방향 문서화(구현 전)

- JS oracle, Rust engine, Python AI, bridge로 책임을 나눈 프로젝트 구조와 아키텍처 문서 추가
- `bridge/`, `rust-engine/`, `python/`, `tests/differential/`의 구현 전 skeleton 추가
- `infra/` 추가(기존 엔진 프로젝트의 재사용 도구)
- GitHub Actions 워크플로 추가
- CI를 `develop` 브랜치 push에도 적용, `CODEOWNERS`/`.gitignore` 추가
- JS↔Rust differential test 하네스 추가(`infra/tools/fixtures/run-differential.js`, `.github/workflows/differential.yml`) -- `rust-engine/`에 후보가 생기면 자동으로 실제 교차 검증 시작
- Dependabot 설정 추가(npm/github-actions/cargo)
- `pre_cpp_engine_code/`(Rust 포팅용 스케치)에 컴파일 체크 CI 추가
- `docs/CPP-ENGINE-GAPS.md` 추가: C++ 엔진 스케치에서 비어 있는 것, 사이트 동작 조사, 카드/기물 틀 제안, 다음 단계(문서만)
