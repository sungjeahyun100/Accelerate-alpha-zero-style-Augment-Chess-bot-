# 사이트 AI worker 비교 도구

이 디렉터리는 공개 사이트의 AI worker와 참고용 JS 엔진을 비교한다. 동결된
메인 클라이언트의 v7 전체 규칙 검증이나 Rust 엔진의 완료 증거로 해석하지 않는다.
실행 명령은 저장소 루트 기준이다.

```text
node projects/augment-chess/oracle/tools/site-parity/fetch-real-worker.js [--force]
node projects/augment-chess/oracle/tools/site-parity/parity-actions.js [engine|-] [N=400] [seed=12345]
node projects/augment-chess/oracle/tools/site-parity/parity-apply.js [engine|-] [N=300] [seed=777]
node projects/augment-chess/oracle/tools/site-parity/parity-playout.js [engine|-] [GAMES=30] [seed=4242] [PLIES=40]
node projects/augment-chess/oracle/tools/site-parity/check-site-update.js [--save]
```

`engine` 생략 또는 `-`는
`projects/augment-chess/reference/infra/engine-merged.js`를 사용한다.
`fetch-real-worker.js`는 사이트의 현재 worker를 도구 소유 `.cache/`에 보관하며,
`--force`가 없고 캐시가 있으면 다시 받지 않는다. `check-site-update.js`는
공개 사이트의 bundle 이름과 worker SHA-256을 `last-seen.json`과 비교한다.
`--save`는 검토 후에만 사용한다. 현재 worker와 동결 v7 메인 클라이언트를
같은 규칙 버전으로 간주하지 않는다.

`.github/workflows/site-watch.yml`은 이 도구를 정기·수동 실행해 결과를
요약과 임시 artifact에 남긴다. 원격 자료의 변경은 그 자체로 카탈로그 승격이나
엔진 구현 완료를 뜻하지 않는다.

다중 수 차이를 해석할 때는 다음 경계를 확인한다.

- 사이트 worker는 일부 카드 효과를 모델링하지 않는다. 예를 들어 `locustSwarm`은
  OPENING 준비 경로에서만 반영되므로 대국 중 카드 적용 비교에 차이가 날 수 있다.
- Brutus 룩, Freeze처럼 무작위 대상을 고르는 효과는 두 코드베이스의 RNG가
  일치하도록 설정되지 않았다. 단독 기물 차이만으로 규칙 불일치를 판정하지 않는다.
- worker는 입력 state에 `parrotMovement`가 있을 때만 앵무새 기억을 추적한다.
  과거 분류와 재현 조건은 [TRIAGE.md](TRIAGE.md)를 참고한다.
