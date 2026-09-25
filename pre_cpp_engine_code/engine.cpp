//ai한테 던져줄 rust 포팅용 cpp코드

// ============================================================================
// [판단 코드 표기 규칙]
//
// 규칙이 애매해서 판단이 필요한 곳은 설명만 남기지 않고, 실제로 동작하는 코드를
// 통째로 짜서 주석 처리해 두었다. 형식은 항상 아래와 같다.
//
//     // 판단(이름): 어떤 판단으로 이렇게 짰는지 한 줄
//     // [JUDGMENT-BEGIN 이름]
//     //     실제 코드 (각 줄 맨 앞이 "// ")
//     // [JUDGMENT-END 이름]
//
// 리뷰어가 그 판단에 동의하면 BEGIN~END 사이 줄의 맨 앞 "// "만 지우면 된다.
// 같은 이름의 블록이 여러 곳에 흩어져 있을 수 있고(선언/구현/테스트), 그 경우
// 이름이 같은 블록은 한 번에 함께 살려야 한다. 서로 다른 이름의 블록끼리는
// 독립이라 하나씩만 또는 전부 살려도 컴파일된다.
// 전부 한 번에 살린 변형본 만들기: ./uncomment_judgment.sh engine.cpp > engine_all.cpp
// ============================================================================

//우선 타입 먼저 만들기
#include<iostream>
#include<vector>
#include<string>
#include<utility>
#include <set>
#include<algorithm>
#include<optional>
#include <variant>
#include <functional>
#include <unordered_map>
#include <cstdlib>

enum class colorType{
    WHITE,
    BLACK,
    NEUTRAL
};

enum class CardType{
    PIECE,
    RULE,
    OPENING,
    MIDDLE,
    END
};

enum class CardActType{
    PASSIVE,
    ACTIVE,
    ACTIVE_FORCED //opening 카드처럼 의사와 상관없이 강제로 적용되는 카드의 경우 이 필드를 갖는다.
};

enum class pieceType{
    KING,
    QUEEN,
    KNIGHT,
    BISHOP,
    ROOK,
    PAWN
};

enum class moveType{
    BOTHTAKEMOVE, //적-아군 기물 상관없이 모두 잡는 행마
    TAKEMOVE,
    CATCH,
    MOVE,
    SHIFT,
    TAKE,
    JUMP //이건 좀 복잡한데 어떻게 주석을 달지
};

/**
 * moveChunk는 하나의 "행마 규칙 조각"을 표현한다.
 *
 * 기본 해석 규칙:
 *
 * 1. direction
 *    - 현재 해석 위치에서 얼마나 이동할지를 나타내는 상대 벡터이다.
 *    - 예: {1, 0} 은 오른쪽 한 칸, {0, 1} 은 전방 한 칸을 의미한다.
 *
 * 2. mT
 *    - destination 위치에서 어떤 종류의 이동/상호작용이 가능한지를 정의한다.
 *
 *    BOTHTAKEMOVE:
 *      목적지에 적/아군 기물이 존재하더라도 포획할 수 있으며,
 *      비어 있는 경우에도 이동할 수 있다.
 *
 *    TAKEMOVE:
 *      목적지가 비어 있으면 이동하고,
 *      적 기물이 존재하면 포획한다.
 *      아군 기물이 있으면 이동할 수 없다.
 *
 *    CATCH:
 *      목적지에 포획 가능한 기물이 존재할 때만 유효하다.
 *      목적지의 포획 가능한 기물을 제거하고 이동하진 않는 행마이다.
 *
 *    MOVE:
 *      목적지가 비어 있을 때만 이동할 수 있다.
 *
 *    SHIFT:
 *      일반 이동/포획과 다른 특수 위치 변경에 사용한다.
 *      정확한 처리 방식은 엔진의 SHIFT 해석 규칙을 따른다.
 * 
 *    TAKE:
 *      목적지에 포획 가능한 기물이 존재할 때만 유효하다.
 *      빈 칸으로는 이동 불가능하며, 적 기물을 포획하고 그 자리로 이동한다.
 *
 * 3. maxDistance
 *    - direction 벡터를 최대 몇 번 연속 적용할 수 있는지를 뜻한다.
 *
 *      maxDistance = 1
 *          현재 위치 + direction
 *
 *      maxDistance = 3
 *          현재 위치 + direction
 *          현재 위치 + direction * 2
 *          현재 위치 + direction * 3
 *
 *      maxDistance = std::nullopt
 *          보드 경계 또는 행마 규칙상 더 이상 진행할 수 없을 때까지 반복한다.
 *
 *    - 진행 도중 기물이나 보드 경계 등에 의해 행마가 막히면
 *      이후 거리는 더 이상 생성하지 않는다.
 *
 * 4. activateSquare
 *    - 값이 존재할 경우 해당 moveChunk는 기물의 현재 기준 위치가
 *      activateSquare와 일치할 때만 활성화된다.
 *
 *    - std::nullopt이면 위치에 상관없이 활성화된다.
 *
 * 5. next
 *    - 현재 moveChunk가 성공적으로 적용된 이후 이어서 해석할
 *      후속 moveChunk들을 저장한다.
 *
 *    예:
 *
 *      A.then(B)
 *
 *      A -> B
 *
 *    이 경우 B의 시작 위치는 A가 성공한 결과 위치가 된다.
 *
 *    예:
 *
 *      moveChunk root(...);
 *      root.next.push_back(A);
 *      root.next.push_back(B);
 *
 *    는 다음과 같은 분기 구조를 의미한다.
 *
 *          root
 *         /    \
 *        A      B
 *
 * 6. 연쇄 행마의 해석
 *
 *    예:
 *
 *      TAKE_MOVE {0, 1}
 *          ->
 *      CATCH {1, 0}
 *
 *    시작 위치가 {3, 2}라면:
 *
 *      첫 번째 chunk:
 *          {3, 2} -> {3, 3}
 *
 *      두 번째 chunk:
 *          {3, 3} -> {4, 3}
 *
 *    즉 next의 direction은 항상 직전 chunk의 성공 위치를 기준으로 적용한다.
 *
 * 7. moveAction 생성
 *
 *    moveChunk 자체는 "행마 정의"일 뿐이며 실제 착수를 의미하지 않는다.
 *
 *    interpretPieceMoves() 또는 이에 준하는 interpreter가
 *    현재 GameState와 moveChunk를 함께 해석한 뒤,
 *    실제로 실행 가능한 결과만 moveAction으로 변환한다.
 *
 *    따라서 구조는 다음과 같다.
 *
 *      moveChunk
 *          ↓
 *      GameState 기반 해석
 *          ↓
 *      legal moveAction
 *
 * 8. PieceVolume
 *
 *    다중 칸을 점유하는 기물의 경우 destination 하나만 확인해서는 안 된다.
 *
 *    이동 후 anchor 위치에 PieceVolume.footprint를 적용했을 때
 *    점유하게 되는 모든 좌표가:
 *
 *      - 현재 보드에서 유효하고
 *      - 해당 moveType의 충돌 규칙을 만족해야 한다.
 *
 * 9. direction은 "행마 정의용 상대 벡터"이고,
 *    실제 start/destination 좌표는 moveChunk에 저장하지 않는다.
 *
 *    실제 좌표는 GameState에서 해석할 때 계산한다.
 */

using Coord = std::pair<int, int>;
using actCoord = std::pair<std::optional<int>, std::optional<int>>; 
//둘 중 하나가 std::nullopt 라면, "임의의" 라는 의미로 해석하라. 예를 들자면,
//actCoord{std::nullopt, 2}는 임의의 파일 좌표를 가진 2랭크 칸 즉, 2랭크를 활성화 칸으로 하라는 의미이다. 

struct PieceTypeAt {
    pieceType type;
    Coord position;
};

struct moveChunk {
    moveType mT;
    Coord direction;

    std::optional<std::vector<actCoord>> activateSquare; //각 벡터는 논리합(OR)으로 계산한다.

    std::vector<moveChunk> next;

    // nullopt = 거리 제한 없음
    std::optional<int> maxDistance = 1;

    moveChunk(
        moveType type,
        Coord dir,
        std::optional<int> maxDist = 1,
        std::optional<std::vector<actCoord>> actSq = {}
    )
        : mT(type),
          direction(dir),
          maxDistance(maxDist),
          activateSquare(actSq)
    {}

    moveChunk(
        moveType type,
        Coord dir,
        std::vector<moveChunk> nextChunks,
        std::optional<int> maxDist = 1,
        std::optional<std::vector<actCoord>> actSq = {}
    )
        : mT(type),
          direction(dir),
          next(std::move(nextChunks)),
          maxDistance(maxDist),
          activateSquare(actSq)
    {}

    moveChunk& then(moveChunk nextChunk) {
        next.push_back(std::move(nextChunk));
        return next.back();
    }
};

// ============================================================
// 기본 기물 행마 정의
// ============================================================

/**
 * 모든 기본 기물의 행마는 moveChunk의 집합으로 정의한다.
 *
 * 기본 행마와 Piece.additional_mC의 차이:
 *
 * - BASE_*_MOVES
 *      해당 pieceType이 원래 가지고 있는 행마이다.
 *      게임 도중 변경되지 않는다.
 *
 * - Piece::additional_mC
 *      카드, 룰, 버프 등의 영향으로 런타임에 추가되는 행마이다.
 *
 * legal move를 생성할 때는:
 *
 *      기본 행마 + additional_mC
 *
 * 를 함께 해석한다.
 *
 * maxDistance:
 *      1            = direction을 한 번 적용
 *      N            = direction을 최대 N번 적용
 *      std::nullopt = 보드/기물에 의해 막힐 때까지 무제한 반복
 */


// ------------------------------------------------------------
// King
// ------------------------------------------------------------

inline const std::vector<moveChunk> BASE_KING_MOVES = {
    {moveType::TAKEMOVE, { 1,  0}},
    {moveType::TAKEMOVE, {-1,  0}},
    {moveType::TAKEMOVE, { 0,  1}},
    {moveType::TAKEMOVE, { 0, -1}},

    {moveType::TAKEMOVE, { 1,  1}},
    {moveType::TAKEMOVE, {-1,  1}},
    {moveType::TAKEMOVE, { 1, -1}},
    {moveType::TAKEMOVE, {-1, -1}}
};


// ------------------------------------------------------------
// Queen
// ------------------------------------------------------------

inline const std::vector<moveChunk> BASE_QUEEN_MOVES = {

    // Rook directions
    {moveType::TAKEMOVE, { 1,  0}, std::nullopt},
    {moveType::TAKEMOVE, {-1,  0}, std::nullopt},
    {moveType::TAKEMOVE, { 0,  1}, std::nullopt},
    {moveType::TAKEMOVE, { 0, -1}, std::nullopt},

    // Bishop directions
    {moveType::TAKEMOVE, { 1,  1}, std::nullopt},
    {moveType::TAKEMOVE, {-1,  1}, std::nullopt},
    {moveType::TAKEMOVE, { 1, -1}, std::nullopt},
    {moveType::TAKEMOVE, {-1, -1}, std::nullopt}
};


// ------------------------------------------------------------
// Rook
// ------------------------------------------------------------

inline const std::vector<moveChunk> BASE_ROOK_MOVES = {
    {moveType::TAKEMOVE, { 1,  0}, std::nullopt},
    {moveType::TAKEMOVE, {-1,  0}, std::nullopt},
    {moveType::TAKEMOVE, { 0,  1}, std::nullopt},
    {moveType::TAKEMOVE, { 0, -1}, std::nullopt}
};


// ------------------------------------------------------------
// Bishop
// ------------------------------------------------------------

inline const std::vector<moveChunk> BASE_BISHOP_MOVES = {
    {moveType::TAKEMOVE, { 1,  1}, std::nullopt},
    {moveType::TAKEMOVE, {-1,  1}, std::nullopt},
    {moveType::TAKEMOVE, { 1, -1}, std::nullopt},
    {moveType::TAKEMOVE, {-1, -1}, std::nullopt}
};


// ------------------------------------------------------------
// Knight
// ------------------------------------------------------------

inline const std::vector<moveChunk> BASE_KNIGHT_MOVES = {
    {moveType::TAKEMOVE, { 1,  2}},
    {moveType::TAKEMOVE, { 2,  1}},

    {moveType::TAKEMOVE, {-1,  2}},
    {moveType::TAKEMOVE, {-2,  1}},

    {moveType::TAKEMOVE, { 1, -2}},
    {moveType::TAKEMOVE, { 2, -1}},

    {moveType::TAKEMOVE, {-1, -2}},
    {moveType::TAKEMOVE, {-2, -1}}
};


// ------------------------------------------------------------
// Pawn
// ------------------------------------------------------------

/**
 * Pawn은 색에 따라 전진 방향이 반대이므로
 * WHITE / BLACK 행마를 별도로 정의한다.
 *
 * 여기서는 기본적인:
 *
 *      전진 1칸
 *      대각선 포획
 *
 * 만 정의한다.
 *
 * 앙파상 등의 규칙은
 * GameState의 상태를 필요로 하므로 별도의 규칙으로 처리한다.
 */

inline const std::vector<moveChunk> BASE_WHITE_PAWN_MOVES = {

    // forward
    {moveType::MOVE, {0, 1}},

    // 2랭크에서는 최대 2칸 전진
    moveChunk(
        moveType::MOVE,
        {0, 1},
        2,
        std::vector<actCoord>{
            {std::nullopt, 2}
        }
    ),

    // capture
    {moveType::TAKE, { 1, 1}},
    {moveType::TAKE, {-1, 1}}
};


inline const std::vector<moveChunk> BASE_BLACK_PAWN_MOVES = {

    // forward
    {moveType::MOVE, {0, -1}},

    // 2랭크에서는 최대 2칸 전진
    moveChunk(
        moveType::MOVE,
        {0, -1},
        2,
        std::vector<actCoord>{
            {std::nullopt, 7}
        }
    ),

    // capture
    {moveType::TAKE, { 1, -1}},
    {moveType::TAKE, {-1, -1}}
};

const std::vector<moveChunk>& getBaseMovement(
    pieceType type,
    colorType color
) {
    switch (type) {

        case pieceType::KING:
            return BASE_KING_MOVES;

        case pieceType::QUEEN:
            return BASE_QUEEN_MOVES;

        case pieceType::ROOK:
            return BASE_ROOK_MOVES;

        case pieceType::BISHOP:
            return BASE_BISHOP_MOVES;

        case pieceType::KNIGHT:
            return BASE_KNIGHT_MOVES;

        case pieceType::PAWN:
            if (color == colorType::WHITE)
                return BASE_WHITE_PAWN_MOVES;

            return BASE_BLACK_PAWN_MOVES;
    }

    // 실제로는 도달하면 안 됨.
    static const std::vector<moveChunk> EMPTY_MOVES;
    return EMPTY_MOVES;
}

struct Card{
    CardActType cAT;
    CardType cT;
    std::string cardId;
    bool isUsed; //cAT 가 CardActType::PASSIVE 인 경우 항상 true
};

/**
 * 이 구조체는 기물의 초기위치에서 얼마나 떨어진 칸을 점유하게 할 것이냐를 정하는 구조체이다.
 * 예시를 들자면,
 * ex) Piece bigRook; 
 * PieceVolume test;
 * test.push_back({1, 0});
 * test.push_back({0, 1});
 * test.push_back({1, 1});
 * bigRook.Pv = test;
 * 이때 이 기물 부피의 정의는 다음과 같다.
 * {0, 1} {1, 1}
 * {0, 0}//초기위치 {1, 0}
 * 만약 이 기물이 {dx, dy}만큼 움직인다고 해보자,
 * 그럼 이 기물의 위치는 자신이 점유한 모든 칸에 대하여 dx, dy값을 더해주는 것으로 처리된다.
 * 당연히 그 킨들 중 하나라도 보드 범위에 벗어나면 안 됀다.
 * 만약 표준 기물이라면, 공백으로 처리하라.
 */
struct PieceVolume{
    std::vector<Coord> footprint; // { {dx1, dy1}, {dx2, dy2} ... }
};

struct MustCapture {
    int remainingTriggers;
};

struct NoCapture {
    int remainingTriggers;
};

struct RepositionMove {
    int remainingTriggers;
};

struct ForcedPiece {
    PieceTypeAt piece;
    int remainingTriggers;
};

// 기물 자체에 붙는 제약
using PieceConstraint = std::variant<
    MustCapture,
    NoCapture,
    RepositionMove
>;

// 게임의 현재 행동 흐름에 붙는 제약
using GameConstraint = std::variant<
    ForcedPiece
>;

/**
 * 기물에 붙는 상태 플래그의 "그릇".
 *
 * docs/GAME-RULES.md §6 '상태 이상과 보정'이 이동 불가(frozen)와 잡기 방어(shielded)를
 * 기물 필드로 들고 있어야 한다고 명시한다. 지금은 필드 자리만 만들고 수 생성/포획에는
 * 아직 연결하지 않는다(연결은 canCapture / interpretedPieceMoveChunk 확장 때).
 * 전체 필드 목록은 augment-chess-engine-twist의 audit-data 기준으로 나중에 채운다.
 */
struct PieceStatusFlags {
    bool frozen = false;   // 이동 불가
    bool shielded = false; // 잡기 방어
};


struct Piece {
    colorType cT;
    pieceType pT;

    std::vector<pieceType> promotion_pool; // 프로모션 불가 시 {}
    std::vector<moveChunk> additional_mC;  // 특수 규칙으로 런타임에 추가되는 행마
    /**
    * additional_mC 필드 사용 예시.
    *
    * 만약 게임 도중 드래프트에서 플레이어가
    * "질주" 카드(폰이 첫 이동에 최대 3칸까지 전진 가능)를 사용했다고 가정한다.
    *
    * AugmentChessGameState s0; // 현재 게임 상태
    *
    * auto& B = s0.board;
    *
    * for (auto& square : B) {
    *     auto& curr_piece = square.curr_piece;
    *
    *     // 현재 턴 플레이어의 폰을 찾는다.
    *     if (
    *         curr_piece.pT == pieceType::PAWN &&
    *         curr_piece.cT == s0.turn
    *     ) {
    *
    *         // 질주로 인해 새롭게 추가되는 행마를 정의한다.
    *         moveChunk sprint(
    *             moveType::MOVE,
    *             {0, 1}
    *         );
    *
    *         sprint.maxDistance = 3;
    *
    *         // 해당 행마를 이 기물의 런타임 추가 행마 목록에 넣는다.
    *         curr_piece.addNewMovement(sprint);
    *     }
    * }
    *
    * 이후 legal move를 생성할 때는
    *
    *     1. 해당 pieceType이 원래 가지고 있는 기본 행마
    *     2. additional_mC에 들어 있는 런타임 추가 행마
    *
    * 를 함께 검사한다.
    *
    * 따라서 additional_mC는 기물의 원래 정의 자체를 수정하지 않고,
    * 카드, 룰, 버프, 디버프 등의 효과로 현재 게임에서만
    * 임시적으로 추가된 행마를 표현하는 용도로 사용한다.
    *
    * 예를 들어 "질주" 효과가 종료되면 해당 moveChunk를
    * additional_mC에서 제거함으로써 원래 행마로 되돌릴 수 있다.
    *
    * ※ "첫 이동에만 사용 가능"과 같은 조건은 moveCount 등의
    *    현재 Piece 상태를 이용해 legal move 생성 단계에서 별도로 판정한다.
    */

    PieceVolume Pv;

    int HP = 1;
    bool isKing = false;
    // 킹 외에도 승패를 가르는 왕족 표식 기물(royalKnight, vip, crownRoyal 등)이 있다.
    // docs/GAME-RULES.md §3 참고. 왕족 여부는 isRoyalPiece()로 판정한다.
    bool isRoyal = false;
    int moveCount = 0;

    std::vector<PieceConstraint> curr_piece_constraint = {}; //상태이상 처리용 확장필드. 이친구도 런타임에 처리되는 필드가 돼겠네.
    //얘는 그냥 직접 접근하게 둘까??? 차피 public이고

    PieceStatusFlags status; // frozen, shielded 등. 지금은 그릇만 (PieceStatusFlags 주석 참고)

    bool isRoyalPiece() const {
        return isKing || isRoyal;
    }

    // 기본 생성자
    Piece() = default;

    // 기본적인 기물 생성자
    Piece(
        colorType color,
        pieceType type,
        PieceVolume volume
    )
        : cT(color),
          pT(type),
          Pv(volume)
    {}

    // 프로모션 풀까지 지정
    Piece(
        colorType color,
        pieceType type,
        PieceVolume volume,
        std::vector<pieceType> promotionPool
    )
        : cT(color),
          pT(type),
          promotion_pool(std::move(promotionPool)),
          Pv(volume)
    {}

    // 전체 설정용 생성자
    Piece(
        colorType color,
        pieceType type,
        PieceVolume volume,
        std::vector<pieceType> promotionPool,
        std::vector<moveChunk> additionalMoves,
        int hp = 1,
        bool king = false,
        int moveCnt = 0
    )
        : cT(color),
          pT(type),
          promotion_pool(std::move(promotionPool)),
          additional_mC(std::move(additionalMoves)),
          Pv(volume),
          HP(hp),
          isKing(king),
          moveCount(moveCnt)
    {}

    void addNewMovement(moveChunk new_mC){
        additional_mC.push_back(new_mC);
    }

    void removeLastMovement(){
        additional_mC.pop_back();
    }

    void removeAllMovement(){
        additional_mC.clear();
    }

    void addConstraint(PieceConstraint Constraint){
        curr_piece_constraint.push_back(Constraint);
    }

    void removeLastConstraint(){
        curr_piece_constraint.pop_back();
    }

    void removeAllConstraint(){
        curr_piece_constraint.clear();
    }
};

struct Square {
    Piece curr_piece;
    Coord coordinate;

    PieceTypeAt getPieceTypeAt() const {
        return {
            curr_piece.pT,
            coordinate
        };
    }
};


struct TurnState {
    colorType player;
    int actionsRemaining = 1;
};

//isTurnUsed 필드는 이 객체로 된 데이터를 엔진에 보내어 엔진이 apply할 때, 이 행위가 턴을 사용하는 지를 판단하는 데 사용된다. 
//예를 들어 엔진이 이 행동을 apply하려고 할 때, 이 필드가 false인 경우 apply후 턴을 유지한다.
struct moveAction {
    colorType cT;
    pieceType pT;
    moveType mT;

    Coord start;
    Coord destination;

    bool isTurnUsed = true;

    // 프로모션 시 어떤 기물로 승격할지. nullopt면 promotion_pool의 첫 항목으로 승격한다.
    // (docs/GAME-RULES.md §4: 일반 폰 승격은 move 적용 중에 처리된다.)
    std::optional<pieceType> promotion;

    moveAction(
        colorType color,
        pieceType piece,
        moveType move,
        Coord from,
        Coord to,
        bool turnUsed = true
    )
        : cT(color),
          pT(piece),
          mT(move),
          start(from),
          destination(to),
          isTurnUsed(turnUsed)
    {}
};

struct cardAction {
    colorType cT;
    Card card;
    bool isTurnUsed = true;

    cardAction(
        colorType color,
        Card usedCard,
        bool turnUsed = true
    )
        : cT(color),
          card(std::move(usedCard)),
          isTurnUsed(turnUsed)
    {}
};

// ============================================================================
// cardId -> 효과 함수 매핑 인터페이스 (골격)
// ============================================================================
class AugmentChessGameState;

using CardEffectFn = std::function<void(AugmentChessGameState&, const cardAction&)>;

/**
 * 카드 효과 레지스트리. 지금은 의도적으로 비어 있다.
 *
 * 카드는 수백 장이고 각 효과가 보드/턴/제약을 서로 다르게 건드린다.
 * 효과 목록을 이 파일에서 다 정의하는 것은 범위 밖이므로, 여기에는 "카드 id로 효과 함수를
 * 찾아 호출하는 통로"만 둔다. 실제 효과는 나중에 registerCardEffect()로 하나씩 추가한다.
 * (docs/GAME-RULES.md §5, §9: 카드 효과는 추측으로 추가하지 말고 oracle과 대조해 채운다.)
 */
inline std::unordered_map<std::string, CardEffectFn>& cardEffectRegistry()
{
    static std::unordered_map<std::string, CardEffectFn> registry;
    return registry;
}

inline void registerCardEffect(const std::string& cardId, CardEffectFn effect)
{
    cardEffectRegistry()[cardId] = std::move(effect);
}

template <typename T>
bool hasConstraint(const Piece& piece)
{
    for (const auto& constraint : piece.curr_piece_constraint) {
        if (std::holds_alternative<T>(constraint)) {
            return true;
        }
    }

    return false;
}

// ============================================================================
// 종료 판정 / 연쇄 행마 / 앙파상 보조 타입
// ============================================================================

// 게임 결과. docs/GAME-RULES.md §3: 승패는 체크메이트가 아니라 왕족 기물의 실제 제거로 갈린다.
enum class GameResult {
    ONGOING,
    WHITE_WIN,
    BLACK_WIN,
    DRAW // 양쪽 왕족이 동시에 사라진 경우 (§3)
};

// moveChunk 하나를 특정 origin에서 해석했을 때 나오는 유효 step 한 건.
struct ChunkStep {
    Coord destination;
    moveType type;  // 실제로 적용된 moveType
    bool captures;  // 목적지의 기물을 제거하는 step인가
};

// moveChunk.next를 끝까지 따라간 완결 경로 하나. steps.front()가 root chunk의 step이고
// steps.back()이 마지막 chunk의 step(= 최종 목적지)이다.
struct ChunkChain {
    std::vector<ChunkStep> steps;
};

// 직전 상대 수가 폰 두 칸 전진이었을 때 남는 앙파상 정보.
struct EnPassantTarget {
    Coord square;      // 폰이 지나간 칸 (앙파상으로 도착하는 칸)
    Coord pawn;        // 두 칸 전진한 폰의 현재 anchor
    colorType pawnColor;
};

// activateSquare 조건을 임의 위치 기준으로 검사한다. (interpretedPieceMoveChunk 안의
// isActivated 람다와 같은 규칙이지만, 연쇄 행마의 next는 "직전 성공 위치"가 기준이라 따로 둔다.)
inline bool isChunkActivatedAt(const moveChunk& chunk, Coord pos)
{
    if (!chunk.activateSquare.has_value()) {
        return true;
    }

    for (const actCoord& condition : *chunk.activateSquare) {
        const bool xMatches =
            !condition.first.has_value() ||
            condition.first.value() == pos.first;

        const bool yMatches =
            !condition.second.has_value() ||
            condition.second.value() == pos.second;

        if (xMatches && yMatches) {
            return true;
        }
    }

    return false;
}

// 프로모션 랭크. 이 스케치의 좌표계에서 백 폰은 rank(second)가 늘어나는 방향으로 전진한다.
inline int promotionRankOf(colorType color)
{
    return color == colorType::WHITE ? 8 : 1;
}

inline colorType opponentOf(colorType color)
{
    return color == colorType::WHITE ? colorType::BLACK : colorType::WHITE;
}

// ============================================================================
// 엔진 본체
// ============================================================================
class AugmentChessGameState {
private:
    std::vector<Square> board;

    std::vector<Card> ruleCard;
    std::vector<Card> whitePlayerCards;
    std::vector<Card> blackPlayerCards;

    std::vector<GameConstraint> constraints;

    // 백이 먼저 둔다. (docs/GAME-RULES.md §2)
    TurnState turn{colorType::WHITE, 1};

    // 완료된 이동 행동 수. Piece.moveCount(기물별 이동 횟수)와는 별개다.
    // docs/GAME-RULES.md §4, §7: turn/actionsRemaining/turnsTaken/moveCount를 각각 보존해야 한다.
    int moveCount = 0;

    // 직전 행동이 폰 두 칸 전진이면 그 정보. 다음 이동 행동이 적용되면 사라진다.
    std::optional<EnPassantTarget> enPassant;

    // 앵커 좌표가 정확히 pos인 Square를 반환한다. (footprint는 보지 않는다.)
    Square* findSquareAtAnchor(Coord pos);

    // 해당 anchor를 가진 Square들을 보드에서 제거한다. (포인터/참조가 무효화되므로 호출 후 재조회)
    void removeAnchors(const std::vector<Coord>& anchors);

    // 앙파상 / 캐슬링 행동 생성 (docs/GAME-RULES.md §2: 기본 지원)
    std::vector<moveAction> enPassantActions(const Square& pawnSquare) const;
    std::vector<moveAction> castlingActions(const Square& kingSquare) const;

public:
    bool isValidSquare(Coord pos) const;
    bool isOccupied(Coord pos) const;
    bool isEmpty(Coord pos) const {
        return isValidSquare(pos) && !isOccupied(pos);
    }

    // anchor뿐 아니라 footprint까지 포함하여 해당 칸을 점유한 기물의 Square를 반환한다.
    Square* getOccupyingSquare(Coord pos);
    const Square* getOccupyingSquare(Coord pos) const;

    Piece* getPieceAt(Coord pos);
    const Piece* getPieceAt(Coord pos) const;

    // 기물을 newAnchor에 놓았을 때 몸 전체가 보드 안에 있는지 확인한다.
    bool isPieceInsideBoard(const Square& square, Coord newAnchor) const;

    // 기물을 newAnchor에 놓았을 때 footprint가 충돌하는 다른 기물들을 반환한다.
    // 같은 기물의 여러 footprint 칸과 겹쳐도 포인터는 한 번만 들어간다.
    std::vector<const Square*> getPlacementCollisions(
        const Square& movingSquare,
        Coord newAnchor
    ) const;

    // 기본 포획 가능 여부.
    // 특수 카드/상태/중립 규칙이 생기면 이 함수를 확장한다.
    bool canCapture(
        const Piece& attacker,
        const Piece& target,
        moveType type
    ) const;

    /**
     * curr_piece가 가진 기본 moveChunk와 additional_mC를 해석하여
     * 현재 GameState에서 실제로 실행 가능한 moveAction 목록을 생성한다.
     *
     * 판단 코드(주석 처리, 파일 상단의 [판단 코드 표기 규칙] 참고)로만 들어 있는 부분:
     * - moveChunk.next 연쇄 행마의 action 변환 (B-chain)
     * - SHIFT (C-shift)
     * - JUMP (C-jump)
     */
    std::vector<moveAction> interpretedPieceMoveChunk(
        const Square& curr_piece
    );

    /**
     * chunk 하나를 origin 기준으로 해석해 유효한 step 목록을 반환한다.
     * interpretedPieceMoveChunk의 단일 chunk 판이며, next 연쇄에서 "직전 성공 위치"를
     * origin으로 넘겨 재사용한다. allowCapture=false면 포획하는 step은 만들지 않는다(NoCapture).
     * 연쇄 안의 SHIFT/JUMP step은 아직 지원하지 않는다(step을 만들지 않는다).
     */
    std::vector<ChunkStep> resolveChunkSteps(
        const Square& movingSquare,
        Coord origin,
        const moveChunk& chunk,
        bool allowCapture
    ) const;

    /**
     * chunk와 그 next를 재귀로 끝까지 따라가 완결된 경로를 모두 반환한다.
     * chunk가 유효 목적지를 만들면 그 목적지를 새 origin으로 삼아 chunk.next 각각을 다시
     * 해석한다. CATCH step은 기물이 이동하지 않으므로 다음 origin이 그대로 유지된다.
     * 이 함수는 경로 전체를 돌려줄 뿐이고, moveAction으로 어떻게 줄일지는 호출 쪽 판단이다.
     */
    std::vector<ChunkChain> resolveChunkChains(
        const Square& movingSquare,
        Coord origin,
        const moveChunk& chunk,
        bool allowCapture
    ) const;

    // 기물 이동
    void apply_action(const moveAction& action);

    // 카드 사용
    void apply_action(const cardAction& action);

    // color 플레이어가 지금 둘 수 있는 모든 기물 이동 행동.
    // docs/GAME-RULES.md §3: 킹 세이프티(자기 왕을 체크에 노출하는 수 금지)는 넣지 않는다.
    // 카드 행동은 효과 목록이 비어 있으므로 여기서 집계하지 않는다.
    std::vector<moveAction> allLegalActions(colorType color);

    // color 쪽 왕족(킹 또는 isRoyal 표식 기물) 수.
    int countRoyals(colorType color) const;

    // 게임 결과. 왕족의 실제 제거로만 판정한다. (docs/GAME-RULES.md §3)
    GameResult result() const;

    bool is_terminal() const
    {
        return result() != GameResult::ONGOING;
    }

    // ---- 상태 구성 / 조회 helper (테스트와 초기 배치용) ----
    void addPiece(Coord anchor, const Piece& piece)
    {
        board.push_back(Square{piece, anchor});
    }

    void addPlayerCard(colorType color, const Card& card)
    {
        (color == colorType::WHITE ? whitePlayerCards : blackPlayerCards)
            .push_back(card);
    }

    void addConstraint(const GameConstraint& constraint)
    {
        constraints.push_back(constraint);
    }

    void setTurn(colorType player, int actionsRemaining = 1)
    {
        turn.player = player;
        turn.actionsRemaining = actionsRemaining;
    }

    const TurnState& getTurn() const
    {
        return turn;
    }

    int getMoveCount() const
    {
        return moveCount;
    }

    int pieceCount() const
    {
        return static_cast<int>(board.size());
    }

    bool isCardUsed(colorType color, const std::string& cardId) const
    {
        const auto& hand =
            color == colorType::WHITE ? whitePlayerCards : blackPlayerCards;

        for (const Card& card : hand) {
            if (card.cardId == cardId && card.isUsed) {
                return true;
            }
        }

        return false;
    }

    // 턴 종료
    void endTurn()
    {
        if (turn.actionsRemaining > 0) {
            return;
        }

        if (turn.player == colorType::WHITE) {
            turn.player = colorType::BLACK;
        }
        else if (turn.player == colorType::BLACK) {
            turn.player = colorType::WHITE;
        }
        else {
            std::cerr << "턴 필드에 이상한 값이 들어감\n";
            return;
        }

        // 임시 기본값. 나중에는 getActionsPerTurn(turn.player) 등으로 교체.
        turn.actionsRemaining = 1;
    }
};

// ============================================================================
// 임시 helper 구현부 - 후에 피어리뷰 필요
// ============================================================================

bool AugmentChessGameState::isValidSquare(Coord pos) const
{
    return
        pos.first >= 1 && pos.first <= 8 &&
        pos.second >= 1 && pos.second <= 8;
}

bool AugmentChessGameState::isOccupied(Coord pos) const
{
    if (!isValidSquare(pos)) {
        return false;
    }

    return getOccupyingSquare(pos) != nullptr;
}

Square* AugmentChessGameState::getOccupyingSquare(Coord pos)
{
    for (auto& sq : board) {
        // anchor
        if (sq.coordinate == pos) {
            return &sq;
        }

        // footprint
        for (const Coord& offset : sq.curr_piece.Pv.footprint) {
            Coord occupiedPos = {
                sq.coordinate.first + offset.first,
                sq.coordinate.second + offset.second
            };

            if (occupiedPos == pos) {
                return &sq;
            }
        }
    }

    return nullptr;
}

const Square* AugmentChessGameState::getOccupyingSquare(Coord pos) const
{
    for (const auto& sq : board) {
        if (sq.coordinate == pos) {
            return &sq;
        }

        for (const Coord& offset : sq.curr_piece.Pv.footprint) {
            Coord occupiedPos = {
                sq.coordinate.first + offset.first,
                sq.coordinate.second + offset.second
            };

            if (occupiedPos == pos) {
                return &sq;
            }
        }
    }

    return nullptr;
}

Piece* AugmentChessGameState::getPieceAt(Coord pos)
{
    Square* sq = getOccupyingSquare(pos);

    if (sq == nullptr) {
        return nullptr;
    }

    return &sq->curr_piece;
}

const Piece* AugmentChessGameState::getPieceAt(Coord pos) const
{
    const Square* sq = getOccupyingSquare(pos);

    if (sq == nullptr) {
        return nullptr;
    }

    return &sq->curr_piece;
}

bool AugmentChessGameState::isPieceInsideBoard(
    const Square& square,
    Coord newAnchor
) const
{
    if (!isValidSquare(newAnchor)) {
        return false;
    }

    for (const Coord& offset : square.curr_piece.Pv.footprint) {
        Coord pos = {
            newAnchor.first + offset.first,
            newAnchor.second + offset.second
        };

        if (!isValidSquare(pos)) {
            return false;
        }
    }

    return true;
}

std::vector<const Square*>
AugmentChessGameState::getPlacementCollisions(
    const Square& movingSquare,
    Coord newAnchor
) const
{
    std::vector<const Square*> collisions;

    auto checkCell = [&](Coord pos) {
        const Square* target = getOccupyingSquare(pos);

        if (target == nullptr) {
            return;
        }

        // 이동 전 자기 자신의 몸과 겹치는 부분은 충돌로 보지 않는다.
        if (target->coordinate == movingSquare.coordinate) {
            return;
        }

        // 같은 기물을 여러 footprint 칸에서 발견해도 한 번만 추가한다.
        if (
            std::find(
                collisions.begin(),
                collisions.end(),
                target
            ) == collisions.end()
        ) {
            collisions.push_back(target);
        }
    };

    // anchor
    checkCell(newAnchor);

    // footprint
    for (const Coord& offset : movingSquare.curr_piece.Pv.footprint) {
        checkCell({
            newAnchor.first + offset.first,
            newAnchor.second + offset.second
        });
    }

    return collisions;
}

bool AugmentChessGameState::canCapture(
    const Piece& attacker,
    const Piece& target,
    moveType type
) const
{
    switch (type) {
        case moveType::BOTHTAKEMOVE:
            return true;

        case moveType::TAKE:
        case moveType::TAKEMOVE:
        case moveType::CATCH:
            // 현재 임시 기본 규칙:
            // 다른 색이면 포획 가능.
            // 중립/특수 상태/카드 예외는 나중에 여기서 처리한다.
            return attacker.cT != target.cT;

        default:
            return false;
    }
}

// ============================================================================
// moveChunk -> legal moveAction 해석
// ============================================================================
std::vector<moveAction>
AugmentChessGameState::interpretedPieceMoveChunk(
    const Square& curr_square
) {
    const Piece& piece = curr_square.curr_piece;
    const Coord origin = curr_square.coordinate;

    // 기본 행마 + 런타임 추가 행마
    std::vector<moveChunk> movements =
        getBaseMovement(piece.pT, piece.cT);

    movements.insert(
        movements.end(),
        piece.additional_mC.begin(),
        piece.additional_mC.end()
    );

    std::vector<moveAction> result;

    const bool noCapture = hasConstraint<NoCapture>(piece);
    const bool mustCapture = hasConstraint<MustCapture>(piece);

    // activateSquare의 각 항목은 OR로 계산한다.
    auto isActivated = [&](const moveChunk& chunk) -> bool {
        if (!chunk.activateSquare.has_value()) {
            return true;
        }

        for (const actCoord& condition : *chunk.activateSquare) {
            const bool xMatches =
                !condition.first.has_value() ||
                condition.first.value() == origin.first;

            const bool yMatches =
                !condition.second.has_value() ||
                condition.second.value() == origin.second;

            if (xMatches && yMatches) {
                return true;
            }
        }

        return false;
    };

    // 현재는 의도적으로 dedup하지 않는다.
    // 나중에 next/연쇄 행마가 들어오면 같은 종착지라도 다른 action일 수 있다.
    auto pushAction = [&](moveType type, Coord destination) {
        result.emplace_back(
            piece.cT,
            piece.pT,
            type,
            origin,
            destination
        );
    };

    for (const moveChunk& chunk : movements) {
        if (!isActivated(chunk)) {
            continue;
        }

        // 무한 ray에서 {0, 0} direction이면 무한루프가 되므로 방어.
        if (
            chunk.direction == Coord{0, 0} &&
            !chunk.maxDistance.has_value()
        ) {
            continue;
        }

        int distance = 1;

        // 판단(C-jump): JUMP case에 딸린 조각 - "적 기물을 이미 뛰어넘었는가"를 ray마다 들고 다니는 플래그 선언.
        // [JUDGMENT-BEGIN C-jump]
        //     bool jumpedOver = false;
        // [JUDGMENT-END C-jump]

        while (
            !chunk.maxDistance.has_value() ||
            distance <= chunk.maxDistance.value()
        ) {
            Coord destination = {
                origin.first + chunk.direction.first * distance,
                origin.second + chunk.direction.second * distance
            };

            bool stopRay = false;

            switch (chunk.mT) {
                // =========================================================
                // MOVE
                // 빈 곳으로만 이동한다.
                // =========================================================
                case moveType::MOVE:
                {
                    if (!isPieceInsideBoard(curr_square, destination)) {
                        stopRay = true;
                        break;
                    }

                    const auto collisions =
                        getPlacementCollisions(curr_square, destination);

                    if (!collisions.empty()) {
                        stopRay = true;
                        break;
                    }

                    if (!mustCapture) {
                        pushAction(moveType::MOVE, destination);
                    }

                    break;
                }

                // =========================================================
                // TAKE
                // 포획 가능한 기물이 있을 때만 그 자리로 이동한다.
                // 빈칸은 후보가 아니지만 ray 탐색은 계속한다.
                // =========================================================
                case moveType::TAKE:
                {
                    if (!isPieceInsideBoard(curr_square, destination)) {
                        stopRay = true;
                        break;
                    }

                    const auto collisions =
                        getPlacementCollisions(curr_square, destination);

                    if (collisions.empty()) {
                        break;
                    }

                    // 현재 일반 포획 행마는 한 번에 한 기물만 포획한다고 가정.
                    if (collisions.size() > 1) {
                        stopRay = true;
                        break;
                    }

                    const Piece& target = collisions.front()->curr_piece;

                    if (
                        !noCapture &&
                        canCapture(piece, target, moveType::TAKE)
                    ) {
                        pushAction(moveType::TAKE, destination);
                    }

                    stopRay = true;
                    break;
                }

                // =========================================================
                // CATCH
                // 목적지의 기물을 제거하지만 자신은 이동하지 않는다.
                // 따라서 moving piece의 footprint를 destination에 배치하지 않는다.
                // =========================================================
                case moveType::CATCH:
                {
                    if (!isValidSquare(destination)) {
                        stopRay = true;
                        break;
                    }

                    const Square* targetSquare =
                        getOccupyingSquare(destination);

                    if (targetSquare == nullptr) {
                        break;
                    }

                    const Piece& target = targetSquare->curr_piece;

                    if (
                        !noCapture &&
                        canCapture(piece, target, moveType::CATCH)
                    ) {
                        pushAction(moveType::CATCH, destination);
                    }

                    stopRay = true;
                    break;
                }

                // =========================================================
                // TAKEMOVE
                // 빈칸 -> 이동
                // 포획 가능한 기물 -> 포획 후 이동
                // =========================================================
                case moveType::TAKEMOVE:
                {
                    if (!isPieceInsideBoard(curr_square, destination)) {
                        stopRay = true;
                        break;
                    }

                    const auto collisions =
                        getPlacementCollisions(curr_square, destination);

                    if (collisions.empty()) {
                        if (!mustCapture) {
                            pushAction(moveType::TAKEMOVE, destination);
                        }

                        break;
                    }

                    if (collisions.size() > 1) {
                        stopRay = true;
                        break;
                    }

                    const Piece& target = collisions.front()->curr_piece;

                    if (
                        !noCapture &&
                        canCapture(piece, target, moveType::TAKEMOVE)
                    ) {
                        pushAction(moveType::TAKEMOVE, destination);
                    }

                    stopRay = true;
                    break;
                }

                // =========================================================
                // BOTHTAKEMOVE
                // 빈칸 -> 이동
                // 기물 존재 -> 색과 관계없이 포획 후 이동
                // =========================================================
                case moveType::BOTHTAKEMOVE:
                {
                    if (!isPieceInsideBoard(curr_square, destination)) {
                        stopRay = true;
                        break;
                    }

                    const auto collisions =
                        getPlacementCollisions(curr_square, destination);

                    if (collisions.empty()) {
                        if (!mustCapture) {
                            pushAction(moveType::BOTHTAKEMOVE, destination);
                        }

                        break;
                    }

                    if (collisions.size() > 1) {
                        stopRay = true;
                        break;
                    }

                    const Piece& target = collisions.front()->curr_piece;

                    if (
                        !noCapture &&
                        canCapture(piece, target, moveType::BOTHTAKEMOVE)
                    ) {
                        pushAction(moveType::BOTHTAKEMOVE, destination);
                    }

                    stopRay = true;
                    break;
                }

                // =========================================================
                // SHIFT
                // 기물끼리 위치를 교환하는 행마.
                // 양쪽 footprint의 교환 가능성 검사가 필요하므로 추후 구현.
                // =========================================================
                case moveType::SHIFT:
                {
                    // 판단(C-shift): SHIFT는 두 기물이 서로 자리(anchor)를 바꾸는 것으로 보고, 양쪽이 새 자리에서 몸 전체가 보드 안이고 제3의 기물과 안 겹치는지 상호 검사한다(교환은 포획이 아니므로 MustCapture면 제외).
                    // [JUDGMENT-BEGIN C-shift]
                    //     if (!isValidSquare(destination)) {
                    //         stopRay = true;
                    //         break;
                    //     }
                    //
                    //     const Square* other = getOccupyingSquare(destination);
                    //
                    //     // 빈칸에는 교환할 상대가 없다. 후보가 아니지만 ray 탐색은 계속한다.
                    //     if (other == nullptr) {
                    //         break;
                    //     }
                    //
                    //     // 기물의 몸(anchor + footprint)이 차지하는 칸 목록.
                    //     auto bodyCells = [](const Piece& body, Coord anchor) {
                    //         std::vector<Coord> cells = {anchor};
                    //         for (const Coord& offset : body.Pv.footprint) {
                    //             cells.push_back({
                    //                 anchor.first + offset.first,
                    //                 anchor.second + offset.second
                    //             });
                    //         }
                    //         return cells;
                    //     };
                    //
                    //     // 교환 후 자리: 움직이는 기물은 상대의 anchor로, 상대는 움직이는 기물의 원래 anchor로 간다.
                    //     const Coord moverNewAnchor = other->coordinate;
                    //     const Coord otherNewAnchor = curr_square.coordinate;
                    //
                    //     bool swappable =
                    //         isPieceInsideBoard(curr_square, moverNewAnchor) &&
                    //         isPieceInsideBoard(*other, otherNewAnchor);
                    //
                    //     // 새 자리에 두 기물 말고 다른 기물이 있으면 안 된다.
                    //     if (swappable) {
                    //         for (const Square* hit : getPlacementCollisions(curr_square, moverNewAnchor)) {
                    //             if (hit->coordinate != other->coordinate) {
                    //                 swappable = false;
                    //             }
                    //         }
                    //         for (const Square* hit : getPlacementCollisions(*other, otherNewAnchor)) {
                    //             if (hit->coordinate != curr_square.coordinate) {
                    //                 swappable = false;
                    //             }
                    //         }
                    //     }
                    //
                    //     // 교환 후 두 몸이 서로 겹쳐도 안 된다.
                    //     if (swappable) {
                    //         const std::vector<Coord> moverCells = bodyCells(piece, moverNewAnchor);
                    //         const std::vector<Coord> otherCells = bodyCells(other->curr_piece, otherNewAnchor);
                    //         for (const Coord& cell : moverCells) {
                    //             if (std::find(otherCells.begin(), otherCells.end(), cell) != otherCells.end()) {
                    //                 swappable = false;
                    //             }
                    //         }
                    //     }
                    //
                    //     if (swappable && !mustCapture) {
                    //         pushAction(moveType::SHIFT, destination);
                    //     }
                    //
                    //     // 처음 만난 기물이 교환 대상이든 아니든 ray는 거기서 막힌다.
                    //     stopRay = true;
                    //     break;
                    // [JUDGMENT-END C-shift]

                    // 판단 코드가 살아나기 전까지는 SHIFT 행마를 만들지 않는다. (위 블록을 살리면 여기는 도달하지 않는다.)
                    stopRay = true;
                    break;
                }

                // =========================================================
                // JUMP
                // 적 기물을 처음 만난 뒤부터 활성화되는 행마.
                // ray 안에 별도 상태값이 필요하므로 추후 구현.
                // =========================================================
                case moveType::JUMP:
                {
                    // 판단(C-jump): 처음 만나는 적 기물을 뛰어넘은 뒤의 빈칸부터만 착지 가능하고 포획은 하지 않으며, 뛰어넘기 전에 아군/중립 기물을 만나거나 뛰어넘은 뒤 또 기물을 만나면 ray가 막힌다.
                    // [JUDGMENT-BEGIN C-jump]
                    //     if (!isPieceInsideBoard(curr_square, destination)) {
                    //         stopRay = true;
                    //         break;
                    //     }
                    //
                    //     const auto collisions =
                    //         getPlacementCollisions(curr_square, destination);
                    //
                    //     if (collisions.empty()) {
                    //         // 이미 적 기물을 넘었다면 여기가 착지 후보, 아니면 아직 비활성 구간이다.
                    //         if (jumpedOver && !mustCapture) {
                    //             pushAction(moveType::JUMP, destination);
                    //         }
                    //
                    //         break;
                    //     }
                    //
                    //     // 한 번 넘은 뒤에 또 기물이 있으면 착지도, 재도약도 불가 (포획도 안 한다).
                    //     if (jumpedOver) {
                    //         stopRay = true;
                    //         break;
                    //     }
                    //
                    //     // 아직 넘기 전: 부딪힌 기물이 전부 적이어야 뛰어넘을 수 있다.
                    //     bool allEnemies = true;
                    //     for (const Square* hit : collisions) {
                    //         if (!canCapture(piece, hit->curr_piece, moveType::TAKE)) {
                    //             allEnemies = false;
                    //         }
                    //     }
                    //
                    //     if (!allEnemies) {
                    //         stopRay = true;
                    //         break;
                    //     }
                    //
                    //     jumpedOver = true;
                    //     break;
                    // [JUDGMENT-END C-jump]

                    // 판단 코드가 살아나기 전까지는 JUMP 행마를 만들지 않는다.
                    stopRay = true;
                    break;
                }
            }

            if (stopRay) {
                break;
            }

            ++distance;
        }

        // 판단(B-chain): moveAction은 start/destination 하나뿐이라 연쇄 결과를 "최종 목적지만" 담고(중간 경유지는 버림), 중간에 포획이 있거나 CATCH로 끝나는 연쇄는 최종 목적지만으로 재현할 수 없어 제외하며, chunk 자체의 단독 이동은 그대로 두고 연쇄 결과를 추가한다.
        // [JUDGMENT-BEGIN B-chain]
        //     if (!chunk.next.empty()) {
        //         for (const ChunkChain& chain : resolveChunkChains(curr_square, origin, chunk, !noCapture)) {
        //             const ChunkStep& last = chain.steps.back();
        //
        //             // CATCH로 끝나면 기물이 마지막에 어디 서 있는지 moveAction으로 표현할 수 없다.
        //             bool representable = last.type != moveType::CATCH;
        //
        //             // 마지막 전 단계에서 포획이 있었다면 최종 목적지만으로는 그 제거를 재현할 수 없다.
        //             for (std::size_t i = 0; i + 1 < chain.steps.size(); ++i) {
        //                 if (chain.steps[i].captures) {
        //                     representable = false;
        //                 }
        //             }
        //
        //             // MustCapture는 결과적으로 포획이 있는 연쇄만 허용한다.
        //             if (mustCapture && !last.captures) {
        //                 representable = false;
        //             }
        //
        //             if (representable) {
        //                 pushAction(last.type, last.destination);
        //             }
        //         }
        //     }
        // [JUDGMENT-END B-chain]
    }

    return result;
}

// ============================================================================
// moveChunk.next 연쇄 해석 (Phase B)
// ============================================================================

std::vector<ChunkStep>
AugmentChessGameState::resolveChunkSteps(
    const Square& movingSquare,
    Coord origin,
    const moveChunk& chunk,
    bool allowCapture
) const {
    const Piece& piece = movingSquare.curr_piece;

    std::vector<ChunkStep> steps;

    // 무한 ray에서 {0, 0} direction이면 무한루프가 되므로 방어.
    if (
        chunk.direction == Coord{0, 0} &&
        !chunk.maxDistance.has_value()
    ) {
        return steps;
    }

    int distance = 1;

    while (
        !chunk.maxDistance.has_value() ||
        distance <= chunk.maxDistance.value()
    ) {
        const Coord destination = {
            origin.first + chunk.direction.first * distance,
            origin.second + chunk.direction.second * distance
        };

        bool stopRay = false;

        switch (chunk.mT) {
            case moveType::MOVE:
            {
                if (!isPieceInsideBoard(movingSquare, destination)) {
                    stopRay = true;
                    break;
                }

                if (!getPlacementCollisions(movingSquare, destination).empty()) {
                    stopRay = true;
                    break;
                }

                steps.push_back({destination, moveType::MOVE, false});
                break;
            }

            case moveType::TAKE:
            {
                if (!isPieceInsideBoard(movingSquare, destination)) {
                    stopRay = true;
                    break;
                }

                const auto collisions =
                    getPlacementCollisions(movingSquare, destination);

                if (collisions.empty()) {
                    break;
                }

                if (
                    collisions.size() == 1 &&
                    allowCapture &&
                    canCapture(piece, collisions.front()->curr_piece, moveType::TAKE)
                ) {
                    steps.push_back({destination, moveType::TAKE, true});
                }

                stopRay = true;
                break;
            }

            case moveType::CATCH:
            {
                if (!isValidSquare(destination)) {
                    stopRay = true;
                    break;
                }

                const Square* targetSquare = getOccupyingSquare(destination);

                if (targetSquare == nullptr) {
                    break;
                }

                if (
                    allowCapture &&
                    canCapture(piece, targetSquare->curr_piece, moveType::CATCH)
                ) {
                    steps.push_back({destination, moveType::CATCH, true});
                }

                stopRay = true;
                break;
            }

            case moveType::TAKEMOVE:
            case moveType::BOTHTAKEMOVE:
            {
                if (!isPieceInsideBoard(movingSquare, destination)) {
                    stopRay = true;
                    break;
                }

                const auto collisions =
                    getPlacementCollisions(movingSquare, destination);

                if (collisions.empty()) {
                    steps.push_back({destination, chunk.mT, false});
                    break;
                }

                if (
                    collisions.size() == 1 &&
                    allowCapture &&
                    canCapture(piece, collisions.front()->curr_piece, chunk.mT)
                ) {
                    steps.push_back({destination, chunk.mT, true});
                }

                stopRay = true;
                break;
            }

            // 연쇄 안의 SHIFT/JUMP는 아직 지원하지 않는다. (step을 만들지 않고 ray를 끝낸다.)
            case moveType::SHIFT:
            case moveType::JUMP:
            {
                stopRay = true;
                break;
            }
        }

        if (stopRay) {
            break;
        }

        ++distance;
    }

    return steps;
}

std::vector<ChunkChain>
AugmentChessGameState::resolveChunkChains(
    const Square& movingSquare,
    Coord origin,
    const moveChunk& chunk,
    bool allowCapture
) const {
    std::vector<ChunkChain> chains;

    for (const ChunkStep& step : resolveChunkSteps(movingSquare, origin, chunk, allowCapture)) {
        // 더 이어질 chunk가 없으면 이 step으로 끝나는 경로다.
        if (chunk.next.empty()) {
            chains.push_back(ChunkChain{{step}});
            continue;
        }

        // 직전 chunk가 성공한 위치가 다음 chunk의 origin이다. CATCH는 이동하지 않는다.
        const Coord nextOrigin =
            step.type == moveType::CATCH ? origin : step.destination;

        for (const moveChunk& nextChunk : chunk.next) {
            if (!isChunkActivatedAt(nextChunk, nextOrigin)) {
                continue;
            }

            for (ChunkChain tail : resolveChunkChains(movingSquare, nextOrigin, nextChunk, allowCapture)) {
                tail.steps.insert(tail.steps.begin(), step);
                chains.push_back(std::move(tail));
            }
        }
    }

    return chains;
}

// ============================================================================
// 보드 조작 helper / 왕족 집계 / 종료 판정
// ============================================================================

Square* AugmentChessGameState::findSquareAtAnchor(Coord pos)
{
    for (auto& sq : board) {
        if (sq.coordinate == pos) {
            return &sq;
        }
    }

    return nullptr;
}

void AugmentChessGameState::removeAnchors(const std::vector<Coord>& anchors)
{
    board.erase(
        std::remove_if(
            board.begin(),
            board.end(),
            [&](const Square& sq) {
                return std::find(
                    anchors.begin(),
                    anchors.end(),
                    sq.coordinate
                ) != anchors.end();
            }
        ),
        board.end()
    );
}

int AugmentChessGameState::countRoyals(colorType color) const
{
    int count = 0;

    for (const Square& sq : board) {
        if (sq.curr_piece.cT == color && sq.curr_piece.isRoyalPiece()) {
            ++count;
        }
    }

    return count;
}

// docs/GAME-RULES.md §3: 기본 승리 조건은 체크메이트가 아니라 상대 왕족 기물의 실제 제거다.
// 체크/체크메이트/스테일메이트 개념과 "자기 왕 노출 수 금지" 가정은 넣지 않는다.
// 양쪽 왕족이 동시에 없어지면 무승부(§3). regency, democracy 같은 예외 승리 조건은 아직 없다.
GameResult AugmentChessGameState::result() const
{
    const bool whiteHasRoyal = countRoyals(colorType::WHITE) > 0;
    const bool blackHasRoyal = countRoyals(colorType::BLACK) > 0;

    if (whiteHasRoyal && blackHasRoyal) {
        return GameResult::ONGOING;
    }

    if (whiteHasRoyal) {
        return GameResult::WHITE_WIN;
    }

    if (blackHasRoyal) {
        return GameResult::BLACK_WIN;
    }

    return GameResult::DRAW;
}

// ============================================================================
// 앙파상 / 캐슬링 (Phase F) - docs/GAME-RULES.md §2: 기본 지원
// ============================================================================

std::vector<moveAction>
AugmentChessGameState::enPassantActions(const Square& pawnSquare) const
{
    std::vector<moveAction> actions;
    const Piece& pawn = pawnSquare.curr_piece;

    if (!enPassant.has_value() || pawn.pT != pieceType::PAWN) {
        return actions;
    }

    // 상대 폰이 방금 두 칸 전진했을 때만, 그 폰과 같은 랭크에서 바로 옆 파일에 있는 폰이 잡을 수 있다.
    const int forward = pawn.cT == colorType::WHITE ? 1 : -1;

    if (
        enPassant->pawnColor != pawn.cT &&
        pawnSquare.coordinate.second == enPassant->pawn.second &&
        std::abs(pawnSquare.coordinate.first - enPassant->pawn.first) == 1 &&
        enPassant->square.second == pawnSquare.coordinate.second + forward
    ) {
        actions.emplace_back(
            pawn.cT,
            pawn.pT,
            moveType::TAKE,
            pawnSquare.coordinate,
            enPassant->square
        );
    }

    return actions;
}

std::vector<moveAction>
AugmentChessGameState::castlingActions(const Square& kingSquare) const
{
    std::vector<moveAction> actions;
    const Piece& king = kingSquare.curr_piece;
    const Coord kingAt = kingSquare.coordinate;

    // 표준 배치(킹 e파일)에서 킹과 룩이 한 번도 움직이지 않았을 때만 성립하는 기본 캐슬링이다.
    if (king.pT != pieceType::KING || king.moveCount != 0 || kingAt.first != 5) {
        return actions;
    }

    for (const int side : {-1, 1}) {
        const int rookFile = side < 0 ? 1 : 8;
        const Coord rookAt = {rookFile, kingAt.second};

        const Square* rookSquare = getOccupyingSquare(rookAt);

        if (
            rookSquare == nullptr ||
            rookSquare->coordinate != rookAt ||
            rookSquare->curr_piece.pT != pieceType::ROOK ||
            rookSquare->curr_piece.cT != king.cT ||
            rookSquare->curr_piece.moveCount != 0
        ) {
            continue;
        }

        // 킹과 룩 사이 칸은 전부 비어 있어야 한다.
        bool pathClear = true;

        for (int file = std::min(kingAt.first, rookFile) + 1;
             file < std::max(kingAt.first, rookFile);
             ++file) {
            if (!isEmpty({file, kingAt.second})) {
                pathClear = false;
            }
        }

        if (!pathClear) {
            continue;
        }

        const Coord kingDestination = {kingAt.first + 2 * side, kingAt.second};

        // 판단(F-castle-attack-check): docs §3이 킹 세이프티를 임의로 넣지 말라고 했으므로 기본 캐슬링은 통과/도착 칸의 피공격 여부를 보지 않고, 표준 체스식 "출발/통과/도착 칸이 공격받으면 캐슬링 불가"는 판단 코드로 남긴다.
        // [JUDGMENT-BEGIN F-castle-attack-check]
        //     // 그 칸이 공격받는지: 복사본 보드에 왕 쪽 기물을 놓고, 상대가 그 칸을 포획하는 행동이 있는지 본다.
        //     auto isAttacked = [&](Coord square) -> bool {
        //         AugmentChessGameState probe = *this;
        //         probe.removeAnchors({square});
        //         probe.addPiece(square, Piece(king.cT, pieceType::KING, PieceVolume{}));
        //
        //         const colorType enemy = opponentOf(king.cT);
        //
        //         for (std::size_t i = 0; i < probe.board.size(); ++i) {
        //             if (probe.board[i].curr_piece.cT != enemy) {
        //                 continue;
        //             }
        //
        //             for (const moveAction& attack : probe.interpretedPieceMoveChunk(probe.board[i])) {
        //                 const bool captureType =
        //                     attack.mT == moveType::TAKE ||
        //                     attack.mT == moveType::TAKEMOVE ||
        //                     attack.mT == moveType::BOTHTAKEMOVE ||
        //                     attack.mT == moveType::CATCH;
        //
        //                 if (captureType && attack.destination == square) {
        //                     return true;
        //                 }
        //             }
        //         }
        //
        //         return false;
        //     };
        //
        //     const Coord passedSquare = {kingAt.first + side, kingAt.second};
        //
        //     if (
        //         isAttacked(kingAt) ||
        //         isAttacked(passedSquare) ||
        //         isAttacked(kingDestination)
        //     ) {
        //         continue;
        //     }
        // [JUDGMENT-END F-castle-attack-check]

        actions.emplace_back(
            king.cT,
            king.pT,
            moveType::MOVE,
            kingAt,
            kingDestination
        );
    }

    return actions;
}

// ============================================================================
// 전체 합법 행동 집계 (Phase E 신설, Phase F/G 연결)
// ============================================================================

std::vector<moveAction>
AugmentChessGameState::allLegalActions(colorType color)
{
    std::vector<moveAction> actions;

    // docs/GAME-RULES.md §3: interpretedPieceMoveChunk가 만든 수는 이미 실제 합법수로 취급한다.
    // 체크 여부로 걸러내지 않는다.
    for (const Square& sq : board) {
        if (sq.curr_piece.cT != color) {
            continue;
        }

        std::vector<moveAction> generated = interpretedPieceMoveChunk(sq);

        const std::vector<moveAction> enPassantMoves = enPassantActions(sq);
        const std::vector<moveAction> castlingMoves = castlingActions(sq);

        generated.insert(generated.end(), enPassantMoves.begin(), enPassantMoves.end());
        generated.insert(generated.end(), castlingMoves.begin(), castlingMoves.end());

        actions.insert(actions.end(), generated.begin(), generated.end());
    }

    // 판단(F-promo-choice): 프로모션 랭크에 도달하는 수는 apply 쪽 기본값(promotion_pool 첫 항목)에 맡기지 않고 promotion_pool의 항목마다 별도 행동으로 펼쳐 AI가 승격 기물을 고르게 한다.
    // [JUDGMENT-BEGIN F-promo-choice]
    //     {
    //         std::vector<moveAction> expanded;
    //
    //         for (const moveAction& action : actions) {
    //             const Piece* mover = getPieceAt(action.start);
    //
    //             const bool relocating =
    //                 action.mT == moveType::MOVE ||
    //                 action.mT == moveType::TAKE ||
    //                 action.mT == moveType::TAKEMOVE ||
    //                 action.mT == moveType::BOTHTAKEMOVE;
    //
    //             const bool promotes =
    //                 mover != nullptr &&
    //                 relocating &&
    //                 mover->pT == pieceType::PAWN &&
    //                 !mover->promotion_pool.empty() &&
    //                 action.destination.second == promotionRankOf(action.cT);
    //
    //             if (!promotes) {
    //                 expanded.push_back(action);
    //                 continue;
    //             }
    //
    //             for (const pieceType choice : mover->promotion_pool) {
    //                 moveAction promoted = action;
    //                 promoted.promotion = choice;
    //                 expanded.push_back(promoted);
    //             }
    //         }
    //
    //         actions = std::move(expanded);
    //     }
    // [JUDGMENT-END F-promo-choice]

    // Phase G: ForcedPiece 제약이 걸려 있으면 그 기물의 행동만 남긴다.
    // docs/GAME-RULES.md §2: 강제 추가 이동이 있으면 다른 기물을 선택할 수 없을 수 있다.
    for (const GameConstraint& constraint : constraints) {
        const ForcedPiece* forced = std::get_if<ForcedPiece>(&constraint);

        // 남은 발동 횟수가 없으면 이미 끝난 제약이다.
        if (forced == nullptr || forced->remainingTriggers <= 0) {
            continue;
        }

        // 지정된 기물이 없어졌거나 상대 기물이면 이 플레이어의 행동을 제한하지 않는다.
        const Piece* target = getPieceAt(forced->piece.position);

        if (
            target == nullptr ||
            target->cT != color ||
            target->pT != forced->piece.type
        ) {
            continue;
        }

        actions.erase(
            std::remove_if(
                actions.begin(),
                actions.end(),
                [&](const moveAction& action) {
                    return !(
                        action.start == forced->piece.position &&
                        action.pT == forced->piece.type
                    );
                }
            ),
            actions.end()
        );
    }

    return actions;
}

// ============================================================================
// apply_action
// ============================================================================

// docs/GAME-RULES.md §2, §4: 기물 이동은 행동 횟수를 소모하고 기본 상태에서는 상대에게 턴을 넘긴다.
// isTurnUsed=false인 행동(무료 행동)은 행동 횟수를 소모하지 않아 턴이 유지된다.
// 이 함수는 합법 행동이 들어온다고 가정한다. 합법성 검사는 하지 않는다.
void AugmentChessGameState::apply_action(const moveAction& action)
{
    Square* mover = nullptr;

    for (auto& sq : board) {
        if (
            sq.coordinate == action.start &&
            sq.curr_piece.cT == action.cT &&
            sq.curr_piece.pT == action.pT
        ) {
            mover = &sq;
            break;
        }
    }

    if (mover == nullptr) {
        std::cerr << "apply_action: 출발 칸에 해당 기물이 없음\n";
        return;
    }

    // 앙파상 기회는 "직후 한 행동"에만 유효하다. 이번 행동을 처리하는 동안만 쓰고 비운다.
    const std::optional<EnPassantTarget> epWindow = enPassant;
    enPassant.reset();

    // 승격이나 캐슬링 판정은 이동 전 기물 상태를 봐야 한다.
    const Coord start = action.start;
    const int movesBefore = mover->curr_piece.moveCount;

    bool relocated = false;
    Coord landing = start; // 행동 후 이 기물의 anchor

    switch (action.mT) {
        case moveType::MOVE:
        case moveType::TAKE:
        case moveType::TAKEMOVE:
        case moveType::BOTHTAKEMOVE:
        {
            std::vector<Coord> captured;

            for (const Square* hit : getPlacementCollisions(*mover, action.destination)) {
                captured.push_back(hit->coordinate);
            }

            if (action.mT == moveType::MOVE && !captured.empty()) {
                std::cerr << "apply_action: MOVE 목적지가 비어 있지 않음\n";
                enPassant = epWindow; // 아무 일도 없었던 것으로 되돌린다.
                return;
            }

            // 앙파상: 폰이 빈 앙파상 칸으로 대각선 포획 이동을 하면 지나간 폰을 제거한다.
            if (
                captured.empty() &&
                action.pT == pieceType::PAWN &&
                (action.mT == moveType::TAKE || action.mT == moveType::TAKEMOVE) &&
                epWindow.has_value() &&
                epWindow->pawnColor != action.cT &&
                epWindow->square == action.destination
            ) {
                captured.push_back(epWindow->pawn);
            }

            removeAnchors(captured);

            // 제거로 vector가 바뀌었을 수 있으므로 mover를 다시 찾는다.
            mover = findSquareAtAnchor(start);
            mover->coordinate = action.destination;

            relocated = true;
            landing = action.destination;
            break;
        }

        case moveType::CATCH:
        {
            // 목적지의 기물만 제거하고 자신은 이동하지 않는다.
            const Square* target = getOccupyingSquare(action.destination);

            if (target == nullptr) {
                std::cerr << "apply_action: CATCH 목적지에 기물이 없음\n";
                enPassant = epWindow;
                return;
            }

            removeAnchors({target->coordinate});
            mover = findSquareAtAnchor(start);
            break;
        }

        // 판단(C-shift): 위 interpretedPieceMoveChunk의 SHIFT 판단(두 기물의 anchor 교환)을 적용하는 쪽 조각.
        // [JUDGMENT-BEGIN C-shift]
        //     case moveType::SHIFT:
        //     {
        //         Square* other = getOccupyingSquare(action.destination);
        //
        //         if (other == nullptr || other == mover) {
        //             std::cerr << "apply_action: SHIFT 상대 기물이 없음\n";
        //             enPassant = epWindow;
        //             return;
        //         }
        //
        //         std::swap(mover->coordinate, other->coordinate);
        //         ++other->curr_piece.moveCount;
        //
        //         relocated = true;
        //         landing = mover->coordinate;
        //         break;
        //     }
        // [JUDGMENT-END C-shift]

        // 판단(C-jump): 위 JUMP 판단(포획 없이 빈 착지 칸으로만 이동)을 적용하는 쪽 조각.
        // [JUDGMENT-BEGIN C-jump]
        //     case moveType::JUMP:
        //     {
        //         if (!getPlacementCollisions(*mover, action.destination).empty()) {
        //             std::cerr << "apply_action: JUMP 목적지가 비어 있지 않음\n";
        //             enPassant = epWindow;
        //             return;
        //         }
        //
        //         mover->coordinate = action.destination;
        //
        //         relocated = true;
        //         landing = action.destination;
        //         break;
        //     }
        // [JUDGMENT-END C-jump]

        default:
        {
            std::cerr << "apply_action: 지원하지 않는 moveType\n";
            enPassant = epWindow;
            return;
        }
    }

    if (relocated) {
        // 기물별 이동 횟수: 실제로 자리를 옮긴 경우에만 올린다. ("첫 이동에만 가능" 조건용)
        ++mover->curr_piece.moveCount;

        // 캐슬링: 킹이 처음으로 두 칸 옆으로 가면 해당 쪽 룩이 킹 안쪽 칸으로 넘어온다.
        if (
            action.mT == moveType::MOVE &&
            action.pT == pieceType::KING &&
            movesBefore == 0 &&
            start.first == 5 &&
            landing.second == start.second &&
            std::abs(landing.first - start.first) == 2
        ) {
            const int rookFile = landing.first > start.first ? 8 : 1;
            Square* rook = findSquareAtAnchor({rookFile, start.second});

            if (
                rook != nullptr &&
                rook->curr_piece.pT == pieceType::ROOK &&
                rook->curr_piece.cT == action.cT &&
                rook->curr_piece.moveCount == 0
            ) {
                rook->coordinate = {(start.first + landing.first) / 2, start.second};
                ++rook->curr_piece.moveCount;
            }
        }

        // 앙파상 기회 기록: 폰이 MOVE로 정확히 두 칸 전진하면 지나간 칸을 남긴다.
        if (
            action.mT == moveType::MOVE &&
            action.pT == pieceType::PAWN &&
            landing.first == start.first &&
            std::abs(landing.second - start.second) == 2
        ) {
            enPassant = EnPassantTarget{
                {start.first, (start.second + landing.second) / 2},
                landing,
                action.cT
            };
        }

        // 프로모션: 승격 풀이 있는 폰이 상대 쪽 끝 랭크에 서면 승격한다.
        // action.promotion이 풀에 없거나 비어 있으면 풀의 첫 항목으로 승격한다.
        const bool relocatingMove =
            action.mT == moveType::MOVE ||
            action.mT == moveType::TAKE ||
            action.mT == moveType::TAKEMOVE ||
            action.mT == moveType::BOTHTAKEMOVE;

        Piece& movedPiece = mover->curr_piece;

        if (
            relocatingMove &&
            movedPiece.pT == pieceType::PAWN &&
            !movedPiece.promotion_pool.empty() &&
            landing.second == promotionRankOf(movedPiece.cT)
        ) {
            pieceType chosen = movedPiece.promotion_pool.front();

            if (
                action.promotion.has_value() &&
                std::find(
                    movedPiece.promotion_pool.begin(),
                    movedPiece.promotion_pool.end(),
                    *action.promotion
                ) != movedPiece.promotion_pool.end()
            ) {
                chosen = *action.promotion;
            }

            movedPiece.pT = chosen;
        }
    }

    // 판단(G-forced-consume): ForcedPiece의 remainingTriggers를 "강제 기물이 행동한 횟수"로 보고, 강제 기물이 움직이면 1 줄이고 위치/종류를 따라가며 0이 되면 제약을 제거한다.
    // [JUDGMENT-BEGIN G-forced-consume]
    //     for (auto it = constraints.begin(); it != constraints.end();) {
    //         ForcedPiece* forced = std::get_if<ForcedPiece>(&*it);
    //
    //         if (
    //             forced != nullptr &&
    //             forced->remainingTriggers > 0 &&
    //             forced->piece.position == start &&
    //             forced->piece.type == action.pT
    //         ) {
    //             --forced->remainingTriggers;
    //
    //             if (forced->remainingTriggers <= 0) {
    //                 it = constraints.erase(it);
    //                 continue;
    //             }
    //
    //             forced->piece.position = landing;
    //             forced->piece.type = mover->curr_piece.pT;
    //         }
    //
    //         ++it;
    //     }
    // [JUDGMENT-END G-forced-consume]

    // 완료된 이동 행동 수 (docs/GAME-RULES.md §4, §7)
    ++moveCount;

    if (action.isTurnUsed) {
        --turn.actionsRemaining;
    }

    endTurn();
}

// docs/GAME-RULES.md §5: 카드는 사용 즉시 used=true가 되고, 카드 사용은 일반적으로 턴을 넘기지 않는 무료 행동이다.
// (§2) 그래서 턴 소비 여부는 action.isTurnUsed를 그대로 따른다. 카드 효과 자체는 cardEffectRegistry()에서 찾는다.
// 효과가 등록되지 않은 카드는 아무것도 바꾸지 않는다.
void AugmentChessGameState::apply_action(const cardAction& action)
{
    if (action.cT != colorType::WHITE && action.cT != colorType::BLACK) {
        std::cerr << "apply_action: 카드 사용자가 백/흑이 아님\n";
        return;
    }

    std::vector<Card>& hand =
        action.cT == colorType::WHITE ? whitePlayerCards : blackPlayerCards;

    Card* inHand = nullptr;

    for (Card& card : hand) {
        if (card.cardId == action.card.cardId && !card.isUsed) {
            inHand = &card;
            break;
        }
    }

    if (inHand == nullptr) {
        std::cerr << "apply_action: 패에 없거나 이미 사용한 카드\n";
        return;
    }

    const auto& registry = cardEffectRegistry();
    const auto effect = registry.find(action.card.cardId);

    if (effect == registry.end()) {
        std::cerr << "apply_action: 효과가 등록되지 않은 카드\n";
        return;
    }

    inHand->isUsed = true;
    effect->second(*this, action);

    if (action.isTurnUsed) {
        --turn.actionsRemaining;
    }

    endTurn();
}

// ============================================================================
// api 테스트용 메인함수.
//
// 표준 출력에 ok/FAIL을 찍고, 하나라도 실패하면 0이 아닌 값을 반환한다.
// 판단 코드(주석 처리된 블록)에 의존하는 검사는 같은 이름의 [JUDGMENT-BEGIN 이름] 블록 안에
// 있어서, 블록을 살릴 때 함께 살아난다. 판단에 따라 결과가 달라지는 검사는 기대값 변수를
// 기본값으로 두고, 같은 이름의 블록이 그 기대값을 덮어쓴다.
// ============================================================================
namespace {

int g_failures = 0;

void check(bool ok, const char* what)
{
    if (ok) {
        std::cout << "ok:   " << what << "\n";
    }
    else {
        ++g_failures;
        std::cout << "FAIL: " << what << "\n";
    }
}

Piece makePiece(colorType color, pieceType type)
{
    return Piece(color, type, PieceVolume{});
}

Piece makeKing(colorType color)
{
    Piece king = makePiece(color, pieceType::KING);
    king.isKing = true;
    return king;
}

bool hasAction(
    const std::vector<moveAction>& actions,
    Coord start,
    Coord destination,
    moveType type
) {
    for (const moveAction& action : actions) {
        if (
            action.start == start &&
            action.destination == destination &&
            action.mT == type
        ) {
            return true;
        }
    }

    return false;
}

[[maybe_unused]] int countType(
    const std::vector<moveAction>& actions,
    moveType type
) {
    int count = 0;

    for (const moveAction& action : actions) {
        if (action.mT == type) {
            ++count;
        }
    }

    return count;
}

// 모든 행동의 출발 칸이 start인가.
[[maybe_unused]] bool allStartAt(
    const std::vector<moveAction>& actions,
    Coord start
) {
    for (const moveAction& action : actions) {
        if (action.start != start) {
            return false;
        }
    }

    return true;
}

} // namespace

int main(){
    // ---- Phase A: Piece 상태 필드 그릇 ----
    {
        Piece pawn = makePiece(colorType::WHITE, pieceType::PAWN);
        check(!pawn.status.frozen && !pawn.status.shielded, "A: status 기본값은 모두 false");

        pawn.status.frozen = true;
        check(pawn.status.frozen, "A: status.frozen 쓰기");
    }

    // ---- Phase B: moveChunk.next 연쇄 해석 (resolveChunkChains) ----
    {
        AugmentChessGameState s;
        const Piece knight = makePiece(colorType::WHITE, pieceType::KNIGHT);
        const Square knightSquare{knight, Coord{3, 2}};

        s.addPiece({3, 2}, knight);
        s.addPiece({4, 3}, makePiece(colorType::BLACK, pieceType::PAWN));

        // {3,2} -> MOVE {0,1} -> {3,3} -> CATCH {1,0} -> {4,3}의 적 기물
        moveChunk root(moveType::MOVE, Coord{0, 1});
        root.then(moveChunk(moveType::CATCH, Coord{1, 0}));

        const auto chains = s.resolveChunkChains(knightSquare, {3, 2}, root, true);

        check(
            chains.size() == 1 &&
            chains[0].steps.size() == 2 &&
            chains[0].steps[0].destination == Coord{3, 3} &&
            chains[0].steps[0].type == moveType::MOVE &&
            chains[0].steps[1].destination == Coord{4, 3} &&
            chains[0].steps[1].type == moveType::CATCH &&
            chains[0].steps[1].captures,
            "B: next는 직전 성공 위치를 origin으로 해석된다"
        );

        check(
            s.resolveChunkChains(knightSquare, {3, 2}, root, false).empty(),
            "B: NoCapture면 포획 step이 든 연쇄는 없다"
        );

        // next의 activateSquare는 직전 성공 위치(3,3) 기준으로 검사한다.
        moveChunk gated(moveType::MOVE, Coord{0, 1});
        gated.then(moveChunk(
            moveType::CATCH,
            Coord{1, 0},
            1,
            std::vector<actCoord>{{std::nullopt, 5}}
        ));

        check(
            s.resolveChunkChains(knightSquare, {3, 2}, gated, true).empty(),
            "B: next의 activateSquare가 맞지 않으면 연쇄가 끊긴다"
        );

        // 분기: root -> A, root -> B
        moveChunk branch(moveType::MOVE, Coord{0, 1});
        branch.then(moveChunk(moveType::MOVE, Coord{-1, 0}));
        branch.then(moveChunk(moveType::MOVE, Coord{0, 1}));

        check(
            s.resolveChunkChains(knightSquare, {3, 2}, branch, true).size() == 2,
            "B: next가 여러 개면 분기 수만큼 경로가 나온다"
        );

        // 3단 연쇄
        moveChunk triple(moveType::MOVE, Coord{0, 1});
        triple.then(moveChunk(moveType::MOVE, Coord{0, 1}))
              .then(moveChunk(moveType::MOVE, Coord{1, 0}));

        const auto tripleChains = s.resolveChunkChains(knightSquare, {3, 2}, triple, true);

        check(
            tripleChains.size() == 1 &&
            tripleChains[0].steps.size() == 3 &&
            tripleChains[0].steps[2].destination == Coord{4, 4},
            "B: 3단 연쇄를 재귀로 끝까지 따라간다"
        );

        // 첫 chunk가 막히면 연쇄 전체가 성립하지 않는다.
        AugmentChessGameState blocked;
        blocked.addPiece({3, 2}, knight);
        blocked.addPiece({3, 3}, makePiece(colorType::WHITE, pieceType::PAWN));

        check(
            blocked.resolveChunkChains(knightSquare, {3, 2}, root, true).empty(),
            "B: 첫 chunk가 막히면 next는 해석되지 않는다"
        );
    }

    // 판단(B-chain)을 살렸을 때만 의미가 있는 검사: 연쇄의 최종 목적지가 moveAction으로 나온다.
    // [JUDGMENT-BEGIN B-chain]
    //     {
    //         AugmentChessGameState s;
    //         Piece knight = makePiece(colorType::WHITE, pieceType::KNIGHT);
    //
    //         moveChunk root(moveType::MOVE, Coord{0, 1});
    //         root.then(moveChunk(moveType::TAKEMOVE, Coord{1, 0}));
    //         knight.addNewMovement(root);
    //
    //         s.addPiece({3, 2}, knight);
    //         s.addPiece({4, 3}, makePiece(colorType::BLACK, pieceType::PAWN));
    //
    //         const auto actions = s.allLegalActions(colorType::WHITE);
    //
    //         check(
    //             hasAction(actions, {3, 2}, {4, 3}, moveType::TAKEMOVE),
    //             "B-chain: 연쇄 결과는 최종 목적지만 담은 action이 된다"
    //         );
    //         check(
    //             hasAction(actions, {3, 2}, {3, 3}, moveType::MOVE),
    //             "B-chain: chunk 자체의 단독 이동은 그대로 남는다"
    //         );
    //
    //         s.apply_action(moveAction(
    //             colorType::WHITE, pieceType::KNIGHT, moveType::TAKEMOVE, {3, 2}, {4, 3}
    //         ));
    //
    //         const Piece* landed = s.getPieceAt({4, 3});
    //         check(
    //             landed != nullptr && landed->cT == colorType::WHITE && s.pieceCount() == 1,
    //             "B-chain: 연쇄 action 적용 결과(최종 칸 도착 + 포획)"
    //         );
    //     }
    //
    //     {
    //         // CATCH로 끝나는 연쇄는 최종 목적지만으로 표현할 수 없어 action이 되지 않는다.
    //         AugmentChessGameState s;
    //         Piece knight = makePiece(colorType::WHITE, pieceType::KNIGHT);
    //
    //         moveChunk root(moveType::MOVE, Coord{0, 1});
    //         root.then(moveChunk(moveType::CATCH, Coord{1, 0}));
    //         knight.addNewMovement(root);
    //
    //         s.addPiece({3, 2}, knight);
    //         s.addPiece({4, 3}, makePiece(colorType::BLACK, pieceType::PAWN));
    //
    //         check(
    //             !hasAction(s.allLegalActions(colorType::WHITE), {3, 2}, {4, 3}, moveType::CATCH),
    //             "B-chain: CATCH로 끝나는 연쇄는 action으로 만들지 않는다"
    //         );
    //     }
    // [JUDGMENT-END B-chain]

    // 판단(C-shift)을 살렸을 때만 의미가 있는 검사: 두 기물 교환과 양쪽 footprint 상호 검사.
    // [JUDGMENT-BEGIN C-shift]
    //     {
    //         AugmentChessGameState s;
    //         Piece bishop = makePiece(colorType::WHITE, pieceType::BISHOP);
    //         bishop.addNewMovement(moveChunk(moveType::SHIFT, Coord{1, 0}));
    //
    //         s.addPiece({1, 1}, bishop);
    //         s.addPiece({2, 1}, makePiece(colorType::WHITE, pieceType::KNIGHT));
    //
    //         check(
    //             hasAction(s.allLegalActions(colorType::WHITE), {1, 1}, {2, 1}, moveType::SHIFT),
    //             "C-shift: 인접한 두 기물은 자리를 바꿀 수 있다"
    //         );
    //
    //         s.apply_action(moveAction(
    //             colorType::WHITE, pieceType::BISHOP, moveType::SHIFT, {1, 1}, {2, 1}
    //         ));
    //
    //         const Piece* atFirst = s.getPieceAt({1, 1});
    //         const Piece* atSecond = s.getPieceAt({2, 1});
    //
    //         check(
    //             atFirst != nullptr && atFirst->pT == pieceType::KNIGHT &&
    //             atSecond != nullptr && atSecond->pT == pieceType::BISHOP &&
    //             s.pieceCount() == 2,
    //             "C-shift: 적용하면 두 기물의 anchor가 맞바뀌고 포획은 없다"
    //         );
    //     }
    //
    //     {
    //         // 다칸 기물과의 교환: 교환 후 두 몸이 (2,1)에서 겹치므로 불가능해야 한다.
    //         AugmentChessGameState s;
    //         Piece bishop = makePiece(colorType::WHITE, pieceType::BISHOP);
    //         bishop.addNewMovement(moveChunk(moveType::SHIFT, Coord{1, 0}));
    //
    //         Piece big = makePiece(colorType::WHITE, pieceType::ROOK);
    //         big.Pv.footprint = {{1, 0}, {0, 1}, {1, 1}};
    //
    //         s.addPiece({1, 1}, bishop);
    //         s.addPiece({2, 1}, big);
    //
    //         check(
    //             !hasAction(s.allLegalActions(colorType::WHITE), {1, 1}, {2, 1}, moveType::SHIFT),
    //             "C-shift: 교환 후 두 몸이 겹치면 교환할 수 없다"
    //         );
    //     }
    //
    //     {
    //         // 2x2 기물이 (1,8)로 옮겨 가면 몸이 위로 보드 밖으로 나가므로 불가능해야 한다.
    //         AugmentChessGameState s;
    //         Piece bishop = makePiece(colorType::WHITE, pieceType::BISHOP);
    //         bishop.addNewMovement(moveChunk(moveType::SHIFT, Coord{1, 0}));
    //
    //         Piece big = makePiece(colorType::BLACK, pieceType::ROOK);
    //         big.Pv.footprint = {{1, 0}, {0, 1}, {1, 1}};
    //
    //         s.addPiece({1, 8}, bishop);
    //         s.addPiece({2, 7}, big);
    //
    //         check(
    //             !hasAction(s.allLegalActions(colorType::WHITE), {1, 8}, {2, 8}, moveType::SHIFT),
    //             "C-shift: 상대 기물의 몸이 새 자리에서 보드 밖으로 나가면 교환할 수 없다"
    //         );
    //     }
    // [JUDGMENT-END C-shift]

    // 판단(C-jump)을 살렸을 때만 의미가 있는 검사: 처음 만나는 적 기물을 넘은 뒤 빈칸에만 착지.
    // [JUDGMENT-BEGIN C-jump]
    //     {
    //         AugmentChessGameState s;
    //         Piece bishop = makePiece(colorType::WHITE, pieceType::BISHOP);
    //         bishop.addNewMovement(moveChunk(moveType::JUMP, Coord{1, 0}, std::nullopt));
    //
    //         s.addPiece({1, 1}, bishop);
    //         s.addPiece({3, 1}, makePiece(colorType::BLACK, pieceType::PAWN));
    //         s.addPiece({6, 1}, makePiece(colorType::WHITE, pieceType::KNIGHT));
    //
    //         const auto actions = s.allLegalActions(colorType::WHITE);
    //
    //         check(
    //             countType(actions, moveType::JUMP) == 2 &&
    //             hasAction(actions, {1, 1}, {4, 1}, moveType::JUMP) &&
    //             hasAction(actions, {1, 1}, {5, 1}, moveType::JUMP),
    //             "C-jump: 적을 넘은 뒤 첫 빈칸부터 다음 기물 앞까지만 착지한다"
    //         );
    //         check(
    //             !hasAction(actions, {1, 1}, {2, 1}, moveType::JUMP) &&
    //             !hasAction(actions, {1, 1}, {3, 1}, moveType::JUMP),
    //             "C-jump: 적을 넘기 전 칸이나 적 기물 자리로는 착지하지 않는다(포획 없음)"
    //         );
    //
    //         s.apply_action(moveAction(
    //             colorType::WHITE, pieceType::BISHOP, moveType::JUMP, {1, 1}, {5, 1}
    //         ));
    //
    //         const Piece* landed = s.getPieceAt({5, 1});
    //         check(
    //             landed != nullptr && landed->pT == pieceType::BISHOP &&
    //             s.getPieceAt({3, 1}) != nullptr && s.pieceCount() == 3,
    //             "C-jump: 적용하면 이동만 하고 뛰어넘은 적 기물은 그대로다"
    //         );
    //     }
    //
    //     {
    //         // 넘기 전에 아군을 만나면 ray가 막힌다.
    //         AugmentChessGameState s;
    //         Piece bishop = makePiece(colorType::WHITE, pieceType::BISHOP);
    //         bishop.addNewMovement(moveChunk(moveType::JUMP, Coord{1, 0}, std::nullopt));
    //
    //         s.addPiece({1, 1}, bishop);
    //         s.addPiece({3, 1}, makePiece(colorType::WHITE, pieceType::KNIGHT));
    //         s.addPiece({5, 1}, makePiece(colorType::BLACK, pieceType::PAWN));
    //
    //         check(
    //             countType(s.allLegalActions(colorType::WHITE), moveType::JUMP) == 0,
    //             "C-jump: 넘기 전에 아군을 만나면 뛰어넘을 수 없다"
    //         );
    //     }
    // [JUDGMENT-END C-jump]

    // ---- Phase D: apply_action(moveAction) - 이동 / 포획 / CATCH / 턴 소비 ----
    {
        AugmentChessGameState s;
        s.addPiece({5, 1}, makeKing(colorType::WHITE));
        s.addPiece({5, 8}, makeKing(colorType::BLACK));
        s.addPiece({1, 2}, makePiece(colorType::WHITE, pieceType::ROOK));
        s.addPiece({1, 5}, makePiece(colorType::BLACK, pieceType::PAWN));

        const auto actions = s.allLegalActions(colorType::WHITE);

        check(
            hasAction(actions, {1, 2}, {1, 3}, moveType::TAKEMOVE) &&
            hasAction(actions, {1, 2}, {1, 5}, moveType::TAKEMOVE),
            "D: 룩의 이동과 포획 행동이 생성된다"
        );

        // 이동
        s.apply_action(moveAction(
            colorType::WHITE, pieceType::ROOK, moveType::TAKEMOVE, {1, 2}, {1, 3}
        ));

        const Piece* moved = s.getPieceAt({1, 3});

        check(
            moved != nullptr && moved->moveCount == 1 && s.getPieceAt({1, 2}) == nullptr,
            "D: 이동하면 기물이 옮겨지고 기물 moveCount가 오른다"
        );
        check(
            s.getMoveCount() == 1 &&
            s.getTurn().player == colorType::BLACK &&
            s.getTurn().actionsRemaining == 1,
            "D: 이동은 게임 moveCount를 올리고 상대에게 턴을 넘긴다"
        );

        // 포획
        s.setTurn(colorType::WHITE);
        s.apply_action(moveAction(
            colorType::WHITE, pieceType::ROOK, moveType::TAKEMOVE, {1, 3}, {1, 5}
        ));

        const Piece* captor = s.getPieceAt({1, 5});

        check(
            captor != nullptr && captor->cT == colorType::WHITE &&
            s.getPieceAt({1, 3}) == nullptr && s.pieceCount() == 3,
            "D: 포획하면 목적지 기물이 제거되고 이동한다"
        );
    }

    {
        // 턴 소비 반영: 무료 행동(isTurnUsed=false)과 행동 2회(acceleration)
        AugmentChessGameState s;
        s.addPiece({1, 1}, makePiece(colorType::WHITE, pieceType::ROOK));

        moveAction freeMove(
            colorType::WHITE, pieceType::ROOK, moveType::TAKEMOVE, {1, 1}, {1, 2}, false
        );
        s.apply_action(freeMove);

        check(
            s.getTurn().player == colorType::WHITE && s.getTurn().actionsRemaining == 1,
            "D: isTurnUsed=false인 행동은 턴을 소비하지 않는다"
        );

        s.setTurn(colorType::WHITE, 2);
        s.apply_action(moveAction(
            colorType::WHITE, pieceType::ROOK, moveType::TAKEMOVE, {1, 2}, {1, 3}
        ));

        check(
            s.getTurn().player == colorType::WHITE && s.getTurn().actionsRemaining == 1,
            "D: 행동 횟수가 2면 첫 이동 뒤에도 같은 플레이어 턴이다"
        );

        s.apply_action(moveAction(
            colorType::WHITE, pieceType::ROOK, moveType::TAKEMOVE, {1, 3}, {1, 4}
        ));

        check(
            s.getTurn().player == colorType::BLACK,
            "D: 행동 횟수를 다 쓰면 상대 턴이 된다"
        );
    }

    {
        // CATCH: 이동 없이 제거
        AugmentChessGameState s;
        Piece knight = makePiece(colorType::WHITE, pieceType::KNIGHT);
        knight.addNewMovement(moveChunk(moveType::CATCH, Coord{0, 1}));

        s.addPiece({4, 4}, knight);
        s.addPiece({4, 5}, makePiece(colorType::BLACK, pieceType::PAWN));

        check(
            hasAction(s.allLegalActions(colorType::WHITE), {4, 4}, {4, 5}, moveType::CATCH),
            "D: CATCH 행동이 생성된다"
        );

        s.apply_action(moveAction(
            colorType::WHITE, pieceType::KNIGHT, moveType::CATCH, {4, 4}, {4, 5}
        ));

        const Piece* stayed = s.getPieceAt({4, 4});

        check(
            s.getPieceAt({4, 5}) == nullptr && stayed != nullptr &&
            stayed->moveCount == 0 && s.pieceCount() == 1 &&
            s.getMoveCount() == 1 && s.getTurn().player == colorType::BLACK,
            "D: CATCH는 대상만 제거하고 이동하지 않으며 턴은 소비한다"
        );
    }

    // ---- Phase D: apply_action(cardAction) - cardId -> 효과 함수 매핑 골격 ----
    {
        AugmentChessGameState s;
        const Card card{CardActType::ACTIVE, CardType::MIDDLE, "test-card", false};
        s.addPlayerCard(colorType::WHITE, card);

        const cardAction use(colorType::WHITE, card, false);

        s.apply_action(use);
        check(!s.isCardUsed(colorType::WHITE, "test-card"), "D-card: 효과가 등록되지 않은 카드는 상태를 바꾸지 않는다");

        int calls = 0;
        registerCardEffect(
            "test-card",
            [&calls](AugmentChessGameState&, const cardAction&) { ++calls; }
        );

        s.apply_action(use);
        check(
            calls == 1 && s.isCardUsed(colorType::WHITE, "test-card"),
            "D-card: 등록된 효과가 호출되고 카드는 사용 처리된다"
        );
        check(
            s.getTurn().player == colorType::WHITE && s.getTurn().actionsRemaining == 1,
            "D-card: 무료 카드 사용은 턴을 넘기지 않는다"
        );

        s.apply_action(use);
        check(calls == 1, "D-card: 이미 사용한 카드는 다시 쓸 수 없다");

        // 턴을 소비하는 카드
        const Card costly{CardActType::ACTIVE, CardType::MIDDLE, "costly-card", false};
        s.addPlayerCard(colorType::WHITE, costly);
        registerCardEffect("costly-card", [](AugmentChessGameState&, const cardAction&) {});

        s.apply_action(cardAction(colorType::WHITE, costly, true));
        check(s.getTurn().player == colorType::BLACK, "D-card: isTurnUsed=true인 카드는 턴을 소비한다");
    }

    // ---- Phase E: 왕족 제거 기반 종료 판정 ----
    {
        AugmentChessGameState s;
        s.addPiece({5, 1}, makeKing(colorType::WHITE));
        s.addPiece({5, 8}, makeKing(colorType::BLACK));

        check(!s.is_terminal() && s.result() == GameResult::ONGOING, "E: 양쪽 왕족이 있으면 진행 중");

        AugmentChessGameState blackGone;
        blackGone.addPiece({5, 1}, makeKing(colorType::WHITE));
        blackGone.addPiece({5, 8}, makePiece(colorType::BLACK, pieceType::QUEEN));
        check(
            blackGone.is_terminal() && blackGone.result() == GameResult::WHITE_WIN,
            "E: 상대 왕족이 전부 제거되면 승리"
        );

        AugmentChessGameState whiteGone;
        whiteGone.addPiece({5, 8}, makeKing(colorType::BLACK));
        check(whiteGone.result() == GameResult::BLACK_WIN, "E: 흑 승리 판정");

        AugmentChessGameState both;
        both.addPiece({1, 1}, makePiece(colorType::WHITE, pieceType::ROOK));
        check(both.is_terminal() && both.result() == GameResult::DRAW, "E: 양쪽 왕족이 동시에 없으면 무승부");

        // isRoyal 표식 기물도 왕족으로 센다.
        AugmentChessGameState royal;
        Piece royalKnight = makePiece(colorType::WHITE, pieceType::KNIGHT);
        royalKnight.isRoyal = true;
        royal.addPiece({3, 3}, royalKnight);
        royal.addPiece({5, 8}, makeKing(colorType::BLACK));
        check(royal.result() == GameResult::ONGOING && royal.countRoyals(colorType::WHITE) == 1, "E: isRoyal 표식 기물도 왕족");

        // 킹을 잡는 수를 적용하면 즉시 종료된다.
        AugmentChessGameState capture;
        capture.addPiece({5, 1}, makeKing(colorType::WHITE));
        capture.addPiece({1, 1}, makePiece(colorType::WHITE, pieceType::ROOK));
        capture.addPiece({1, 8}, makeKing(colorType::BLACK));
        capture.apply_action(moveAction(
            colorType::WHITE, pieceType::ROOK, moveType::TAKEMOVE, {1, 1}, {1, 8}
        ));
        check(capture.result() == GameResult::WHITE_WIN, "E: 킹 포획 직후 종료");
    }

    {
        // 킹 세이프티를 넣지 않는다: 왕이 공격받는 중이어도, 왕을 지키지 않는 수가 합법이다. (docs §3)
        AugmentChessGameState s;
        s.addPiece({5, 1}, makeKing(colorType::WHITE));
        s.addPiece({1, 1}, makePiece(colorType::WHITE, pieceType::ROOK));
        s.addPiece({5, 8}, makePiece(colorType::BLACK, pieceType::ROOK));
        s.addPiece({8, 8}, makeKing(colorType::BLACK));

        check(
            hasAction(s.allLegalActions(colorType::WHITE), {1, 1}, {1, 2}, moveType::TAKEMOVE),
            "E: 체크를 무시하는 수도 합법수로 취급한다"
        );
    }

    // ---- Phase F: 프로모션 / 앙파상 / 캐슬링 ----
    {
        AugmentChessGameState s;
        Piece pawn = makePiece(colorType::WHITE, pieceType::PAWN);
        pawn.promotion_pool = {pieceType::QUEEN, pieceType::KNIGHT};

        s.addPiece({5, 1}, makeKing(colorType::WHITE));
        s.addPiece({5, 8}, makeKing(colorType::BLACK));
        s.addPiece({1, 7}, pawn);

        std::size_t expectedPromotionActions = 1;
        // 판단(F-promo-choice): 이 판단을 살리면 승격 기물마다 별도 행동이 되므로 위 기대값을 2(승격 풀 크기)로 덮어쓴다.
        // [JUDGMENT-BEGIN F-promo-choice]
        //     expectedPromotionActions = 2;
        // [JUDGMENT-END F-promo-choice]

        std::size_t promotionActions = 0;
        for (const moveAction& action : s.allLegalActions(colorType::WHITE)) {
            if (action.start == Coord{1, 7} && action.destination == Coord{1, 8}) {
                ++promotionActions;
            }
        }

        check(promotionActions == expectedPromotionActions, "F: 프로모션 랭크 도달 행동 수");

        AugmentChessGameState byDefault = s;
        byDefault.apply_action(moveAction(
            colorType::WHITE, pieceType::PAWN, moveType::MOVE, {1, 7}, {1, 8}
        ));
        const Piece* queen = byDefault.getPieceAt({1, 8});
        check(queen != nullptr && queen->pT == pieceType::QUEEN, "F: 승격 기물을 안 정하면 promotion_pool 첫 항목");

        AugmentChessGameState chosen = s;
        moveAction toKnight(colorType::WHITE, pieceType::PAWN, moveType::MOVE, {1, 7}, {1, 8});
        toKnight.promotion = pieceType::KNIGHT;
        chosen.apply_action(toKnight);
        const Piece* knight = chosen.getPieceAt({1, 8});
        check(knight != nullptr && knight->pT == pieceType::KNIGHT, "F: 지정한 승격 기물로 승격");

        AugmentChessGameState invalid = s;
        moveAction toRook(colorType::WHITE, pieceType::PAWN, moveType::MOVE, {1, 7}, {1, 8});
        toRook.promotion = pieceType::ROOK;
        invalid.apply_action(toRook);
        const Piece* fallback = invalid.getPieceAt({1, 8});
        check(fallback != nullptr && fallback->pT == pieceType::QUEEN, "F: 풀에 없는 승격 기물은 첫 항목으로 대체");
    }

    {
        // 앙파상
        AugmentChessGameState s;
        s.addPiece({8, 1}, makeKing(colorType::WHITE));
        s.addPiece({8, 8}, makeKing(colorType::BLACK));
        s.addPiece({5, 2}, makePiece(colorType::WHITE, pieceType::PAWN));
        s.addPiece({4, 4}, makePiece(colorType::BLACK, pieceType::PAWN));

        s.apply_action(moveAction(
            colorType::WHITE, pieceType::PAWN, moveType::MOVE, {5, 2}, {5, 4}
        ));

        AugmentChessGameState expired = s;

        check(
            hasAction(s.allLegalActions(colorType::BLACK), {4, 4}, {5, 3}, moveType::TAKE),
            "F: 두 칸 전진 직후 옆의 폰이 앙파상 행동을 갖는다"
        );

        s.apply_action(moveAction(
            colorType::BLACK, pieceType::PAWN, moveType::TAKE, {4, 4}, {5, 3}
        ));

        const Piece* taker = s.getPieceAt({5, 3});
        check(
            taker != nullptr && taker->cT == colorType::BLACK &&
            s.getPieceAt({5, 4}) == nullptr && s.pieceCount() == 3,
            "F: 앙파상을 적용하면 지나간 폰이 제거된다"
        );

        // 상대가 다른 수를 두면 앙파상 기회는 사라진다.
        expired.apply_action(moveAction(
            colorType::BLACK, pieceType::KING, moveType::TAKEMOVE, {8, 8}, {8, 7}
        ));
        expired.setTurn(colorType::BLACK);
        check(
            !hasAction(expired.allLegalActions(colorType::BLACK), {4, 4}, {5, 3}, moveType::TAKE),
            "F: 앙파상은 직후 한 행동에만 유효하다"
        );
    }

    {
        // 캐슬링
        AugmentChessGameState s;
        s.addPiece({5, 1}, makeKing(colorType::WHITE));
        s.addPiece({1, 1}, makePiece(colorType::WHITE, pieceType::ROOK));
        s.addPiece({8, 1}, makePiece(colorType::WHITE, pieceType::ROOK));
        s.addPiece({5, 8}, makeKing(colorType::BLACK));

        const auto actions = s.allLegalActions(colorType::WHITE);

        check(
            hasAction(actions, {5, 1}, {7, 1}, moveType::MOVE) &&
            hasAction(actions, {5, 1}, {3, 1}, moveType::MOVE),
            "F: 킹과 룩이 안 움직였고 사이가 비어 있으면 양쪽 캐슬링 행동이 나온다"
        );

        AugmentChessGameState castled = s;
        castled.apply_action(moveAction(
            colorType::WHITE, pieceType::KING, moveType::MOVE, {5, 1}, {7, 1}
        ));

        const Piece* castledKing = castled.getPieceAt({7, 1});
        const Piece* castledRook = castled.getPieceAt({6, 1});
        check(
            castledKing != nullptr && castledKing->pT == pieceType::KING &&
            castledRook != nullptr && castledRook->pT == pieceType::ROOK &&
            castled.getPieceAt({8, 1}) == nullptr,
            "F: 킹사이드 캐슬링을 적용하면 룩이 킹 안쪽 칸으로 넘어온다"
        );

        AugmentChessGameState queenSide = s;
        queenSide.apply_action(moveAction(
            colorType::WHITE, pieceType::KING, moveType::MOVE, {5, 1}, {3, 1}
        ));
        const Piece* queenRook = queenSide.getPieceAt({4, 1});
        check(
            queenRook != nullptr && queenRook->pT == pieceType::ROOK &&
            queenSide.getPieceAt({1, 1}) == nullptr,
            "F: 퀸사이드 캐슬링을 적용하면 룩이 d파일로 온다"
        );

        // 사이에 기물이 있거나 룩이 이미 움직였으면 불가능
        AugmentChessGameState blockedPath = s;
        blockedPath.addPiece({6, 1}, makePiece(colorType::WHITE, pieceType::BISHOP));
        check(
            !hasAction(blockedPath.allLegalActions(colorType::WHITE), {5, 1}, {7, 1}, moveType::MOVE),
            "F: 킹과 룩 사이에 기물이 있으면 캐슬링할 수 없다"
        );

        AugmentChessGameState movedRook;
        movedRook.addPiece({5, 1}, makeKing(colorType::WHITE));
        Piece rook = makePiece(colorType::WHITE, pieceType::ROOK);
        rook.moveCount = 1;
        movedRook.addPiece({8, 1}, rook);
        movedRook.addPiece({5, 8}, makeKing(colorType::BLACK));
        check(
            !hasAction(movedRook.allLegalActions(colorType::WHITE), {5, 1}, {7, 1}, moveType::MOVE),
            "F: 이미 움직인 룩과는 캐슬링할 수 없다"
        );

        // 통과 칸이 공격받는 캐슬링: 기본은 킹 세이프티를 넣지 않으므로 허용한다.
        AugmentChessGameState attacked = s;
        attacked.addPiece({6, 8}, makePiece(colorType::BLACK, pieceType::ROOK));

        bool expectKingsideThroughAttack = true;
        // 판단(F-castle-attack-check): 이 판단을 살리면 공격받는 통과 칸의 캐슬링이 불허되므로 위 기대값을 false로 덮어쓴다.
        // [JUDGMENT-BEGIN F-castle-attack-check]
        //     expectKingsideThroughAttack = false;
        // [JUDGMENT-END F-castle-attack-check]

        const auto attackedActions = attacked.allLegalActions(colorType::WHITE);

        check(
            hasAction(attackedActions, {5, 1}, {7, 1}, moveType::MOVE) == expectKingsideThroughAttack,
            "F: 통과 칸(f1)이 공격받는 킹사이드 캐슬링의 합법 여부"
        );
        check(
            hasAction(attackedActions, {5, 1}, {3, 1}, moveType::MOVE),
            "F: 공격받지 않는 쪽 캐슬링은 항상 가능"
        );
    }

    // ---- Phase G: ForcedPiece 제약 ----
    {
        AugmentChessGameState s;
        s.addPiece({5, 2}, makeKing(colorType::WHITE));
        s.addPiece({1, 1}, makePiece(colorType::WHITE, pieceType::ROOK));
        s.addPiece({7, 5}, makePiece(colorType::WHITE, pieceType::KNIGHT));
        s.addPiece({5, 8}, makeKing(colorType::BLACK));

        const auto unrestricted = s.allLegalActions(colorType::WHITE);
        check(
            hasAction(unrestricted, {1, 1}, {1, 2}, moveType::TAKEMOVE) &&
            hasAction(unrestricted, {7, 5}, {8, 7}, moveType::TAKEMOVE),
            "G: 제약이 없으면 모든 기물의 행동이 나온다"
        );

        AugmentChessGameState forced = s;
        forced.addConstraint(ForcedPiece{PieceTypeAt{pieceType::ROOK, {1, 1}}, 1});
        const auto onlyRook = forced.allLegalActions(colorType::WHITE);

        bool onlyRookMoves = !onlyRook.empty();
        for (const moveAction& action : onlyRook) {
            if (action.start != Coord{1, 1} || action.pT != pieceType::ROOK) {
                onlyRookMoves = false;
            }
        }
        check(onlyRookMoves, "G: ForcedPiece가 있으면 그 기물 외의 행동은 제외된다");

        // 상대 기물에 걸린 강제는 이 플레이어를 제한하지 않는다.
        AugmentChessGameState enemyForced = s;
        enemyForced.addConstraint(ForcedPiece{PieceTypeAt{pieceType::KING, {5, 8}}, 1});
        check(
            enemyForced.allLegalActions(colorType::WHITE).size() == unrestricted.size(),
            "G: 상대 기물에 걸린 ForcedPiece는 무시한다"
        );

        // 남은 횟수가 0이면 이미 끝난 제약이다.
        AugmentChessGameState spent = s;
        spent.addConstraint(ForcedPiece{PieceTypeAt{pieceType::ROOK, {1, 1}}, 0});
        check(
            spent.allLegalActions(colorType::WHITE).size() == unrestricted.size(),
            "G: remainingTriggers가 0이면 제한하지 않는다"
        );
    }

    // 판단(G-forced-consume)을 살렸을 때만 의미가 있는 검사: 강제 기물이 움직이면 횟수가 줄고 위치를 따라간다.
    // [JUDGMENT-BEGIN G-forced-consume]
    //     {
    //         AugmentChessGameState s;
    //         s.addPiece({5, 2}, makeKing(colorType::WHITE));
    //         s.addPiece({1, 1}, makePiece(colorType::WHITE, pieceType::ROOK));
    //         s.addPiece({7, 5}, makePiece(colorType::WHITE, pieceType::KNIGHT));
    //         s.addPiece({5, 8}, makeKing(colorType::BLACK));
    //         s.addConstraint(ForcedPiece{PieceTypeAt{pieceType::ROOK, {1, 1}}, 2});
    //
    //         s.apply_action(moveAction(
    //             colorType::WHITE, pieceType::ROOK, moveType::TAKEMOVE, {1, 1}, {1, 3}, false
    //         ));
    //         check(
    //             allStartAt(s.allLegalActions(colorType::WHITE), {1, 3}),
    //             "G-forced-consume: 강제 기물이 움직여도 새 위치의 그 기물만 계속 강제된다"
    //         );
    //
    //         s.apply_action(moveAction(
    //             colorType::WHITE, pieceType::ROOK, moveType::TAKEMOVE, {1, 3}, {1, 4}, false
    //         ));
    //         check(
    //             !allStartAt(s.allLegalActions(colorType::WHITE), {1, 4}),
    //             "G-forced-consume: 횟수를 다 쓰면 제약이 사라져 다른 기물도 움직일 수 있다"
    //         );
    //     }
    // [JUDGMENT-END G-forced-consume]

    std::cout << (g_failures == 0 ? "ALL PASSED" : "SOME TESTS FAILED") << "\n";
    return g_failures == 0 ? 0 : 1;
}

//g++ -std=c++20 -Wall -Wextra engine.cpp -o engine_test && ./engine_test

