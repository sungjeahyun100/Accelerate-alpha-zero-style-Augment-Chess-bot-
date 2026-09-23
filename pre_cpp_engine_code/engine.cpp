//ai한테 던져줄 rust 포팅용 cpp코드

//우선 타입 먼저 만들기
#include<iostream>
#include<vector>
#include<string>
#include<utility>
#include<algorithm>
#include<optional>

enum class colorType{
    WHITE,
    BLAKE,
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
    TAKE
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
 *      빈 칸으로는 이동할 수 없다.
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

struct moveChunk {
    moveType mT;
    Coord direction;

    std::optional<std::vector<actCoord>> activateSquare;

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
    int moveCount = 0;

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
};

struct Square{
    Piece curr_piece;
    Coord coordinate;
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

//그리고 엔진 본체
class AugmentChessGameState {
private:
    std::vector<Square> board;

    std::vector<Card> whitePlayerCards;
    std::vector<Card> blackPlayerCards;

    colorType turn;

public:
    bool isValidSquare(Coord pos) const;

    bool isOccupied(Coord pos) const;

    bool isEmpty(Coord pos) const {
        return isValidSquare(pos) && !isOccupied(pos);
    }

    Piece* getPieceAt(Coord pos);

    const Piece* getPieceAt(Coord pos) const;

    /**
    * curr_piece가 가진 기본 moveChunk와 additional_mC를 해석하여
    * 현재 GameState에서 실제로 실행 가능한 moveAction 목록을 생성한다.
    *
    * 처리 순서:
    *
    * 1. curr_piece.pT에 해당하는 기본 행마를 가져온다.
    * 2. curr_piece.additional_mC를 기본 행마에 추가한다.
    * 3. 각 moveChunk의 activateSquare 조건을 검사한다.
    * 4. direction과 maxDistance를 이용해 후보 destination들을 생성한다.
    * 5. moveType에 따라 빈칸/적/아군 여부를 판정한다.
    * 6. PieceVolume이 있는 경우 전체 footprint에 대해 충돌 및 보드 범위를 검사한다.
    * 7. next가 존재하면 현재 성공 위치를 기준으로 후속 moveChunk를 재귀적으로 해석한다.
    * 8. 최종적으로 실행 가능한 결과를 moveAction으로 변환하여 반환한다.
    *
    * moveChunk는 정적/런타임 행마 정의이고,
    * moveAction은 특정 GameState에서 실제로 실행 가능한 구체적인 행동이다.
    */
    std::vector<moveAction> interpretedPieceMoveChunk(const Square curr_piece); // 현제 기물이 있는 위치와 기물의 moveChunk를 받고 그에 맞는 moveAction을 계산하여 반환하는 함수.

    // 기물 이동
    void apply_action(const moveAction& action);

    // 카드 사용
    void apply_action(const cardAction& action);
};

//api 테스트용 메인함수.
int main(){
    return 0;
}

//g++ engine.cpp -o engine_test