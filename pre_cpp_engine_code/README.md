이 폴더는 rust를 다루지 못하는 한 불쌍한 개발자가 cpp로 손코딩하고 그걸 ai한테 rust포팅 요청을 날리면서 제작하기 위한 작은 스케치 코드입니다.

## 판단 코드 표기 규칙

규칙이 애매해서 판단이 필요한 곳은 설명만 남기지 않고, 실제로 동작하는 코드를 짜서 주석 처리해 두었습니다.
형식은 `// 판단(이름): 한 줄 근거` 아래에 `// [JUDGMENT-BEGIN 이름]` ~ `// [JUDGMENT-END 이름]` 블록이 오고,
블록 안의 각 줄 맨 앞 `// `만 지우면 그대로 살아납니다. 같은 이름의 블록은 함께 살려야 합니다.

- 전부 살린 변형본: `./uncomment_judgment.sh engine.cpp > engine_all.cpp`
- 이름을 지정해 일부만: `./uncomment_judgment.sh engine.cpp C-shift F-promo-choice > part.cpp`
- 빌드와 자체 검사 실행: `g++ -std=c++20 -Wall -Wextra engine.cpp -o engine_test && ./engine_test`
  (`main()`이 검사를 돌리고 실패하면 0이 아닌 값을 반환합니다. 변형본도 같은 방식으로 빌드합니다.)

## 남아 있는 판단 코드 블록

PR #17 리뷰(구독좋아요님)에서 `B-chain`은 반려되어 제거되었고(대신 `moveChunk.next`를 활성화 트리로 해석하는 코드가 활성 코드가 되었습니다), `C-jump`는 승인되어 활성 코드가 되었습니다.
아직 주석 상태로 남은 블록은 아래 네 가지입니다. (`./uncomment_judgment.sh --list engine.cpp`로도 확인할 수 있습니다.)

| 블록 | 상태 | 내용 |
|---|---|---|
| `C-shift` | 보류 | SHIFT(두 기물의 자리 교환). 실제 게임에서 큰 기물과의 상호작용을 먼저 확인한 뒤 재논의 |
| `F-promo-choice` | 판단 대기 | 프로모션 랭크에 도달하는 수를 `promotion_pool` 항목마다 별도 행동으로 펼침 |
| `F-castle-attack-check` | 판단 대기 | 캐슬링 출발/통과/도착 칸이 공격받으면 캐슬링 불가 |
| `G-forced-consume` | 판단 대기 | ForcedPiece의 남은 횟수를 강제 기물이 행동할 때마다 소모 |

## 엔진 구조 메모

- `moveChunk.next`는 부모-자식 활성화 트리입니다. 유효하게 활성화된 칸마다 기물의 원래 위치를 start로 하는 독립 `moveAction`이 나옵니다. 규칙은 `engine.cpp`의 `[moveChunk 활성화 트리]` 주석에 있습니다.
- `apply_action`은 이미 검증된 합법 action만 받는 내부 빠른 경로입니다. 외부 입력(bridge 등)은 먼저 `validateExternalAction`을 통과시킵니다.
- 카드 효과 추가 절차는 `engine.cpp`의 `registerCardEffect` 아래 `[임의의 카드를 추가하는 절차]` 주석에 있습니다.
- 왕족 속성 필드는 `Piece::isRoyal` 하나입니다. KING 종류 여부는 `pT == pieceType::KING`으로 판단하고, 기본 KING은 `isRoyal = true`로 만들어집니다.
