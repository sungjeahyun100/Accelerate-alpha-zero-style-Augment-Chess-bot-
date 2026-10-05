# 연구 영수증: 메모리 최적화 후보 조사표

## 식별과 출처

| 항목 | 기록 |
|---|---|
| 작성 시점 | 2026-10-05 19:51 UTC |
| 마지막 정정 시점 | 해당 없음: 최초 초안 |
| GitHub 작성자·공동 작성자 | [sungjeahyun100](https://github.com/sungjeahyun100); 공동 작성자 없음 |
| 관련 PR·이슈 | 별도 문서 PR; 번호는 PR 본문에서 확인. 측정 인프라 [PR #39](https://github.com/sungjeahyun100/Accelerate-alpha-zero-style-Augment-Chess-bot-/pull/39)와 분리 |
| 저장소·기준 commit SHA | `sungjeahyun100/Accelerate-alpha-zero-style-Augment-Chess-bot-` `3552b96fb276dd59e805bc09cf80c975a4525474` (`develop`) |
| 미커밋 변경 | 소스 조사 시 없음. 이 문서만 신규 작성 |
| 자료 유형·관측 범위 | 소스 조사. 메모리·속도 측정, CI 실행, 모델 성능 검증은 미실행 |

## 목적과 판단 범위

동시 프로세스 실행 시 보고된 OOM의 원인을 이 문서만으로 확정할 수 없다. 아래의 효과 등급은 **측정치가 아닌 가설**이다. `High`는 해당 경로의 메모리가 실제로 지배적일 때 개선 상한이 크다는 뜻이며, 전체 프로세스 RSS의 예측값이 아니다. 우선순위는 후속 계측·프로토타입의 순서이다. 어떤 기법도 채택 결정이 아니다.

`P0`: 작은 변경으로 낭비를 확인·제거할 후보. `P1`: 큰 효과가 가능하고 검증 비용이 현실적인 후보. `P2`: 구조·계약 변경 또는 큰 검증이 필요한 후보. `P3`: 현 단계의 근거가 부족하거나 위험·비용이 큰 후보. `Low/Medium/High`는 다른 후보와의 상대적 예상이며, `조건`과 `부적합`을 함께 읽어야 한다.

### 현재 소스에서 확인한 경로

| 관측 사실 | 메모리 가설·주의점 |
|---|---|
| [`GameState`](../../projects/augment-chess/engine/src/state.rs)는 `Vec<Vec<Option<Piece>>>` 보드, 덱·포획·이력과 `serde_json::Value` 기반 `extra`를 소유한다. `Piece`도 `String`, `Fields`, `source_order`를 소유한다. | 상태 깊은 복제는 셀뿐 아니라 문자열·JSON 트리까지 복제할 수 있다. 보드만 작게 바꿔도 전체 상태 비용이 사라지지는 않는다. |
| [`V7HostPosition`](../../projects/augment-chess/engine/src/v7_host.rs)의 상태·공간 상태·원본/기준 JSON은 `Arc`로 공유된다. 어댑터의 `begin_transaction`은 Position을 `clone`한다. | `Arc` handle 복사를 `GameState` 깊은 복사로 동일시하면 안 된다. 내부 `Arc::make_mut`, import/export, 전이 경계의 실제 복제를 별도 계측해야 한다. |
| [`SourceActionCursor::new`와 `accepts`](../../projects/augment-chess/engine/src/v7_action_surface.rs)는 상태를 소유 복제하고 각 후보 검증에서 상태를 복제한다. [`V7PublicActionCursor::next_page`](../../projects/augment-chess/engine/src/v7_adapter_actions.rs)는 cursor를 복제한 뒤 commit한다. | 후보 수에 비례하는 할당·복제 가능성이 있다. cursor 복제는 실패 시 원자성 때문에 존재할 수 있으므로 단순 제거는 안전하지 않다. |
| [`legal_public_intents`](../../projects/augment-chess/engine/src/v7_adapter_actions.rs)는 전체 후보의 JSON intent와 canonical byte를 만들어 `BTreeSet`으로 중복을 제거한다. | eager 결과와 dedup buffer의 최고치가 동시에 살아 있을 수 있다. 순서·동일성·상한을 유지하며 측정해야 한다. |
| [`_SearchState`, `_Node`, `_Edge`](../../projects/accelerate/python/accelerate_chess/search.py)는 Python 사전과 JSON형 intent를 보관하고 `max_nodes`, `max_edges`로 제한한다. 입자 목록, 동시에 대기하는 leaf 요청, 관측 trace도 별도로 존재한다. | 현재 노드에 `GameState`를 직접 보관하지 않는다. 트리 크기와 입자·leaf batch의 **동시** 메모리를 구분해야 한다. 현재 코드는 별도 self-play worker 실행기를 보여 주지 않으며, leaf batch도 같은 스레드의 협력적 simulation이다. |
| [`batch_positions`](../../projects/accelerate/python/accelerate_chess/encoding.py)는 `np.stack`으로 board/condition을 만들고 최대 후보 수에 맞춘 padded feature/mask를 만든다. [PyO3 bridge](../../projects/accelerate/native/src/lib.rs)는 action마다 payload·Python 객체를 생성한다. | 임시 배열과 변환 중 복사, 원본·batch·결과의 동시 생존이 peak를 올릴 수 있다. ONNX runtime 내부 메모리는 이 소스 조사만으로 단정할 수 없다. |
| 기존 [`allocation-probe`](../../projects/augment-chess/perf/README.md)는 allocator 요청 수·요청 byte를 기록한다. | live byte/RSS나 Python·ONNX를 재지 않는다. PR #39의 측정 코드도 이 문서 PR에 가져오지 않는다. |

## 측정 규칙

각 행의 `검증`은 같은 입력·seed·규칙/profile, 동일 worker/leaf batch, 같은 빌드와 반복 조건에서 전후 비교한다. 최소한 peak RSS(전체 프로세스와 필요한 자식), steady RSS, allocator 호출·요청 byte·live byte, 시간/노드, nodes/sec, retained nodes/edges, action당 allocation, 상태 복제 시간·byte를 구분한다. `Vec` capacity와 arena가 반환하지 않은 보유 메모리도 기록한다. `perf`, `/usr/bin/time -v`, `/proc/<pid>/smaps_rollup`·`status`, heap profiler 또는 기존 native allocation probe를 용도에 맞춰 사용한다. Python은 `tracemalloc`과 native RSS를 함께 본다. `tracemalloc`만으로 Rust/NumPy/ONNX의 전체 할당량을 판단하지 않는다. 초기에 OOM이 난 공동 실행 조건은 재현 예산을 제한하고 단독 실행과 별도로 비교한다. 게임 동등성·공개 정보 경계·정확한 action 순서/identity·오류 복구를 성능 수치와 같이 확인한다.

표의 `메모리`는 peak/steady 예상, `속도`는 CPU·locality 예상이다. `오버헤드·경계`는 초기화, reset/free, thread, Python/ONNX 영향까지 포함한다. 행에서 언급하지 않은 bridge 영향은 Rust 내부 한정 시 직접 영향 없음이며, 공개 DTO·action 순서·tensor 계약을 바꾸는 후보는 별도 계약 검증을 요구한다. `저/중/고` 위험도는 정확성·수명·동시성·borrow checker 구현 난이도를 합친 상대 등급이다.

### 1. 낮은 위험·즉시 검토 가능

| 기법 | 대상 | 해결하려는 문제 | 예상 메모리 효과 (peak/steady) | 예상 성능 효과 (CPU/locality) | 추가 오버헤드 | 구현 난이도 | 위험도 | 적용 조건 | 부적합 조건 | 검증 방법 | 현재 프로젝트 적합성 | 우선순위 | 비고 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 불필요한 `Clone` 제거·borrow/move | `accepts`, action 포장, JSON projection | 반복 깊은 복사 | 높음/중간 가능 | 향상 가능 | borrow 범위·오류 원자성 검토; thread·bridge 계약 유지 | 중 | 중 | 복제물이 읽기 전용이거나 소유권 이전 가능 | 원본을 변경하거나 rollback 보장이 필요한 clone | clone당 byte·시간, legal action parity | 높음 | P0 | `Arc` handle clone과 깊은 clone 분리 |
| `Vec::with_capacity`·`reserve` | legal intent/page/result, Python 전 단계 Rust Vec | 재할당 | 낮음/중립 | 향상 가능 | 과대 예약은 peak·steady 증가; 초기 예약 비용 | 저 | 저 | 후보 상한·분포가 알려짐 | 희소·조기 종료가 흔함 | 재할당 수, capacity/len, RSS | 높음 | P0 | [Rust Vec 문서](https://doc.rust-lang.org/std/vec/struct.Vec.html) |
| 임시 Vec/버퍼 재사용 | action canonical byte, per-page scratch | 반복 할당 | 중간/중립~증가 | 향상 가능 | clear/drop 뒤 큰 capacity 잔류; session 간 공유 시 lock 비용 | 중 | 중 | 명확한 호출 경계와 최대 보유량 | 중첩 호출·비동기 수명·큰 outlier | alloc/action, warm RSS, reset RSS | 높음 | P0 | 재사용 뒤 큰 버퍼 shrink 기준 필요 |
| serialization buffer 재사용 | JCS/digest, host import/export | JSON byte 임시 할당 | 중간/중립~증가 | 향상 가능 | canonical 순서·hash 불변; 재진입성 | 중 | 중 | 한 호출 내 buffer 소유가 명확 | 동시에 여러 응답이 buffer 참조 | alloc/serialize, digest parity | 높음 | P0 | JSON `Value` 트리 비용은 별개 |
| eager 대신 기존 cursor/page 활용 | 전체 `legal_public_intents` 호출자 | 모든 action 동시 보유 | 높음/중간 가능 | 페이지 반복으로 저하 가능 | 순서·examined budget·오류·Python API 확인 | 중 | 중 | 소비자가 일부 후보만 필요 | 전체 행동 집합·동일 순서가 필수 | peak/action, pages, completeness | 중 | P1 | cursor는 이미 존재; 호출자별 판단 |
| 중간 tensor 생존 기간 단축 | `batch_positions`, `_evaluate_many` | encode·stack·출력 동시 생존 | 중간/낮음 | 향상 또는 중립 | Python 참조 수명·native evaluator 입력 수명 확인 | 중 | 중 | 평가 직후 원본 임시 배열 해제 가능 | 결과가 원본 feature를 참조 | batch peak RSS, shape/dtype parity | 높음 | P0 | GPU/ONNX 별도 계측 |

### 2. 자료구조 최적화

| 기법 | 대상 | 해결하려는 문제 | 예상 메모리 효과 (peak/steady) | 예상 성능 효과 (CPU/locality) | 추가 오버헤드 | 구현 난이도 | 위험도 | 적용 조건 | 부적합 조건 | 검증 방법 | 현재 프로젝트 적합성 | 우선순위 | 비고 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| compact struct layout·padding 순서 | Rust `Action`, `Piece`, cursor | padding | 낮음/낮음 | locality 향상 가능 | 내부 레이아웃만; serde/API/FFI layout 확인 | 중 | 중 | `size_of`와 heap profile에서 실익 확인 | dynamic JSON/String이 지배 | size/alignment, bytes/action | 중 | P2 | 필드 순서만으로 heap payload 감소 없음 |
| enum·정수 폭 축소 | 좌표·카운터·stage | 과대 필드 | 낮음/낮음 | cache 개선 가능 | 범위·overflow 검증; wire 정수 의미 보존 | 중 | 중 | 실제 상한 증명 | JS 안전정수·장기 카운터 축소 위험 | size, range/property 검사 | 중 | P2 | 공개 schema 변경은 별도 PR |
| bit packing·bitboard | occupancy·hazard·board query | 반복 bool/좌표 저장 | 중간/중간 가능 | query 향상 또는 decode 저하 | variant 규칙·다중 셀·상태 동기화 비용 | 고 | 고 | query profile이 지배적 | 복잡한 `extra`와 보드 JSON이 대부분 | memory/query, 규칙 차분 | 낮음 | P3 | 단순 8×8 체스 가정 금지 |
| dense/sparse 표현 선택 | `SpatialState`, constraint set | 희소 자료의 dense 비용 또는 역 | 중간/중간 | 접근 패턴 의존 | 변환·metadata·clone 비용 | 중 | 중 | 밀도 분포·접근 빈도 측정 | 양쪽 표현 동시 보유가 peak 증가 | bytes/state, lookup 시간 | 중 | P2 | 단일 표현 강제 전 분포 확인 |
| `SmallVec`·inline storage | 작은 타깃/이동 목록 | 짧은 Vec heap | 낮음/중간 가능 | 작은 목록 향상 가능 | inline 크기만큼 owner 커짐; 긴 tail은 spill | 중 | 중 | 크기 분포가 매우 작고 owner 수가 많음 | 큰 목록·큰 owner·깊은 clone | len histogram, size, RSS | 중 | P2 | [smallvec 문서](https://docs.rs/smallvec/latest/smallvec/) |
| 고정 배열·stack storage | 정형 8×8 보조 버퍼 | heap 할당 | 낮음/중간 가능 | locality 향상 가능 | stack 사용·초기화 비용; 보드 의미 보존 | 중 | 중 | 크기가 계약상 고정된 내부 scratch | 가변 보드/큰 타입·깊은 재귀 | alloc/query, stack, parity | 중 | P2 | `GameState` wire 보드 변경은 고위험 |
| 문자열 interning·공유 ID | `Piece.kind/id`, card ID, action key | 반복 문자열 | 중간/중간 가능 | 비교 향상·lookup 저하 가능 | interner 생존·lock·메타데이터; JSON 재구성 | 고 | 중 | 중복률과 수명 공유 확인 | 대부분 고유 ID·공유 테이블이 무한 성장 | unique ratio, retained bytes | 중 | P2 | 공개 문자열 값 불변 |
| immutable data `Arc` 공유 | catalog·규칙·불변 원본 | worker/position당 복제 | 중간/중간 가능 | refcount 비용·locality 혼합 | 원자 refcount; mutable 상태 공유 금지 | 중 | 중 | 동일 불변 객체가 복제됨 | 변형·짧은 수명·작은 객체 | strong_count, bytes/session | 중 | P1 | `V7HostPosition`에서 일부 이미 사용 |
| copy-on-write (`Arc::make_mut`) | Position의 일부 공유 substate | 항상 깊은 clone | 중간/중간 가능 | 읽기 향상·쓰기 시 복제 spike | refcount·alias·write 시 peak; transaction rollback | 고 | 고 | 읽기 대 쓰기 비율 높음 | 대부분 후보가 상태를 변형 | copy-on-write 횟수·bytes | 중 | P2 | 기존 Arc 사용과 효과 구분 |
| AoS ↔ SoA | 대량 action metadata/edge 통계 | padding·cache miss | 중간/중간 | 순회 향상, 개별 접근 저하 | 동기화·인덱스 관리; Python/Rust 경계 변환 | 고 | 중 | 동종 필드의 대량 순회 | JSON형 가변 action 중심 | bytes/edge, nodes/sec | 낮음 | P3 | Python dict 트리에는 직접 적용 불가 |
| 공통 JSON/불변 부분 분리 | `GameState.extra`, source shape | 매 state의 반복 JSON | 높음/중간 가능 | projection 비용 증가 가능 | source-preserving identity·serialization 확인 | 고 | 고 | 여러 상태가 같은 큰 부분 공유 | 매 transition마다 그 부분이 변함 | clone byte, canonical parity | 중 | P2 | semantic contract 강함 |

### 3. MCTS·탐색 전용

| 기법 | 대상 | 해결하려는 문제 | 예상 메모리 효과 (peak/steady) | 예상 성능 효과 (CPU/locality) | 추가 오버헤드 | 구현 난이도 | 위험도 | 적용 조건 | 부적합 조건 | 검증 방법 | 현재 프로젝트 적합성 | 우선순위 | 비고 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 현재 node/edge 보유량·key 계측 | Python `nodes`, `edges` | per-entry 비용 불명 | 측정만 | 계측 시 저하 | 임시 계측 데이터 제한; thread 영향 없음 | 저 | 저 | 실제 탐색 입력 준비 | 샘플을 전체 게임으로 일반화 | bytes/node·edge, retained count | 높음 | P0 | 현재 노드에 state 없음 |
| edge intent/key 중복 완화 | `_Edge.intent`, `edges` key, `intents` | JSON dict+canonical string 중복 | 중간/중간 가능 | encode 재계산 시 저하 | 공개 intent 정확성·순서, Python 참조 수명 | 중 | 중 | 중복 보유가 profile로 확인 | re-encode가 hot path 지배 | bytes/edge, nodes/sec, intent parity | 높음 | P1 | key 유일성 보장 유지 |
| index 기반 graph·contiguous storage | Python node/edge 사전 또는 미래 Rust 트리 | per-object/hash 오버헤드 | 중간/중간 가능 | locality 향상·lookup 비용 | index 유효성, generation, borrow/API 변경 | 고 | 고 | profile상 graph overhead 큼 | 정보집합 키 lookup이 지배·작은 트리 | bytes/node, lookup, correctness | 중 | P2 | Rust `Box` pointer graph는 현재 없음 |
| transposition·node dedup | 정보집합 key 기반 tree | 중복 node | 중간/중간 가능 | 탐색 향상 또는 hash 비용 | collision·actor·history·정보 누출 검증 | 고 | 고 | 같은 정보 상태 재방문 많음 | 서로 다른 belief/history 합치면 오류 | unique/visit, parity, bytes | 중 | P2 | 현행 dict key 재사용 여부부터 측정 |
| tree pruning·root 재사용 | `_SearchState` | 오래된 노드 잔류 | 중간/중간 가능 | pruning CPU 비용 | 가치 통계·availability 보존 정책 | 고 | 고 | run 간 tree를 유지하는 설계가 생김 | 현재 `run`마다 새 tree이므로 무효 | retained nodes across runs | 낮음 | P3 | 현행에는 우선 필요 없음 |
| bounded tree·budget 조정 | `SearchLimits.max_nodes/max_edges` | 최악 peak 상한 | 높음/중간 가능 | 탐색 품질 저하 가능 | 초기 설정·품질/승률 실험; OOM 회피만으로 채택 금지 | 저 | 중 | 성능/품질 Pareto 확인 | 필수 탐색 커버리지 훼손 | RSS vs visits/quality | 중 | P1 | 기존 한도 존재; 조용한 축소 금지 |
| node recycling·free list | 향후 장수 tree | dead node 할당 반복 | 중간/중간 가능 | 향상 또는 reset 비용 | stale key·동시 reader 위험 | 고 | 고 | pruning·재사용 실제 존재 | 현행 per-run tree 전량 해제 | alloc/run, stale-ref 검사 | 낮음 | P3 | 현재 수명 패턴과 불일치 |
| root state + action path 재구성 | 가상의 per-node state 저장 | node별 full state 복제 | 현행 0/0 | 재구성 CPU 증가 | 현재 대상 구조 없음; chance/hidden state 보존 난해 | 고 | 고 | 미래에 per-node state를 도입할 때 | 현행 Python node는 state 미보관 | 구조 확인 후 비교 | 낮음 | P3 | 이번 코드에 적용할 최적화가 아님 |
| state delta·undo/redo | `accepts`, transition simulation | 상태 깊은 clone | 높음/중간 가능 | 적용/역적용 비용; locality 개선 가능 | 오류·RNG·history·replay rollback, borrow 수명 | 고 | 고 | clone 비용이 지배하고 역연산 증명 가능 | 복잡한 효과·예외 중 rollback 누락 | 차분·실패 주입, byte/action | 중 | P2 | 원자성·동결 규칙 우선 |
| action path에서 immutable state 부분 공유 | simulation Position | 경로별 반복 상태 | 중간/중간 가능 | refcount/clone-on-write 부담 | chance/hidden state 격리·Python bridge 영향 | 고 | 고 | 경로가 대부분 동일 부분 공유 | 매 action이 공유 부분을 고침 | bytes/simulation, parity | 중 | P2 | full clone 제거와 중복 평가 |

### 4. allocation 전략

| 기법 | 대상 | 해결하려는 문제 | 예상 메모리 효과 (peak/steady) | 예상 성능 효과 (CPU/locality) | 추가 오버헤드 | 구현 난이도 | 위험도 | 적용 조건 | 부적합 조건 | 검증 방법 | 현재 프로젝트 적합성 | 우선순위 | 비고 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| object pool | 동일형 후보/임시 객체 | 반복 생성·해제 | 낮음/steady 증가 가능 | 향상 또는 lock 저하 | 초기 용량·반환·reset; worker 간 격리 | 중 | 중 | 큰 동일형 객체 반복 사용 | 동적 JSON·수명 불규칙 | alloc/action, pool high-water | 중 | P2 | 잔류 heap이 OOM 악화 가능 |
| free list·slab | 장수 Rust object graph 후보 | 개별 할당·fragmentation | 현행 낮음/미상 | locality 가능 | slot metadata·세대·동기화 | 고 | 고 | 잦은 node 제거/재생성 | 현행 탐색이 Python dict/per-run | bytes/node, churn | 낮음 | P3 | [slotmap 문서](https://docs.rs/slotmap/latest/slotmap/) |
| generational arena | 미래 index graph | stale index 방지 | 현행 낮음/미상 | lookup 간접 비용 | 세대 byte·slot 보유·borrow 설계 | 고 | 중 | 삭제 후 index 재사용 필요 | 현행 그래프 없음 | stale-key 검사, bytes/slot | 낮음 | P3 | 단순 arena보다 metadata 큼 |
| arena allocation | 동일 수명 Rust scratch | per-object allocator 호출 | 중간/steady 증가 가능 | locality 향상 가능 | 전체 reset만 쉬움; drop/borrowing·thread 경계 | 고 | 고 | phase 단위 전량 해제 | 임의 노드 생존·개별 해제 필요 | alloc, retained chunks, RSS after reset | 중 | P2 | 먼저 수명 histogram |
| `bumpalo` 별도 평가 | action page 한정 임시 JSON/byte 후보 | 짧은 다수 allocation | 중간/steady 증가 가능 | 할당 향상 가능 | chunk 보유·`Drop` 미호출·외부 Vec heap 별도; arena 공유 불가 | 고 | 고 | page 종료 시 모든 참조 소멸·명시 reset 가능 | MCTS 장수 node/edge, Python 객체 반환, 혼합 수명 | page peak·reset 뒤 RSS·Drop 검증·parity | 낮음 | P3 | [bumpalo 공식 crate 문서](https://docs.rs/bumpalo/latest/bumpalo/). `Bump` 내부 객체의 `Drop`은 자동 호출되지 않음 |
| thread-local scratch | Rust page/serialization 버퍼 | worker 간 lock·반복 할당 | 낮음/steady 증가 | 향상 가능 | thread별 high-water 배수·재진입/종료 | 중 | 중 | 고정 수 worker와 작은 상한 | 많은 thread·큰 outlier | RSS/thread, alloc/thread | 중 | P2 | 현행 Python leaf batch는 worker thread 아님 |
| stack allocation | 작고 상한 있는 temporary | heap 왕복 | 낮음/중간 가능 | 향상 가능 | stack 폭주·복사·초기화 | 중 | 중 | 작은 고정 상한 증명 | 큰 JSON·가변 action | alloc/call, stack peak | 중 | P2 | fixed array와 결합 |

### 5. allocator 변경

| 기법 | 대상 | 해결하려는 문제 | 예상 메모리 효과 (peak/steady) | 예상 성능 효과 (CPU/locality) | 추가 오버헤드 | 구현 난이도 | 위험도 | 적용 조건 | 부적합 조건 | 검증 방법 | 현재 프로젝트 적합성 | 우선순위 | 비고 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| fragmentation 완화·큰 버퍼 release 정책 | Rust/Python 장수 process | 해제 뒤 RSS 잔류 | 중간/중간 가능 | shrink·재할당 비용 | allocator별 반환 차이; thread cache | 중 | 중 | live byte 대비 RSS 격차가 큼 | 실제 live object가 원인 | live vs RSS, reset 후 RSS | 중 | P2 | `shrink_to_fit` 반환 보장 아님 |
| jemalloc 교체 실험 | Rust native process | allocator fragmentation/경합 | 불명/불명 | workload 의존 | 빌드·배포·PyO3 process-wide 상호작용 | 중 | 중 | system allocator가 병목으로 확인 | Python/ONNX 포함 전체 RSS 설명 불가 | 동일 binary 조건 RSS·throughput | 낮음 | P3 | 구현은 이번 PR 범위 밖 |
| mimalloc 교체 실험 | Rust native process | allocator 경합·fragmentation | 불명/불명 | workload 의존 | 배포/ABI·다른 allocator와 공존 확인 | 중 | 중 | allocator profile 근거 있음 | 코드의 live data 자체가 지배 | RSS·throughput·fragmentation | 낮음 | P3 | 단독 A/B 실험만 |
| custom allocator | 특수 크기 Rust 객체 | 일반 allocator 오버헤드 | 불명/불명 | 개선 또는 악화 | unsafe·layout·OOM·thread·FFI 고위험 | 고 | 고 | hot allocation 크기/수명 분포 확인 | JSON/Python/ONNX가 지배 | Miri/ASan, stress, RSS | 낮음 | P3 | 범용 선제 도입 부적절 |

### 6. 병렬·self-play·bridge

| 기법 | 대상 | 해결하려는 문제 | 예상 메모리 효과 (peak/steady) | 예상 성능 효과 (CPU/locality) | 추가 오버헤드 | 구현 난이도 | 위험도 | 적용 조건 | 부적합 조건 | 검증 방법 | 현재 프로젝트 적합성 | 우선순위 | 비고 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| leaf batch/particle 예산의 메모리 모델 | `leaf_batch_size`, belief particles, padded features | 동시 요청 곱셈 peak | 높음/중간 가능 | 작은 batch는 throughput 저하 | 품질·추론 효율; 기존 제한 유지 | 중 | 중 | batch/particle과 RSS 상관 확인 | 무조건 줄여 모델 품질/속도 훼손 | RSS/batch·particle, nodes/sec | 높음 | P1 | 조용한 자동 축소 금지 |
| immutable catalog/model 공유 | 향후 병렬 self-play worker | worker당 중복 모델/규칙 | 높음/중간 가능 | refcount/IPC 비용 | process 간 `Arc` 불가; Python/ONNX session thread safety | 고 | 고 | 실제 worker 복제 확인 | process 격리·모델 mutable state 필요 | RSS/worker, throughput | 중 | P2 | 현행 검색 루프는 단일 프로세스·협력적 |
| worker 수/동시성 예산 | 향후 self-play scheduler | 동시 peak 합산 | 높음/중간 | throughput 저하 | 총 메모리·GPU·termination 명시 | 저 | 중 | worker별 RSS 측정 | 품질·시간 예산 미충족 | total/worker RSS, games/sec | 중 | P1 | 아직 별도 구현 확인 전 후보 |
| PyO3 action 객체·payload 복사 축소 | `Action::wrap`, `legal_actions` | Rust Value/Python tuple 중복 | 중간/중간 가능 | 경계 호출 향상 가능 | Python 소유권·GIL·action 수명·API 계약 | 고 | 고 | action 전체 포장이 지배 | Python 소비자가 원본 Position 참조 필요 | bytes/action, reference validity | 중 | P2 | zero-copy 가능 범위는 제한적 |
| NumPy/ONNX 입력 copy 최소화 | `batch_positions`, evaluator | stack/pad/runtime copy | 중간/중간 가능 | 향상 가능 | contiguous float32·shape·mask·수명, provider copy 확인 | 고 | 고 | profiler에서 변환 비용 확인 | runtime이 필수 사본 요구 | peak/batch, copy count, logits parity | 중 | P2 | Python↔Rust zero-copy 가능성 확인 전 가설 |
| self-play batch 구성 개선 | 향후 다수 게임 inference | 작은 batch와 padded 낭비 | 중간/중간 가능 | throughput 향상 또는 대기 증가 | 지연·공정성·padding·결과 대응 | 고 | 중 | 여러 게임이 같은 encoder/model 사용 | 가변 action 수로 padding 급증 | padded/valid ratio, RSS/game | 중 | P2 | 현재 전용 self-play worker 경로 미확인 |

### 7. 고위험 구조 변경·현재 부적합 후보

| 기법 | 대상 | 해결하려는 문제 | 예상 메모리 효과 (peak/steady) | 예상 성능 효과 (CPU/locality) | 추가 오버헤드 | 구현 난이도 | 위험도 | 적용 조건 | 부적합 조건 | 검증 방법 | 현재 프로젝트 적합성 | 우선순위 | 비고 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| `GameState` 전체 compact board 재설계 | 보드와 `Piece` | heap 및 clone | 높음 가능/중간 | 변환 비용·locality 혼합 | source-shape JSON, replay, oracle, Python 계약 광범위 | 고 | 고 | 보드가 실제 RSS 지배 | JSON extra/trace가 지배 | 전체 v7 차분·RSS·nodes/sec | 낮음 | P3 | 현재 작업에서 구조 변경 금지 |
| Python tree를 Rust로 이동 | `_SearchState` 전체 | Python object overhead | 높음 가능/중간 | FFI 감소 또는 증가 | 정보집합·hidden state·평가 경계 재설계 | 고 | 고 | Python heap이 지배·계약 안정 | 알고리즘/모델 경계가 변동 중 | parity, RSS, visits/sec | 낮음 | P3 | 규칙 중복 구현 금지 |
| 전체 상태 zero-copy 직렬화 | `GameState`/JSON/bridge | DTO 복사 | 불명/불명 | 향상 또는 alias 비용 | canonical identity·가변 `Value`·수명·FFI 불변식 | 고 | 고 | 소비자가 같은 immutable byte를 사용 | 변형/재정렬/서로 다른 표현 필요 | byte equality, lifetime, RSS | 낮음 | P3 | 선택적 buffer 재사용부터 평가 |

## 해석과 한계·후속 순서

가장 먼저 측정할 가설은 **후보 action 검증의 깊은 상태 복제**, eager 합법 행동의 JSON/정규화·중복 제거, Python node/edge·particle과 batch 배열의 동시 생존이다. `V7HostPosition`의 얕은 `Arc` 복제, Python tree가 `GameState`를 직접 보관한다는 주장, 현재 self-play worker가 여러 개라는 주장은 이 소스로 뒷받침되지 않는다.

1. 동일한 국면·행동·profile에서 native action 생성, apply, Python search/belief/encode/evaluate의 peak/live/alloc을 **서로 다른 구간**으로 계측한다. 현행 native allocation probe는 요청 수만 제공한다.
2. 할당 상위 경로와 수명·중복률·size/len 분포를 얻은 다음 P0의 작은 후보를 독립적으로 A/B 비교한다. 각 결과의 속도, 캐시, peak와 steady, reset 후 RSS, 의미 대조를 같이 기록한다.
3. P1/P2는 실제 병목과 부작용을 확인한 뒤 계약별 PR로 나눈다. 특히 `bumpalo`·arena는 page 종료 시 참조가 모두 끊어지는지 먼저 확인한다. 노드가 장수하거나 Python으로 값이 나가면 reset 시점이 맞지 않아 peak가 오를 수 있다.

미측정: OOM의 정확한 프로세스·국면·동시 실행 수, live/fragmented byte 비율, GameState clone 횟수와 크기, action 길이 분포, bytes/node·edge, particle 중복률, ONNX 내부/장치 메모리, allocator별 반환, 품질 대비 예산 효과. 본 문서의 `High`도 이 자료 없이 실제 절감량으로 해석하지 않는다.

재현 방법: 기준 SHA의 위 소스 경로를 읽고 `rg -n 'clone\(|Arc<|Vec<|legal_public_intents|class _Node|class _Edge|batch_positions' projects/augment-chess/engine/src projects/accelerate`로 호출 지점을 재확인한다. 실행 입력·seed·모델·worker·환경·시간·메모리 예산은 소스 조사만 수행했으므로 해당 없음. 측정 명령 및 수치는 **미실행/미측정**. 원시 산출물 없음. 코드 변경 없음.

관련 기본 자료: [Rust Vec](https://doc.rust-lang.org/std/vec/struct.Vec.html), [Rust Arc](https://doc.rust-lang.org/std/sync/struct.Arc.html), [bumpalo](https://docs.rs/bumpalo/latest/bumpalo/), [smallvec](https://docs.rs/smallvec/latest/smallvec/), [slotmap](https://docs.rs/slotmap/latest/slotmap/). 이 자료는 일반 기법의 계약을 설명하며, 이 저장소에서의 실제 효과를 입증하지 않는다.

## 정정과 공유 전 점검

정정·후속 기록 없음. 목적·기준 SHA·소스 근거·미측정 한계를 기록했다. 공개 로그인명을 GitHub API에서 확인했다. 본문은 저장소 상대 경로만 사용하고 원시 로그·로컬 식별 정보·비밀을 포함하지 않는다. 후속 실험 결과는 같은 질문의 기록에 추가하고 실제 수치와 이 가설을 구분한다.
