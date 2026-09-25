//! Draft Rust port of `pre_cpp_engine_code/engine.cpp`.
//!
//! This is a 1:1 translation of the current C++ sketch, kept intentionally as
//! small as the source it was ported from. It does **not** add anything the
//! C++ sketch doesn't already have — no king-safety check, no SHIFT/JUMP, no
//! chained `next` interpretation, no card effects, no `apply_action` body.
//! Anywhere the C++ has a `// TODO` or an empty switch case, this file has
//! the same gap, marked the same way (`todo!()` where C++ left a declaration
//! with no definition, a silent no-op where C++ just `break`s out of the
//! ray walk).
//!
//! Deliberately **no `Cargo.toml` yet** (see `rust-engine/README.md`): once
//! one exists, `.github/workflows/differential.yml` immediately treats
//! `rust-engine` as a real candidate and runs
//! `./rust-engine/target/release/oracle_bridge` against the oracle
//! fixtures. There is no `oracle_bridge` binary yet and this file is nowhere
//! near full board/card parity with the JS oracle, so wiring Cargo.toml in
//! now would turn `differential.yml` red on every push to `develop` — not
//! because of a bug, but because a real candidate hasn't been built. Add
//! `Cargo.toml` together with an `oracle_bridge` bin that speaks the
//! stdin/stdout protocol documented in
//! `infra/tools/fixtures/run-differential.js`'s header comment, once this
//! module can answer it.
//!
//! One deliberate deviation from the C++: `MoveChunk::then` takes and
//! returns an owned `MoveChunk` instead of a reference into a `Vec`. The
//! C++ version's `moveChunk& then(...)` returns `next.back()`, and a later
//! `push_back` on the same vector can reallocate and dangle that reference.
//! Rust's ownership rules make the equivalent pattern awkward to write
//! unsafely by accident, so the builder-style signature below sidesteps the
//! issue rather than porting it.

// ============================================================
// Enums (colorType, pieceType, CardType, CardActType, moveType)
// ============================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    White,
    Black,
    Neutral,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardType {
    Piece,
    Rule,
    Opening,
    Middle,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardActType {
    Passive,
    Active,
    /// opening 카드처럼 의사와 상관없이 강제로 적용되는 카드.
    ActiveForced,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PieceType {
    King,
    Queen,
    Knight,
    Bishop,
    Rook,
    Pawn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveType {
    /// 적-아군 기물 상관없이 모두 잡는 행마
    BothTakeMove,
    TakeMove,
    Catch,
    Move,
    /// 기물끼리 위치를 교환하는 행마. 미구현 (cpp도 TODO).
    Shift,
    Take,
    /// 적 기물을 처음 만난 뒤부터 활성화되는 행마. 미구현 (cpp도 TODO).
    Jump,
}

// ============================================================
// Coord / actCoord
// ============================================================

pub type Coord = (i32, i32);

/// 둘 중 하나가 `None`이면 "임의의"로 해석한다. 예를 들어 `(None, Some(2))`는
/// 임의의 파일 좌표를 가진 2랭크 칸, 즉 2랭크를 활성화 칸으로 하라는 의미다.
pub type ActCoord = (Option<i32>, Option<i32>);

// ============================================================
// moveChunk
// ============================================================

/// 하나의 "행마 규칙 조각". 자세한 해석 규칙은 `pre_cpp_engine_code/engine.cpp`의
/// 원본 주석(direction/mT/maxDistance/activateSquare/next)을 참고. 이 구조체
/// 자체는 "행마 정의"일 뿐이며, 실제 착수로의 해석은
/// `GameState::interpret_piece_move_chunk`가 담당한다.
#[derive(Debug, Clone)]
pub struct MoveChunk {
    pub move_type: MoveType,
    pub direction: Coord,

    /// 각 원소는 논리합(OR)으로 계산한다.
    pub activate_square: Option<Vec<ActCoord>>,

    /// 이 chunk가 성공적으로 적용된 뒤 이어서 해석할 후속 chunk들.
    /// 구조는 포팅했지만 `interpret_piece_move_chunk`는 아직 여기까지
    /// 내려가서 해석하지 않는다 (cpp 원본과 동일한 상태).
    pub next: Vec<MoveChunk>,

    /// `None` = 거리 제한 없음.
    pub max_distance: Option<i32>,
}

impl MoveChunk {
    pub fn new(move_type: MoveType, direction: Coord) -> Self {
        Self {
            move_type,
            direction,
            activate_square: None,
            next: Vec::new(),
            max_distance: Some(1),
        }
    }

    pub fn with_distance(mut self, max_distance: Option<i32>) -> Self {
        self.max_distance = max_distance;
        self
    }

    pub fn with_activation(mut self, squares: Vec<ActCoord>) -> Self {
        self.activate_square = Some(squares);
        self
    }

    /// 후속 chunk를 추가하고 `self`를 돌려준다 (builder 스타일).
    /// cpp의 참조-반환 버전과 달리 댕글링 참조 위험이 없다 — 위 모듈 문서 참고.
    pub fn then(mut self, next_chunk: MoveChunk) -> Self {
        self.next.push(next_chunk);
        self
    }
}

// ============================================================
// PieceVolume / PieceTypeAt
// ============================================================

/// 기물이 초기 위치(anchor)에서 얼마나 떨어진 칸까지 점유하는지를 정의한다.
/// 표준 기물이면 비워 둔다. 자세한 예시는 cpp 원본 주석 참고.
#[derive(Debug, Clone, Default)]
pub struct PieceVolume {
    pub footprint: Vec<Coord>,
}

#[derive(Debug, Clone, Copy)]
pub struct PieceTypeAt {
    pub piece_type: PieceType,
    pub position: Coord,
}

// ============================================================
// 기본 기물 행마 정의
// ============================================================

pub fn base_king_moves() -> Vec<MoveChunk> {
    [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (-1, 1), (1, -1), (-1, -1)]
        .into_iter()
        .map(|d| MoveChunk::new(MoveType::TakeMove, d))
        .collect()
}

pub fn base_queen_moves() -> Vec<MoveChunk> {
    [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (-1, 1), (1, -1), (-1, -1)]
        .into_iter()
        .map(|d| MoveChunk::new(MoveType::TakeMove, d).with_distance(None))
        .collect()
}

pub fn base_rook_moves() -> Vec<MoveChunk> {
    [(1, 0), (-1, 0), (0, 1), (0, -1)]
        .into_iter()
        .map(|d| MoveChunk::new(MoveType::TakeMove, d).with_distance(None))
        .collect()
}

pub fn base_bishop_moves() -> Vec<MoveChunk> {
    [(1, 1), (-1, 1), (1, -1), (-1, -1)]
        .into_iter()
        .map(|d| MoveChunk::new(MoveType::TakeMove, d).with_distance(None))
        .collect()
}

pub fn base_knight_moves() -> Vec<MoveChunk> {
    [
        (1, 2), (2, 1), (-1, 2), (-2, 1),
        (1, -2), (2, -1), (-1, -2), (-2, -1),
    ]
    .into_iter()
    .map(|d| MoveChunk::new(MoveType::TakeMove, d))
    .collect()
}

/// 앙파상 등은 GameState 상태가 필요하므로 여기 포함하지 않는다 (cpp와 동일).
pub fn base_white_pawn_moves() -> Vec<MoveChunk> {
    vec![
        MoveChunk::new(MoveType::Move, (0, 1)),
        MoveChunk::new(MoveType::Move, (0, 1))
            .with_distance(Some(2))
            .with_activation(vec![(None, Some(2))]),
        MoveChunk::new(MoveType::Take, (1, 1)),
        MoveChunk::new(MoveType::Take, (-1, 1)),
    ]
}

pub fn base_black_pawn_moves() -> Vec<MoveChunk> {
    vec![
        MoveChunk::new(MoveType::Move, (0, -1)),
        MoveChunk::new(MoveType::Move, (0, -1))
            .with_distance(Some(2))
            .with_activation(vec![(None, Some(7))]),
        MoveChunk::new(MoveType::Take, (1, -1)),
        MoveChunk::new(MoveType::Take, (-1, -1)),
    ]
}

pub fn get_base_movement(piece_type: PieceType, color: Color) -> Vec<MoveChunk> {
    match piece_type {
        PieceType::King => base_king_moves(),
        PieceType::Queen => base_queen_moves(),
        PieceType::Rook => base_rook_moves(),
        PieceType::Bishop => base_bishop_moves(),
        PieceType::Knight => base_knight_moves(),
        PieceType::Pawn => {
            if color == Color::White {
                base_white_pawn_moves()
            } else {
                base_black_pawn_moves()
            }
        }
    }
}

// ============================================================
// Card
// ============================================================

#[derive(Debug, Clone)]
pub struct Card {
    pub act_type: CardActType,
    pub card_type: CardType,
    pub card_id: String,
    /// `act_type`이 `Passive`면 항상 `true`.
    pub is_used: bool,
}

// ============================================================
// 제약 (PieceConstraint / GameConstraint)
// ============================================================

#[derive(Debug, Clone, Copy)]
pub struct MustCapture {
    pub remaining_triggers: i32,
}

#[derive(Debug, Clone, Copy)]
pub struct NoCapture {
    pub remaining_triggers: i32,
}

#[derive(Debug, Clone, Copy)]
pub struct RepositionMove {
    pub remaining_triggers: i32,
}

#[derive(Debug, Clone, Copy)]
pub struct ForcedPiece {
    pub piece: PieceTypeAt,
    pub remaining_triggers: i32,
}

/// 기물 자체에 붙는 제약. cpp의 `std::variant<MustCapture, NoCapture, RepositionMove>`에 대응.
#[derive(Debug, Clone, Copy)]
pub enum PieceConstraint {
    MustCapture(MustCapture),
    NoCapture(NoCapture),
    RepositionMove(RepositionMove),
}

/// 게임의 현재 행동 흐름에 붙는 제약. cpp의 `std::variant<ForcedPiece>`에 대응.
/// 정의만 포팅했고, cpp와 마찬가지로 move generation 어디에서도 아직 참조하지 않는다.
#[derive(Debug, Clone, Copy)]
pub enum GameConstraint {
    ForcedPiece(ForcedPiece),
}

// ============================================================
// Piece
// ============================================================

#[derive(Debug, Clone)]
pub struct Piece {
    pub color: Color,
    pub piece_type: PieceType,

    /// 프로모션 불가면 빈 벡터.
    pub promotion_pool: Vec<PieceType>,
    /// 카드/룰/버프 등으로 런타임에 추가되는 행마.
    pub additional_moves: Vec<MoveChunk>,

    pub volume: PieceVolume,

    pub hp: i32,
    pub is_king: bool,
    pub move_count: i32,

    pub constraints: Vec<PieceConstraint>,
}

impl Piece {
    pub fn new(color: Color, piece_type: PieceType, volume: PieceVolume) -> Self {
        Self {
            color,
            piece_type,
            promotion_pool: Vec::new(),
            additional_moves: Vec::new(),
            volume,
            hp: 1,
            is_king: false,
            move_count: 0,
            constraints: Vec::new(),
        }
    }

    pub fn add_new_movement(&mut self, new_chunk: MoveChunk) {
        self.additional_moves.push(new_chunk);
    }

    pub fn remove_last_movement(&mut self) {
        self.additional_moves.pop();
    }

    pub fn remove_all_movement(&mut self) {
        self.additional_moves.clear();
    }

    pub fn add_constraint(&mut self, constraint: PieceConstraint) {
        self.constraints.push(constraint);
    }

    pub fn remove_last_constraint(&mut self) {
        self.constraints.pop();
    }

    pub fn remove_all_constraint(&mut self) {
        self.constraints.clear();
    }

    pub fn has_must_capture(&self) -> bool {
        self.constraints
            .iter()
            .any(|c| matches!(c, PieceConstraint::MustCapture(_)))
    }

    pub fn has_no_capture(&self) -> bool {
        self.constraints
            .iter()
            .any(|c| matches!(c, PieceConstraint::NoCapture(_)))
    }
}

// ============================================================
// Square / TurnState / moveAction / cardAction
// ============================================================

#[derive(Debug, Clone)]
pub struct Square {
    pub piece: Piece,
    pub coordinate: Coord,
}

impl Square {
    pub fn piece_type_at(&self) -> PieceTypeAt {
        PieceTypeAt {
            piece_type: self.piece.piece_type,
            position: self.coordinate,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TurnState {
    pub player: Color,
    pub actions_remaining: i32,
}

/// `is_turn_used == false`면 엔진이 apply한 뒤에도 턴을 유지한다.
#[derive(Debug, Clone, Copy)]
pub struct MoveAction {
    pub color: Color,
    pub piece_type: PieceType,
    pub move_type: MoveType,
    pub start: Coord,
    pub destination: Coord,
    pub is_turn_used: bool,
}

impl MoveAction {
    pub fn new(color: Color, piece_type: PieceType, move_type: MoveType, start: Coord, destination: Coord) -> Self {
        Self {
            color,
            piece_type,
            move_type,
            start,
            destination,
            is_turn_used: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CardAction {
    pub color: Color,
    pub card: Card,
    pub is_turn_used: bool,
}

// ============================================================================
// 엔진 본체 (AugmentChessGameState)
// ============================================================================

#[derive(Debug, Clone)]
pub struct GameState {
    board: Vec<Square>,

    rule_card: Vec<Card>,
    white_player_cards: Vec<Card>,
    black_player_cards: Vec<Card>,

    constraints: Vec<GameConstraint>,

    turn: TurnState,
}

impl GameState {
    pub fn new(board: Vec<Square>, turn: TurnState) -> Self {
        Self {
            board,
            rule_card: Vec::new(),
            white_player_cards: Vec::new(),
            black_player_cards: Vec::new(),
            constraints: Vec::new(),
            turn,
        }
    }

    pub fn is_valid_square(&self, pos: Coord) -> bool {
        (1..=8).contains(&pos.0) && (1..=8).contains(&pos.1)
    }

    pub fn is_occupied(&self, pos: Coord) -> bool {
        self.is_valid_square(pos) && self.get_occupying_square(pos).is_some()
    }

    pub fn is_empty(&self, pos: Coord) -> bool {
        self.is_valid_square(pos) && !self.is_occupied(pos)
    }

    /// anchor뿐 아니라 footprint까지 포함하여 해당 칸을 점유한 기물의 Square를 반환한다.
    ///
    /// cpp 원본과 동일하게 보드 전체를 선형 탐색한다 — 탐색 트리(자기대국/MCTS)를
    /// 돌릴 단계에서는 좌표->기물 맵으로 바꾸는 게 좋다는 점을 이미 스케치 단계에서
    /// 확인했다 (`docs/refactor-candidates.md` 성격의 메모로 남겨둘 것).
    pub fn get_occupying_square(&self, pos: Coord) -> Option<&Square> {
        self.board.iter().find(|sq| {
            sq.coordinate == pos
                || sq
                    .piece
                    .volume
                    .footprint
                    .iter()
                    .any(|offset| (sq.coordinate.0 + offset.0, sq.coordinate.1 + offset.1) == pos)
        })
    }

    pub fn get_piece_at(&self, pos: Coord) -> Option<&Piece> {
        self.get_occupying_square(pos).map(|sq| &sq.piece)
    }

    /// 기물을 `new_anchor`에 놓았을 때 몸 전체(footprint 포함)가 보드 안에 있는지 확인한다.
    pub fn is_piece_inside_board(&self, square: &Square, new_anchor: Coord) -> bool {
        if !self.is_valid_square(new_anchor) {
            return false;
        }

        square.piece.volume.footprint.iter().all(|offset| {
            self.is_valid_square((new_anchor.0 + offset.0, new_anchor.1 + offset.1))
        })
    }

    /// 기물을 `new_anchor`에 놓았을 때 footprint가 충돌하는 다른 기물들을 반환한다.
    /// 같은 기물의 여러 footprint 칸과 겹쳐도 한 번만 들어간다.
    pub fn get_placement_collisions(&self, moving_square: &Square, new_anchor: Coord) -> Vec<&Square> {
        let mut collisions: Vec<&Square> = Vec::new();

        let mut check_cell = |pos: Coord, collisions: &mut Vec<&Square>| {
            let Some(target) = self.get_occupying_square(pos) else {
                return;
            };

            // 이동 전 자기 자신의 몸과 겹치는 부분은 충돌로 보지 않는다.
            if target.coordinate == moving_square.coordinate {
                return;
            }

            if !collisions.iter().any(|s| s.coordinate == target.coordinate) {
                collisions.push(target);
            }
        };

        check_cell(new_anchor, &mut collisions);

        for offset in &moving_square.piece.volume.footprint {
            check_cell((new_anchor.0 + offset.0, new_anchor.1 + offset.1), &mut collisions);
        }

        collisions
    }

    /// 기본 포획 가능 여부. 특수 카드/상태/중립 규칙은 아직 없다 (cpp와 동일).
    pub fn can_capture(&self, attacker: &Piece, target: &Piece, move_type: MoveType) -> bool {
        match move_type {
            MoveType::BothTakeMove => true,
            MoveType::Take | MoveType::TakeMove | MoveType::Catch => attacker.color != target.color,
            _ => false,
        }
    }

    /// `curr_square`의 기물이 가진 기본 moveChunk와 additional_moves를 해석하여
    /// 현재 GameState에서 실제로 실행 가능한 MoveAction 목록을 생성한다.
    ///
    /// 아직 미구현 (cpp 원본과 동일한 범위):
    /// - `MoveChunk.next` 연쇄 행마
    /// - `MoveType::Shift`
    /// - `MoveType::Jump`
    pub fn interpret_piece_move_chunk(&self, curr_square: &Square) -> Vec<MoveAction> {
        let piece = &curr_square.piece;
        let origin = curr_square.coordinate;

        let mut movements = get_base_movement(piece.piece_type, piece.color);
        movements.extend(piece.additional_moves.iter().cloned());

        let mut result = Vec::new();

        let no_capture = piece.has_no_capture();
        let must_capture = piece.has_must_capture();

        let is_activated = |chunk: &MoveChunk| -> bool {
            let Some(conditions) = &chunk.activate_square else {
                return true;
            };

            conditions.iter().any(|(x, y)| {
                let x_matches = x.map_or(true, |v| v == origin.0);
                let y_matches = y.map_or(true, |v| v == origin.1);
                x_matches && y_matches
            })
        };

        let mut push_action = |result: &mut Vec<MoveAction>, move_type: MoveType, destination: Coord| {
            result.push(MoveAction::new(piece.color, piece.piece_type, move_type, origin, destination));
        };

        for chunk in &movements {
            if !is_activated(chunk) {
                continue;
            }

            // 무한 ray에서 {0, 0} direction이면 무한루프가 되므로 방어.
            if chunk.direction == (0, 0) && chunk.max_distance.is_none() {
                continue;
            }

            let mut distance = 1;

            loop {
                if let Some(max) = chunk.max_distance {
                    if distance > max {
                        break;
                    }
                }

                let destination = (
                    origin.0 + chunk.direction.0 * distance,
                    origin.1 + chunk.direction.1 * distance,
                );

                let mut stop_ray = false;

                match chunk.move_type {
                    // =====================================================
                    // Move: 빈 곳으로만 이동한다.
                    // =====================================================
                    MoveType::Move => {
                        if !self.is_piece_inside_board(curr_square, destination) {
                            stop_ray = true;
                        } else if !self.get_placement_collisions(curr_square, destination).is_empty() {
                            stop_ray = true;
                        } else if !must_capture {
                            push_action(&mut result, MoveType::Move, destination);
                        }
                    }

                    // =====================================================
                    // Take: 포획 가능한 기물이 있을 때만 그 자리로 이동한다.
                    // 빈칸은 후보가 아니지만 ray 탐색은 계속하지 않는다(한 번만 검사하고 멈춤).
                    // =====================================================
                    MoveType::Take => {
                        if !self.is_piece_inside_board(curr_square, destination) {
                            stop_ray = true;
                        } else {
                            let collisions = self.get_placement_collisions(curr_square, destination);
                            if collisions.len() > 1 {
                                stop_ray = true;
                            } else if let Some(target_square) = collisions.first() {
                                if !no_capture && self.can_capture(piece, &target_square.piece, MoveType::Take) {
                                    push_action(&mut result, MoveType::Take, destination);
                                }
                                stop_ray = true;
                            }
                            // collisions.is_empty() -> 빈칸, ray는 계속 진행 (stop_ray 그대로 false)
                        }
                    }

                    // =====================================================
                    // Catch: 목적지의 기물을 제거하지만 자신은 이동하지 않는다.
                    // =====================================================
                    MoveType::Catch => {
                        if !self.is_valid_square(destination) {
                            stop_ray = true;
                        } else if let Some(target_square) = self.get_occupying_square(destination) {
                            if !no_capture && self.can_capture(piece, &target_square.piece, MoveType::Catch) {
                                push_action(&mut result, MoveType::Catch, destination);
                            }
                            stop_ray = true;
                        }
                        // 빈칸이면 ray 계속 진행 (cpp와 동일)
                    }

                    // =====================================================
                    // TakeMove: 빈칸 -> 이동, 포획 가능한 기물 -> 포획 후 이동.
                    // =====================================================
                    MoveType::TakeMove => {
                        if !self.is_piece_inside_board(curr_square, destination) {
                            stop_ray = true;
                        } else {
                            let collisions = self.get_placement_collisions(curr_square, destination);
                            if collisions.is_empty() {
                                if !must_capture {
                                    push_action(&mut result, MoveType::TakeMove, destination);
                                }
                            } else if collisions.len() > 1 {
                                stop_ray = true;
                            } else {
                                let target_square = collisions[0];
                                if !no_capture && self.can_capture(piece, &target_square.piece, MoveType::TakeMove) {
                                    push_action(&mut result, MoveType::TakeMove, destination);
                                }
                                stop_ray = true;
                            }
                        }
                    }

                    // =====================================================
                    // BothTakeMove: 빈칸 -> 이동, 기물 존재 -> 색과 관계없이 포획 후 이동.
                    // =====================================================
                    MoveType::BothTakeMove => {
                        if !self.is_piece_inside_board(curr_square, destination) {
                            stop_ray = true;
                        } else {
                            let collisions = self.get_placement_collisions(curr_square, destination);
                            if collisions.is_empty() {
                                if !must_capture {
                                    push_action(&mut result, MoveType::BothTakeMove, destination);
                                }
                            } else if collisions.len() > 1 {
                                stop_ray = true;
                            } else {
                                let target_square = collisions[0];
                                if !no_capture && self.can_capture(piece, &target_square.piece, MoveType::BothTakeMove) {
                                    push_action(&mut result, MoveType::BothTakeMove, destination);
                                }
                                stop_ray = true;
                            }
                        }
                    }

                    // =====================================================
                    // Shift: 양쪽 footprint의 교환 가능성 검사가 필요 — 미구현.
                    // =====================================================
                    MoveType::Shift => {
                        stop_ray = true;
                    }

                    // =====================================================
                    // Jump: ray 안에 별도 상태값이 필요 — 미구현.
                    // =====================================================
                    MoveType::Jump => {
                        stop_ray = true;
                    }
                }

                if stop_ray {
                    break;
                }

                distance += 1;
            }
        }

        result
    }

    /// 기물 이동 적용. **아직 본문 없음** — cpp 원본도 클래스에 선언만 있고
    /// 정의가 없는 상태라 그대로 옮겼다 (링크 에러 대신 런타임 panic).
    pub fn apply_move_action(&mut self, _action: &MoveAction) {
        todo!("apply_action(moveAction) — pre_cpp_engine_code/engine.cpp에도 본문 없음")
    }

    /// 카드 사용 적용. 위와 동일하게 미구현.
    pub fn apply_card_action(&mut self, _action: &CardAction) {
        todo!("apply_action(cardAction) — pre_cpp_engine_code/engine.cpp에도 본문 없음")
    }

    pub fn end_turn(&mut self) {
        if self.turn.actions_remaining > 0 {
            return;
        }

        self.turn.player = match self.turn.player {
            Color::White => Color::Black,
            Color::Black => Color::White,
            Color::Neutral => {
                eprintln!("턴 필드에 이상한 값이 들어감");
                return;
            }
        };

        // 임시 기본값. 나중에는 getActionsPerTurn(turn.player) 등으로 교체.
        self.turn.actions_remaining = 1;
    }
}

// ============================================================================
// 최소 스모크 테스트 — Cargo.toml이 생기면 `cargo test`로 바로 돌아가도록 작성.
// engine.cpp의 `main()`(빈 함수, "api 테스트용")보다 조금 더 나아간 버전.
// ============================================================================
#[cfg(test)]
mod tests {
    use super::*;

    fn empty_square(color: Color, piece_type: PieceType, coordinate: Coord) -> Square {
        Square {
            piece: Piece::new(color, piece_type, PieceVolume::default()),
            coordinate,
        }
    }

    #[test]
    fn rook_on_empty_board_sees_full_ray() {
        let square = empty_square(Color::White, PieceType::Rook, (4, 4));
        let state = GameState::new(
            vec![square.clone()],
            TurnState { player: Color::White, actions_remaining: 1 },
        );

        let actions = state.interpret_piece_move_chunk(&square);
        // 4방향 * (보드 경계까지 거리) = 3+4+3+3 = 13칸
        assert_eq!(actions.len(), 13);
    }

    #[test]
    fn pawn_first_move_can_advance_two() {
        let square = empty_square(Color::White, PieceType::Pawn, (5, 2));
        let state = GameState::new(
            vec![square.clone()],
            TurnState { player: Color::White, actions_remaining: 1 },
        );

        let actions = state.interpret_piece_move_chunk(&square);
        let forward: Vec<_> = actions
            .iter()
            .filter(|a| a.move_type == MoveType::Move)
            .map(|a| a.destination)
            .collect();

        assert!(forward.contains(&(5, 3)));
        assert!(forward.contains(&(5, 4)));
    }

    #[test]
    fn king_move_blocked_by_own_piece() {
        let king = empty_square(Color::White, PieceType::King, (4, 4));
        let ally = empty_square(Color::White, PieceType::Pawn, (5, 4));
        let state = GameState::new(
            vec![king.clone(), ally],
            TurnState { player: Color::White, actions_remaining: 1 },
        );

        let actions = state.interpret_piece_move_chunk(&king);
        assert!(!actions.iter().any(|a| a.destination == (5, 4)));
    }
}
