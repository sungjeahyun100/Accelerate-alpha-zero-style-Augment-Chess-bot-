//! Source-pinned v7 effects that settle at a completed turn boundary.
//!
//! The frozen client calls `resolveOthelloAll` when the Othello card is played
//! and once more after `turnsTaken[movingColor]` increases and the source's
//! empty-lunchbox, platform, crown and mistake-card callbacks run, if
//! `othelloPending[movingColor]` is set. The client clears that latch before
//! the second scan. Neither scan consumes RNG or creates a capture record.

use crate::{
    Color, EngineError, GameState, Piece, RULES_VERSION_V6, RULES_VERSION_V7, Result, Square,
};
use serde_json::{Value, json};
use std::collections::{BTreeSet, VecDeque};

const KING_DIRECTIONS: [(isize, isize); 8] = [
    (-1, -1),
    (-1, 0),
    (-1, 1),
    (0, -1),
    (0, 1),
    (1, -1),
    (1, 0),
    (1, 1),
];

fn v7_only(state: &GameState) -> Result<()> {
    match state.ruleset_id.as_str() {
        RULES_VERSION_V7 => Ok(()),
        other => Err(EngineError::UnsupportedFeature(format!(
            "v7 Othello on rules version {other}"
        ))),
    }
}

fn cell(state: &GameState, row: usize, col: usize) -> Option<&Piece> {
    state.board.get(row)?.get(col)?.as_ref()
}

fn ally_at_offset(
    state: &GameState,
    row: usize,
    col: usize,
    dr: isize,
    dc: isize,
    actor: Color,
) -> bool {
    row.checked_add_signed(dr)
        .zip(col.checked_add_signed(dc))
        .and_then(|(r, c)| cell(state, r, c))
        .is_some_and(|piece| piece.color == actor)
}

/// The source asks whether one enemy piece is flanked by friendly pieces on
/// opposite king-neighbor squares. A large piece is never an Othello target.
fn is_othello_target(state: &GameState, actor: Color, row: usize, col: usize) -> bool {
    let Some(piece) = cell(state, row, col) else {
        return false;
    };
    if piece.color != actor.opponent()
        || matches!(piece.kind.as_str(), "wall" | "football" | "blackHole")
        || piece.is_large()
        || (piece.kind == "shotgunKing"
            && state
                .extra
                .get("campaign")
                .and_then(|value| value.get("setup"))
                .and_then(Value::as_str)
                == Some("shotgunKing"))
    {
        return false;
    }
    KING_DIRECTIONS.iter().any(|&(dr, dc)| {
        ally_at_offset(state, row, col, dr, dc, actor)
            && ally_at_offset(state, row, col, -dr, -dc, actor)
    })
}

/// Collect every target before converting any piece. This preserves the
/// client's row-major identity de-duplication and prevents an earlier flip
/// from making a new target eligible in the same pass.
fn targets(state: &GameState, actor: Color) -> Result<Vec<(Square, String)>> {
    let mut seen = BTreeSet::new();
    let mut selected = Vec::new();
    for (row, cells) in state.board.iter().enumerate() {
        for (col, cell) in cells.iter().enumerate() {
            let Some(piece) = cell.as_ref() else { continue };
            if seen.contains(&piece.id) || !is_othello_target(state, actor, row, col) {
                continue;
            }
            let row = u8::try_from(row).map_err(|_| {
                EngineError::UnsupportedFeature("Othello row exceeds wire square".into())
            })?;
            let col = u8::try_from(col).map_err(|_| {
                EngineError::UnsupportedFeature("Othello column exceeds wire square".into())
            })?;
            seen.insert(piece.id.clone());
            selected.push((Square { row, col }, piece.id.clone()));
        }
    }
    Ok(selected)
}

fn resolve_all_on_owned_state(state: &mut GameState, actor: Color) -> Result<usize> {
    let selected = targets(state, actor)?;
    for (square, id) in &selected {
        let previous = state
            .at(*square)
            .cloned()
            .filter(|piece| piece.id == *id)
            .ok_or_else(|| {
                EngineError::InvalidState("Othello target changed during resolution".into())
            })?;
        let mut converted = previous.clone();
        converted.color = actor.into();
        converted.moved = true;
        crate::card_effects::mark_transformed_origin_with_options(
            state,
            &mut converted,
            *square,
            true,
        )?;
        converted.extra.shift_remove("freshNoCaptureUntil");
        converted
            .extra
            .insert("coolGuyCapturedLast".into(), json!(false));
        state.board[square.row as usize][square.col as usize] = Some(converted.clone());
        crate::card_effects::mark_animation(state, &converted)?;
        // The source invokes the royal-loss callback with the pre-flip piece,
        // even though the board now contains its converted identity.
        crate::transition::resolve_royal_capture(state, &previous, actor)?;
    }
    Ok(selected.len())
}

fn pending_object(state: &mut GameState) -> Result<&mut serde_json::Map<String, Value>> {
    if state.extra.get("othelloPending").is_none_or(Value::is_null) {
        state.extra.insert(
            "othelloPending".into(),
            json!({"white":false,"black":false}),
        );
    }
    state
        .extra
        .get_mut("othelloPending")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| EngineError::InvalidState("othelloPending must be a color map".into()))
}

/// Apply the v7 Othello card's immediate branch. The card handler owns its
/// selection and cost policy; this function owns the latch and source flip.
pub(crate) fn activate_othello(state: &mut GameState, actor: Color) -> Result<usize> {
    v7_only(state)?;
    if state.mode == "gameover" {
        return Err(EngineError::Terminal);
    }
    if state.mode != "play" || state.turn != actor {
        return Err(EngineError::WrongActor);
    }
    let mut next = state.clone();
    pending_object(&mut next)?.insert(actor.as_str().into(), json!(true));
    let converted = resolve_all_on_owned_state(&mut next, actor)?;
    *state = next;
    Ok(converted)
}

/// The source order is `turnsTaken[actor]++`, empty-lunchbox cleanup, platform
/// tick, crown adjudication, `mistakeCard[actor]=false`, Othello recheck, then
/// pending gales. A missing or false latch is a no-op. Until the preceding
/// effects are ported, active cases that could change this scan fail closed.
/// v6 uses its existing transition path unchanged.
pub(crate) fn settle_after_completed_turn(state: &mut GameState, actor: Color) -> Result<usize> {
    if state.ruleset_id == RULES_VERSION_V6 || state.mode == "gameover" {
        return Ok(0);
    }
    v7_only(state)?;
    let pending = match state.extra.get("othelloPending") {
        None | Some(Value::Null) => false,
        Some(Value::Object(sides)) => match sides.get(actor.as_str()) {
            None | Some(Value::Null | Value::Bool(false)) => false,
            Some(Value::Bool(true)) => true,
            _ => {
                return Err(EngineError::InvalidState(
                    "othelloPending owner latch must be boolean".into(),
                ));
            }
        },
        _ => {
            return Err(EngineError::InvalidState(
                "othelloPending must be a color map".into(),
            ));
        }
    };
    if !pending {
        return Ok(0);
    }
    let mut next = state.clone();
    pending_object(&mut next)?.insert(actor.as_str().into(), json!(false));
    let converted = resolve_all_on_owned_state(&mut next, actor)?;
    *state = next;
    Ok(converted)
}

// Frozen main4315/4411/4552: Don routes use the source knight-neighbor
// order, royal-safe fast feasibility, and its exact shortest-route tie-breaks.
const DON_KNIGHT_OFFSETS: [(isize, isize); 8] = [
    (-2, -1),
    (-2, 1),
    (-1, -2),
    (-1, 2),
    (1, -2),
    (1, 2),
    (2, -1),
    (2, 1),
];

fn don_unsupported(reason: &str) -> EngineError {
    EngineError::UnsupportedFeature(format!("v7 Don Quixote turn entry: {reason}"))
}

fn don_entries(state: &GameState) -> Vec<(Square, Piece)> {
    // expansionBoardEntries groups aliased cells by source item id and keeps
    // the first row-major cell as its origin (main647).
    let mut seen = BTreeSet::new();
    let mut entries = Vec::new();
    for (row, cells) in state.board.iter().enumerate() {
        for (col, piece) in cells.iter().enumerate() {
            let Some(piece) = piece else {
                continue;
            };
            if !piece.id.is_empty() && !seen.insert(piece.id.clone()) {
                continue;
            }
            entries.push((
                Square {
                    row: row as u8,
                    col: col as u8,
                },
                piece.clone(),
            ));
        }
    }
    entries
}

fn don_find(state: &GameState, id: &str, color: Color) -> Option<(Square, Piece)> {
    don_entries(state)
        .into_iter()
        .find(|(_, piece)| piece.id == id && piece.color == color)
}

fn don_windmills(state: &GameState) -> Vec<Square> {
    don_entries(state)
        .into_iter()
        .filter(|(_, piece)| piece.kind == "windmill")
        .map(|(at, _)| at)
        .collect()
}

fn don_cell(index: usize) -> Square {
    Square {
        row: (index / 8) as u8,
        col: (index % 8) as u8,
    }
}

fn don_index(cell: Square) -> usize {
    usize::from(cell.row) * 8 + usize::from(cell.col)
}

struct DonGraph {
    available: [bool; 64],
    neighbors: [Vec<usize>; 64],
}

fn don_graph(state: &GameState, from: Square, avoid_royal: bool) -> DonGraph {
    let start = don_index(from);
    let available = std::array::from_fn(|index| {
        index == start
            || !avoid_royal
            || state.at(don_cell(index)).is_none_or(|piece| {
                piece.kind != "vip" && !crate::v7_threat::is_royal_identity_v7(state, piece)
            })
    });
    let neighbors = std::array::from_fn(|index| {
        if !available[index] {
            return Vec::new();
        }
        let cell = don_cell(index);
        DON_KNIGHT_OFFSETS
            .into_iter()
            .filter_map(|(dr, dc)| {
                let row = usize::from(cell.row).checked_add_signed(dr)?;
                let col = usize::from(cell.col).checked_add_signed(dc)?;
                if row >= 8 || col >= 8 || !available[row * 8 + col] {
                    return None;
                }
                Some(row * 8 + col)
            })
            .collect()
    });
    DonGraph {
        available,
        neighbors,
    }
}

/// Fast planner: find the first remaining target in source BFS order, remove
/// every target encountered along that segment, and repeat. Cache statistics
/// are outside source Positions and do not affect the selected path.
fn don_fast_route(graph: &DonGraph, start: usize, destinations: &[usize]) -> Option<Vec<usize>> {
    let mut remaining = destinations.iter().copied().collect::<BTreeSet<_>>();
    remaining.remove(&start);
    let mut path = Vec::new();
    let mut position = start;
    while !remaining.is_empty() {
        let mut parent = [None; 64];
        let mut queue = VecDeque::from([position]);
        parent[position] = Some(position);
        let mut found = None;
        while let Some(current) = queue.pop_front() {
            for &next in &graph.neighbors[current] {
                if parent[next].is_some() {
                    continue;
                }
                parent[next] = Some(current);
                queue.push_back(next);
                if remaining.contains(&next) {
                    found = Some(next);
                    break;
                }
            }
            if found.is_some() {
                break;
            }
        }
        let found = found?;
        let mut segment = Vec::new();
        let mut cursor = found;
        while cursor != position {
            segment.push(cursor);
            cursor = parent[cursor]?;
        }
        segment.reverse();
        for cell in segment {
            path.push(cell);
            remaining.remove(&cell);
        }
        position = found;
    }
    Some(path)
}

struct DonRouteSearch<'a> {
    graph: &'a DonGraph,
    destinations: Vec<usize>,
    bit_at: [u64; 64],
    complete: u64,
    distances: [[i16; 64]; 64],
    previous: [[Option<usize>; 64]; 64],
    tree_cache: std::collections::BTreeMap<u64, usize>,
}

impl<'a> DonRouteSearch<'a> {
    fn new(graph: &'a DonGraph, destinations: &[usize]) -> Self {
        let mut unique = Vec::new();
        for &target in destinations {
            if !unique.contains(&target) {
                unique.push(target);
            }
        }
        let mut bit_at = [0; 64];
        for (offset, &target) in unique.iter().enumerate() {
            bit_at[target] = 1_u64 << offset;
        }
        let complete = if unique.len() == 64 {
            u64::MAX
        } else {
            (1_u64 << unique.len()) - 1
        };
        let mut distances = [[-1; 64]; 64];
        let mut previous = [[None; 64]; 64];
        for source in 0..64 {
            let mut queue = VecDeque::from([source]);
            distances[source][source] = 0;
            while let Some(current) = queue.pop_front() {
                for &next in &graph.neighbors[current] {
                    if distances[source][next] >= 0 {
                        continue;
                    }
                    distances[source][next] = distances[source][current] + 1;
                    previous[source][next] = Some(current);
                    queue.push_back(next);
                }
            }
        }
        Self {
            graph,
            destinations: unique,
            bit_at,
            complete,
            distances,
            previous,
            tree_cache: std::collections::BTreeMap::new(),
        }
    }

    fn unseen(&self, mask: u64) -> Vec<usize> {
        self.destinations
            .iter()
            .copied()
            .filter(|&target| mask & self.bit_at[target] == 0)
            .collect()
    }

    fn between(&self, source: usize, target: usize) -> Option<Vec<usize>> {
        let mut path = Vec::new();
        let mut current = target;
        while current != source {
            path.push(current);
            current = self.previous[source][current]?;
        }
        path.reverse();
        Some(path)
    }

    fn lower_bound(&mut self, position: usize, mask: u64) -> usize {
        let unseen = self.unseen(mask);
        if unseen.is_empty() {
            return 0;
        }
        let tree = if let Some(&tree) = self.tree_cache.get(&mask) {
            tree
        } else {
            let mut cost = 0;
            let mut pending = unseen
                .iter()
                .skip(1)
                .map(|&target| (target, self.distances[unseen[0]][target] as usize))
                .collect::<Vec<_>>();
            while !pending.is_empty() {
                let mut chosen = 0;
                for index in 1..pending.len() {
                    if pending[index].1 < pending[chosen].1 {
                        chosen = index;
                    }
                }
                let (target, distance) = pending.remove(chosen);
                cost += distance;
                for (next, distance) in &mut pending {
                    *distance = (*distance).min(self.distances[target][*next] as usize);
                }
            }
            self.tree_cache.insert(mask, cost);
            cost
        };
        tree + unseen
            .iter()
            .map(|&target| self.distances[position][target] as usize)
            .min()
            .unwrap_or(0)
    }

    /// Source >=20-target IDA branch. Recursion depth is bounded by the
    /// strictly decreasing budget below the finite greedy route length.
    fn visit_ida(
        &mut self,
        position: usize,
        mask: u64,
        budget: usize,
        path: &mut Vec<usize>,
        seen: &mut std::collections::BTreeMap<(u64, usize), usize>,
    ) -> bool {
        if mask == self.complete {
            return true;
        }
        if budget == 0 {
            return false;
        }
        let unseen = self.unseen(mask);
        let parity = |index: usize| (index / 8 + index % 8) % 2;
        let same = unseen
            .iter()
            .filter(|&&target| parity(target) == parity(position))
            .count();
        if (2 * same).max((2 * (unseen.len() - same)).saturating_sub(1)) > budget
            || self.lower_bound(position, mask) > budget
            || seen
                .get(&(mask, position))
                .is_some_and(|&previous| previous >= budget)
        {
            return false;
        }
        if seen.len() >= 20_000 {
            seen.clear();
        }
        if self.tree_cache.len() >= 20_000 {
            self.tree_cache.clear();
        }
        seen.insert((mask, position), budget);
        let unvisited = |index: usize| self.bit_at[index] & !mask != 0;
        let mut choices = self.graph.neighbors[position].clone();
        choices.sort_by_key(|&index| {
            (
                !unvisited(index),
                self.graph.neighbors[index]
                    .iter()
                    .filter(|&&neighbor| unvisited(neighbor))
                    .count(),
                index,
            )
        });
        for next in choices {
            path.push(next);
            if self.visit_ida(next, mask | self.bit_at[next], budget - 1, path, seen) {
                return true;
            }
            path.pop();
        }
        false
    }
}

#[derive(Clone, Copy)]
struct DonFrontierEntry {
    position: usize,
    mask: u64,
    g: usize,
    parent: Option<usize>,
}

fn don_shortest_route(
    graph: &DonGraph,
    start: usize,
    destinations: &[usize],
) -> Option<Vec<usize>> {
    if destinations.iter().any(|&target| !graph.available[target]) {
        return None;
    }
    let mut search = DonRouteSearch::new(graph, destinations);
    let initial_mask = search.bit_at[start];
    if initial_mask == search.complete {
        return Some(Vec::new());
    }
    if search
        .destinations
        .iter()
        .any(|&target| search.distances[start][target] < 0)
    {
        return None;
    }
    let mut incumbent = Vec::new();
    let mut position = start;
    let mut mask = initial_mask;
    while mask != search.complete {
        let mut unseen = search.unseen(mask);
        unseen.sort_by_key(|&target| (search.distances[position][target], target));
        let target = unseen[0];
        for step in search.between(position, target)? {
            incumbent.push(step);
            mask |= search.bit_at[step];
        }
        position = target;
    }
    let initial_h = search.lower_bound(start, initial_mask);
    if initial_h >= incumbent.len() {
        return Some(incumbent);
    }
    if search.destinations.len() >= 20 {
        let mut path = Vec::new();
        for limit in initial_h..incumbent.len() {
            let mut seen = std::collections::BTreeMap::new();
            if search.visit_ida(start, initial_mask, limit, &mut path, &mut seen) {
                return Some(path);
            }
        }
        return Some(incumbent);
    }
    let mut entries = vec![DonFrontierEntry {
        position: start,
        mask: initial_mask,
        g: 0,
        parent: None,
    }];
    let mut frontier = std::collections::BinaryHeap::new();
    frontier.push(std::cmp::Reverse((initial_h, initial_h, 0_u64, 0_usize)));
    let mut best = (0..64)
        .map(|_| std::collections::BTreeMap::<u64, usize>::new())
        .collect::<Vec<_>>();
    best[start].insert(initial_mask, 0);
    let mut order = 1_u64;
    while let Some(std::cmp::Reverse((f, _, _, index))) = frontier.pop() {
        let current = entries[index];
        if best[current.position].get(&current.mask) != Some(&current.g) {
            continue;
        }
        if f >= incumbent.len() {
            break;
        }
        if current.mask == search.complete {
            let mut path = Vec::new();
            let mut cursor = index;
            while let Some(parent) = entries[cursor].parent {
                path.push(entries[cursor].position);
                cursor = parent;
            }
            path.reverse();
            return Some(path);
        }
        for next in graph.neighbors[current.position].iter().copied() {
            let mask = current.mask | search.bit_at[next];
            let g = current.g + 1;
            if best[next].get(&mask).is_some_and(|&previous| previous <= g) {
                continue;
            }
            let h = search.lower_bound(next, mask);
            if g + h >= incumbent.len() {
                continue;
            }
            best[next].insert(mask, g);
            let next_index = entries.len();
            entries.push(DonFrontierEntry {
                position: next,
                mask,
                g,
                parent: Some(index),
            });
            frontier.push(std::cmp::Reverse((g + h, h, order, next_index)));
            order += 1;
        }
    }
    Some(incumbent)
}

fn don_uses_fast_route(state: &GameState) -> bool {
    let source = state
        .extra
        .get("cardState")
        .filter(|value| crate::observation::truth(Some(value)));
    let profile = if let Some(source) = source {
        source.get("profile")
    } else {
        state.extra.get("profile")
    };
    let hash = profile.and_then(|profile| profile.get("catalogHash"));
    if crate::observation::truth(hash) {
        hash.and_then(Value::as_str) == Some(crate::v7_execution_profile::SOURCE_CATALOG_HASH)
    } else {
        !crate::observation::truth(state.extra.get("campaign"))
            || state.extra.get("september27CopyPools") == Some(&json!(true))
    }
}

fn don_plan_route(
    state: &GameState,
    from: Square,
    targets: &[Square],
    don_count: usize,
) -> Option<Vec<Square>> {
    let destinations = targets.iter().copied().map(don_index).collect::<Vec<_>>();
    let start = don_index(from);
    let fast = don_uses_fast_route(state)
        && (targets.len() >= 13
            || don_count >= 2 && targets.len() >= 7
            || don_count >= 4 && targets.len() >= 2);
    let safe_graph = don_graph(state, from, true);
    let safe = don_fast_route(&safe_graph, start, &destinations);
    let route = if let Some(safe) = safe {
        if fast {
            Some(safe)
        } else {
            don_shortest_route(&safe_graph, start, &destinations)
        }
    } else {
        let unrestricted = don_graph(state, from, false);
        if fast {
            don_fast_route(&unrestricted, start, &destinations)
        } else {
            don_shortest_route(&unrestricted, start, &destinations)
        }
    };
    route.map(|route| route.into_iter().map(don_cell).collect())
}

#[cfg(test)]
fn don_safe_route(state: &GameState, from: Square, target: Square) -> Result<Vec<Square>> {
    don_plan_route(state, from, &[target], 1).ok_or_else(|| don_unsupported("route unavailable"))
}

fn don_hidden_from(state: &GameState, piece: &Piece, at: Square) -> Value {
    if let Some(hidden) = piece
        .extra
        .get("hiddenFrom")
        .filter(|value| crate::observation::truth(Some(value)))
    {
        return hidden.clone();
    }
    crate::observation::piece_hidden_from_v7(state, piece, at)
        .map(|color| json!(color.as_str()))
        .unwrap_or_else(|| json!(""))
}

fn don_sync_reference(state: &mut GameState, piece: &Piece) {
    crate::transition::update_piece(state, piece);
    for owner in [Color::White, Color::Black] {
        for captured in state.captures.get_mut(owner) {
            if captured.id == piece.id {
                *captured = piece.clone();
            }
        }
    }
}

fn don_rampage_step(
    state: &mut GameState,
    actor: Color,
    from: Square,
    mut moving: Piece,
    to: Square,
) -> Result<Value> {
    let victim = state.at(to).cloned();
    let captured = victim
        .as_ref()
        .map(|piece| json!({"id":piece.id,"type":piece.kind,"color":piece.color}))
        .unwrap_or(Value::Null);
    let captured_from = victim
        .as_ref()
        .and_then(|piece| {
            don_entries(state)
                .into_iter()
                .find(|(_, other)| other.id == piece.id)
                .map(|(at, _)| at)
        })
        .unwrap_or(to);
    let hidden = don_hidden_from(state, &moving, from);
    let captured_hidden = victim
        .as_ref()
        .map(|piece| don_hidden_from(state, piece, to))
        .unwrap_or_else(|| json!(""));
    // The original rampage options do not supply a pre-move privacy snapshot.
    crate::replay::queue_don_quixote_move_notation(
        state,
        &moving,
        from,
        to,
        &Value::Null,
        victim.is_some(),
        true,
    )?;
    if let Some(victim) = victim {
        crate::v7_capture_reactions::record_new_card_capture_reactions(
            state, &victim, actor, false,
        )?;
        if let Some((_, live)) = don_entries(state)
            .into_iter()
            .find(|(_, piece)| piece.id == moving.id)
        {
            moving = live;
        }
        let threat = json!({"label":"돈 키호테"});
        let options = crate::transition::ForceRemovalOptions {
            attacker: Some(&moving),
            count_as_capture: true,
            threat_source: Some(&threat),
            ..Default::default()
        };
        crate::transition::force_remove_piece_at_with_options(state, to, actor, &options)?;
        // source.item remains a live reference, including after Reaper has
        // removed it. Retain any reaction mutation before assigning counters.
        if let Some((_, live)) = don_entries(state)
            .into_iter()
            .find(|(_, piece)| piece.id == moving.id)
        {
            moving = live;
        } else {
            for owner in [Color::Black, Color::White] {
                if let Some(captured) = state
                    .captures
                    .get(owner)
                    .iter()
                    .rev()
                    .find(|piece| piece.id == moving.id)
                {
                    moving = captured.clone();
                    break;
                }
            }
        }
        let previous = crate::card_effects::js_number(moving.extra.get("totalCaptures"), 0)
            .unwrap_or(0.0)
            .max(0.0);
        let total = previous + 1.0;
        // Source JSON emits integral Number values as integers; preserve the
        // typed counter accessor while retaining actual fractional counts.
        moving.extra.insert(
            "totalCaptures".into(),
            if total.fract() == 0.0 && total >= i64::MIN as f64 && total < i64::MAX as f64 {
                json!(total as i64)
            } else {
                json!(total)
            },
        );
        crate::v7_move_piece_effects::resolve_witch_trial_capture(&mut moving, true)?;
        don_sync_reference(state, &moving);
    }
    if state.at(from).is_some_and(|piece| piece.id == moving.id) {
        state.board[usize::from(from.row)][usize::from(from.col)] = None;
        moving.moved = true;
        state.board[usize::from(to.row)][usize::from(to.col)] = Some(moving.clone());
        don_sync_reference(state, &moving);
    }
    state.en_passant = None;
    crate::card_effects::remember_local_movement(state, &moving)?;
    Ok(
        json!({"pieceId":moving.id,"color":actor,"from":from,"to":to,"captured":captured,
        "capturedFrom":captured_from,"hiddenFrom":hidden,"capturedHiddenFrom":captured_hidden,"rampage":true}),
    )
}

/// Frozen resolveLocalDonQuixote/resolveDonQuixoteTurn, main115279/4552.
/// Windmills bypass isEnabled; absent windmills use the exact public legal
/// move and Don free-move contracts. All callback mutations commit atomically.
pub(crate) fn resolve_don_quixote_turn_entry(
    state: &mut GameState,
    incoming: Color,
) -> Result<usize> {
    if state.ruleset_id == RULES_VERSION_V6 || state.mode == "gameover" {
        return Ok(0);
    }
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(don_unsupported("requires the frozen v7 ruleset"));
    }
    if state.turn != incoming {
        return Err(EngineError::WrongActor);
    }
    if state.board.len() != 8 || state.board.iter().any(|row| row.len() != 8) {
        return Err(don_unsupported("requires the adopted 8x8 board"));
    }
    let mut next = state.clone();
    let ids = don_entries(&next)
        .into_iter()
        .filter(|(_, piece)| {
            piece.color == incoming && matches!(piece.kind.as_str(), "donQuixote" | "don-quixote")
        })
        .map(|(_, piece)| piece.id)
        .collect::<Vec<_>>();
    let mut transitions = Vec::new();
    for id in &ids {
        if next.mode == "gameover" {
            break;
        }
        let Some((from, moving)) = don_find(&next, id, incoming) else {
            continue;
        };
        if don_windmills(&next).is_empty() {
            if next.threat_probe_depth > 0
                || crate::movement::frozen(&moving)
                || crate::card_effects::js_number(moving.extra.get("poisonStunTurns"), 0)
                    .unwrap_or(0.0)
                    .floor()
                    > 0.0
                || crate::observation::truth(moving.extra.get("grapplerBound"))
            {
                continue;
            }
            let legal =
                crate::movement::v7_legal_move_targets(&next, &moving, from, Default::default())?;
            let mut choices = Vec::new();
            for (dr, dc) in DON_KNIGHT_OFFSETS {
                let Some(row) = usize::from(from.row).checked_add_signed(dr) else {
                    continue;
                };
                let Some(col) = usize::from(from.col).checked_add_signed(dc) else {
                    continue;
                };
                if row >= 8 || col >= 8 {
                    continue;
                }
                let at = Square {
                    row: row as u8,
                    col: col as u8,
                };
                if let Some(victim) = next.at(at) {
                    if victim.color == moving.color
                        || !crate::movement::v7_can_capture_target_without_attacker(
                            &next,
                            incoming,
                            victim,
                            "donQuixote",
                        )?
                    {
                        continue;
                    }
                }
                if legal.iter().any(|target| target.square() == at) {
                    choices.push(at);
                }
            }
            if choices.is_empty() {
                continue;
            }
            let index = (next.rng.sample()? * choices.len() as f64).floor() as usize;
            next.rng.record_last_probability(
                1.0 / choices.len() as f64,
                "source Don Quixote destination",
            )?;
            let to = *choices
                .get(index)
                .ok_or_else(|| EngineError::InvalidState("v7 Don choice out of bounds".into()))?;
            // canEnter and step both call getLegalMoves at their source point.
            let legal =
                crate::movement::v7_legal_move_targets(&next, &moving, from, Default::default())?;
            let Some(target) = legal.into_iter().find(|target| target.square() == to) else {
                continue;
            };
            crate::transition::apply_v7_don_quixote_free_move(&mut next, incoming, from, target)?;
            let actual = don_entries(&next)
                .into_iter()
                .find(|(_, piece)| piece.id == *id)
                .map(|(at, _)| at)
                .unwrap_or(to);
            transitions.push(
                json!({"pieceId":id,"color":incoming,"from":from,"to":actual,"rampage":false}),
            );
            continue;
        }
        let mut path = Vec::new();
        let mut path_index = 0;
        let mut expected = Vec::new();
        while next.mode != "gameover" {
            let Some((from, moving)) = don_find(&next, id, incoming) else {
                break;
            };
            let targets = don_windmills(&next);
            if targets.is_empty() {
                break;
            }
            if path_index >= path.len() || targets != expected {
                let Some(route) = don_plan_route(&next, from, &targets, ids.len()) else {
                    break;
                };
                if route.is_empty() {
                    break;
                }
                path = route;
                path_index = 0;
            }
            let to = path[path_index];
            path_index += 1;
            expected = targets.into_iter().filter(|target| *target != to).collect();
            transitions.push(don_rampage_step(&mut next, incoming, from, moving, to)?);
        }
    }
    let count = transitions.len();
    if next.ai_simulation_depth == 0 && transitions.iter().any(|step| step["rampage"] == true) {
        crate::replay::queue_visual(
            &mut next,
            json!({"type":"don-quixote-route","color":incoming,"transitions":transitions}),
        )?;
    }
    *state = next;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GameConfig, PieceColor, RngState};

    fn bare_state() -> GameState {
        let mut state = GameState::new(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            5,
        )
        .unwrap();
        state.ruleset_id = RULES_VERSION_V7.into();
        state.board = vec![vec![None; 8]; 8];
        state
    }

    fn place(state: &mut GameState, color: Color, kind: &str, row: usize, col: usize, id: &str) {
        state.board[row][col] = Some(Piece::new(kind, color, id));
    }

    fn source_don_probe_turn_entry() -> GameState {
        let mut state = bare_state();
        state.turn = Color::White;
        state.mode = "play".into();
        state.move_count = 1;
        state.full_move = 2;
        state.turns_taken.black = 1;
        state.actions_remaining = 1;
        state.rng = RngState {
            algorithm: "lcg32-v1".into(),
            state: 3_596_032_795,
            tape: Vec::new(),
            cursor: 222,
            source_chance_trace: None,
        };
        state.board[0][0] = Some(Piece::new(
            "windmill",
            PieceColor::Neutral,
            "neutral-windmill-ahe4rqzocq6",
        ));
        place(
            &mut state,
            Color::Black,
            "king",
            2,
            3,
            "black-king-pmiwvlwhlr9",
        );
        place(
            &mut state,
            Color::Black,
            "pawn",
            2,
            7,
            "black-pawn-jdnrcu69kmj",
        );
        state.board[2][7].as_mut().unwrap().moved = true;
        place(
            &mut state,
            Color::White,
            "donQuixote",
            4,
            4,
            "white-donQuixote-yli72yexcko",
        );
        place(
            &mut state,
            Color::White,
            "king",
            7,
            4,
            "white-king-wvt63tbibo",
        );
        for field in ["capturedTypes", "turnCaptures"] {
            state.extra.insert(
                field.into(),
                json!({"white":{"__simType":"Set","values":[]},"black":{"__simType":"Set","values":[]}}),
            );
        }
        state.extra.insert("mediumMovement".into(), Value::Null);
        state.extra.insert(
            "parrotMovement".into(),
            json!({"white":null,"black":{"type":"pawn"}}),
        );
        state.extra.insert("deathmatch".into(), Value::Null);
        state.extra.insert("campaign".into(), Value::Null);
        state
            .extra
            .insert("prophecy".into(), json!({"white":null,"black":null}));
        state
            .extra
            .insert("initiative".into(), json!({"white":null,"black":null}));
        state
    }

    #[test]
    fn source_v7_don_probe_avoids_royal_and_preserves_four_step_rng_and_capture() {
        let mut state = source_don_probe_turn_entry();
        let route =
            don_safe_route(&state, Square { row: 4, col: 4 }, Square { row: 0, col: 0 }).unwrap();
        assert_eq!(
            route,
            [
                Square { row: 2, col: 5 },
                Square { row: 0, col: 4 },
                Square { row: 1, col: 2 },
                Square { row: 0, col: 0 },
            ]
        );
        assert_eq!(
            resolve_don_quixote_turn_entry(&mut state, Color::White).unwrap(),
            4
        );
        assert_eq!(state.board[2][3].as_ref().unwrap().kind, "king");
        assert_eq!(state.board[0][0].as_ref().unwrap().kind, "donQuixote");
        assert!(state.board[0][0].as_ref().unwrap().moved);
        assert_eq!(
            state.board[0][0].as_ref().unwrap().number("totalCaptures"),
            1
        );
        assert!(state.board[4][4].is_none());
        assert_eq!(state.captures.white.len(), 1);
        assert_eq!(state.captures.white[0].kind, "windmill");
        assert_eq!(
            state.extra["capturedTypes"]["white"]["values"],
            json!(["windmill"])
        );
        assert_eq!(
            state.extra["turnCaptures"]["white"]["values"],
            json!(["windmill"])
        );
        assert_eq!(state.extra["mediumMovement"], json!({"type":"windmill"}));
        assert_eq!(
            state.extra["parrotMovement"]["white"],
            json!({"type":"don-quixote"})
        );
        assert_eq!(state.rng.cursor, 226);
        assert_eq!(state.rng.state, 1_854_409_343);
        let notations = state.extra["pendingNotations"].as_array().unwrap();
        assert_eq!(notations.len(), 4);
        assert_eq!(
            notations
                .iter()
                .map(|event| event["text"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["DQe4-f6", "DQf6-e8", "DQe8-c7", "DQc7xa8"]
        );
        assert_eq!(
            notations
                .iter()
                .map(|event| event["id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            [
                "special-1790581292828-h21pcen",
                "special-1790581292828-foaamup",
                "special-1790581292828-5sv39in",
                "special-1790581292828-fjkckyb",
            ]
        );
        assert_eq!(
            state.extra["pendingReplayVisuals"][0]["transitions"]
                .as_array()
                .unwrap()
                .len(),
            4
        );
    }

    #[test]
    fn multiple_windmills_use_general_source_route_and_capture_each_target() {
        let mut state = source_don_probe_turn_entry();
        state.board[3][3] = Some(Piece::new("windmill", PieceColor::Neutral, "second"));
        state.rng.state = 17;
        state.rng.cursor = 0;
        let count = resolve_don_quixote_turn_entry(&mut state, Color::White).unwrap();
        assert!(count > 0);
        assert!(don_windmills(&state).is_empty());
        assert_eq!(
            state
                .captures
                .white
                .iter()
                .filter(|piece| piece.kind == "windmill")
                .count(),
            2
        );
        let (_, don) = don_find(&state, "white-donQuixote-yli72yexcko", Color::White).unwrap();
        assert_eq!(don.number("totalCaptures"), 2);
    }

    #[test]
    fn black_disabled_don_rampages_when_a_windmill_exists() {
        let mut state = source_don_probe_turn_entry();
        state.turn = Color::Black;
        state.board[4][4] = Some(Piece::new("donQuixote", Color::Black, "black-don"));
        let don = state.board[4][4].as_mut().unwrap();
        don.extra.insert("frozen".into(), json!(true));
        don.extra.insert("poisonStunTurns".into(), json!(2));
        don.extra
            .insert("grapplerBound".into(), json!({"id":"binding-piece"}));
        let count = resolve_don_quixote_turn_entry(&mut state, Color::Black).unwrap();
        assert!(count > 0);
        assert!(don_windmills(&state).is_empty());
        assert_eq!(state.captures.black.len(), 1);
        let (_, don) = don_find(&state, "black-don", Color::Black).unwrap();
        assert!(don.moved);
        assert!(don.flag("frozen"));
        assert_eq!(don.number("totalCaptures"), 1);
    }

    #[test]
    fn no_windmill_disabled_or_probe_don_is_inert_and_failure_is_atomic() {
        let baseline = source_don_probe_turn_entry();
        for probe in [false, true] {
            let mut state = baseline.clone();
            state.board[0][0] = None;
            if probe {
                state.threat_probe_depth = 1;
            } else {
                state.board[4][4]
                    .as_mut()
                    .unwrap()
                    .extra
                    .insert("frozen".into(), json!(true));
            }
            let before = state.clone();
            assert_eq!(
                resolve_don_quixote_turn_entry(&mut state, Color::White).unwrap(),
                0
            );
            assert_eq!(state, before);
        }
        let mut state = baseline;
        state.extra.insert("pendingNotations".into(), json!(42));
        let before = state.clone();
        assert!(matches!(
            resolve_don_quixote_turn_entry(&mut state, Color::White),
            Err(EngineError::InvalidState(_))
        ));
        assert_eq!(state, before);
    }

    fn don_receipt_differences(
        actual: &Value,
        expected: &Value,
        path: &str,
        output: &mut Vec<String>,
    ) {
        if output.len() >= 12
            || serde_jcs::to_vec(actual).unwrap() == serde_jcs::to_vec(expected).unwrap()
        {
            return;
        }
        match (actual, expected) {
            (Value::Object(actual), Value::Object(expected)) => {
                for key in actual
                    .keys()
                    .chain(expected.keys())
                    .collect::<BTreeSet<_>>()
                {
                    let next = format!("{path}.{key}");
                    match (actual.get(key), expected.get(key)) {
                        (Some(actual), Some(expected)) => {
                            don_receipt_differences(actual, expected, &next, output)
                        }
                        (Some(_), None) => output.push(format!("{next}: unexpected field")),
                        (None, Some(_)) => output.push(format!("{next}: missing field")),
                        _ => {}
                    }
                    if output.len() >= 12 {
                        break;
                    }
                }
            }
            (Value::Array(actual), Value::Array(expected)) => {
                if actual.len() != expected.len() {
                    output.push(format!(
                        "{path}.length: {} != {}",
                        actual.len(),
                        expected.len()
                    ));
                }
                for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
                    don_receipt_differences(actual, expected, &format!("{path}[{index}]"), output);
                    if output.len() >= 12 {
                        break;
                    }
                }
            }
            _ => output.push(format!("{path}: {actual} != {expected}")),
        }
    }

    /// Root-generated raw callbacks compare complete source Positions before
    /// terminal microtasks settle; RNG, replay, captures and aliases are included.
    #[test]
    #[ignore = "requires root-generated ACCELERATE_V7_DON_CASES receipt"]
    fn frozen_don_callbacks_match_full_positions() {
        let path = std::env::var_os("ACCELERATE_V7_DON_CASES")
            .expect("main agent must provide the source-pinned Don receipts");
        let source = std::fs::read_to_string(path).expect("Don receipts must be readable");
        let lines = source
            .lines()
            .filter(|line| !line.trim().is_empty())
            .collect::<Vec<_>>();
        let expected_names = [
            "don-single-windmill-white",
            "don-multiple-windmills-white",
            "don-black-alias-windmill",
            "don-multiple-pieces-after-rampage",
            "don-no-windmill-white",
            "don-no-windmill-black",
            "don-no-windmill-frozen",
            "don-no-windmill-fractional-poison",
            "don-rampage-bypasses-disabled-piece",
            "don-concealed-witch-trial-rampage",
            "don-rampage-friendly-route-victim",
            "don-royal-safe-fallback-terminal",
            "don-rampage-threat-simulation",
            "don-no-windmill-threat-probe",
            "don-fast-two-pieces-seven-windmills",
            "don-fast-four-pieces-two-windmills",
        ]
        .into_iter()
        .collect::<BTreeSet<_>>();
        assert_eq!(
            lines.len(),
            expected_names.len(),
            "all 16 frozen Don cases are required"
        );
        let mut seen = BTreeSet::new();
        let mut failures = Vec::new();
        for line in lines {
            let receipt: Value = serde_json::from_str(line).unwrap();
            assert_eq!(receipt["schemaVersion"], json!(1));
            assert_eq!(
                receipt["sourceSha256"],
                "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c"
            );
            assert_eq!(receipt["rulesVersion"], RULES_VERSION_V7);
            assert_eq!(
                receipt["executionProfile"]["profileVersion"],
                "accelerate-headless-semantic-v7-faithful-init-v1"
            );
            assert_eq!(
                receipt["executionProfile"]["selectedInitializerCount"],
                json!(175)
            );
            assert_eq!(
                receipt["executionProfile"]["excludedInitializerCount"],
                json!(168)
            );
            let name = receipt["name"]
                .as_str()
                .expect("receipt must identify its case");
            assert!(expected_names.contains(name), "unexpected Don case {name}");
            assert!(seen.insert(name.to_owned()), "duplicate Don case {name}");
            let host =
                match crate::v7_host::V7HostPosition::from_envelope(receipt["before"].clone()) {
                    Ok(host) => host,
                    Err(error) => {
                        failures.push(format!("{name} admission: {error}"));
                        continue;
                    }
                };
            let mut working = host.state().clone();
            working.ai_simulation_depth = receipt["hostContext"]["aiSimulationDepth"]
                .as_u64()
                .unwrap_or(0) as u32;
            working.threat_probe_depth = receipt["hostContext"]["threatProbeDepth"]
                .as_u64()
                .unwrap_or(0) as u32;
            let actor: Color = serde_json::from_value(receipt["actor"].clone())
                .expect("Don receipt actor must be white or black");
            if let Err(error) = resolve_don_quixote_turn_entry(&mut working, actor) {
                failures.push(format!("{name} callback: {error}"));
                continue;
            }
            let settled = match crate::v7_host::V7HostPosition::from_state(working) {
                Ok(settled) => settled,
                Err(error) => {
                    failures.push(format!("{name} checkpoint export: {error}"));
                    continue;
                }
            };
            let actual = settled.export_envelope().unwrap();
            let mut differences = Vec::new();
            don_receipt_differences(&actual, &receipt["after"], "position", &mut differences);
            if !differences.is_empty() {
                failures.push(format!("{name}: {differences:?}"));
            }
        }
        assert!(
            failures.is_empty(),
            "source Don differences:\n{}",
            failures.join("\n")
        );
    }

    /// Three targets give an MST bound of seven and an eight-step greedy
    /// incumbent. The root-generated source proof requires actual A* expansion.
    #[test]
    #[ignore = "requires root-generated ACCELERATE_V7_DON_PLANNER_CASE receipt"]
    fn frozen_don_shortest_astar_matches_full_position() {
        let path = std::env::var_os("ACCELERATE_V7_DON_PLANNER_CASE")
            .expect("main agent must provide the source-pinned Don A* receipt");
        let source = std::fs::read_to_string(path).expect("Don A* receipt must be readable");
        let lines = source
            .lines()
            .filter(|line| !line.trim().is_empty())
            .collect::<Vec<_>>();
        assert_eq!(lines.len(), 1, "exactly one frozen A* case is required");
        let receipt: Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(receipt["name"], "don-shortest-astar-three-targets");
        assert_eq!(
            receipt["sourceSha256"],
            "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c"
        );
        assert_eq!(receipt["rulesVersion"], RULES_VERSION_V7);
        assert_eq!(
            receipt["executionProfile"]["profileVersion"],
            "accelerate-headless-semantic-v7-faithful-init-v1"
        );
        assert_eq!(
            receipt["executionProfile"]["selectedInitializerCount"],
            json!(175)
        );
        assert_eq!(
            receipt["executionProfile"]["excludedInitializerCount"],
            json!(168)
        );
        assert!(
            receipt["plannerProbe"]["expandedStates"]
                .as_u64()
                .is_some_and(|count| count > 0),
            "the authoritative source case must enter A* rather than only returning its greedy route"
        );
        let host =
            crate::v7_host::V7HostPosition::from_envelope(receipt["before"].clone()).unwrap();
        let mut working = host.state().clone();
        let (from, _) = don_find(&working, "don-astar", Color::White).unwrap();
        let route = don_plan_route(&working, from, &don_windmills(&working), 1).unwrap();
        assert_eq!(
            serde_json::to_value(route).unwrap(),
            receipt["plannerProbe"]["path"]
        );
        resolve_don_quixote_turn_entry(&mut working, Color::White).unwrap();
        let actual = crate::v7_host::V7HostPosition::from_state(working)
            .unwrap()
            .export_envelope()
            .unwrap();
        let mut differences = Vec::new();
        don_receipt_differences(&actual, &receipt["after"], "position", &mut differences);
        assert!(
            differences.is_empty(),
            "source Don A* differences: {differences:?}"
        );
    }

    #[test]
    fn immediate_and_completed_turn_scans_are_distinct_and_latch_is_private() {
        let mut state = bare_state();
        place(&mut state, Color::White, "rook", 4, 2, "left");
        place(&mut state, Color::Black, "pawn", 4, 3, "first");
        place(&mut state, Color::White, "rook", 4, 4, "right");
        assert_eq!(activate_othello(&mut state, Color::White).unwrap(), 1);
        let first = state.board[4][3].as_ref().unwrap();
        assert_eq!(first.color, Color::White);
        assert!(first.moved);
        assert_eq!(first.extra["origin"], "d4");
        assert_eq!(first.extra["coolGuyCapturedLast"], false);
        assert_eq!(state.extra["othelloPending"]["white"], true);
        assert!(
            state.extra["forceAnimatedPieceIds"]["values"]
                .as_array()
                .unwrap()
                .contains(&json!("first"))
        );

        place(&mut state, Color::White, "rook", 2, 2, "later-left");
        place(&mut state, Color::Black, "pawn", 2, 3, "later");
        place(&mut state, Color::White, "rook", 2, 4, "later-right");
        state.turns_taken.white += 1;
        assert_eq!(
            settle_after_completed_turn(&mut state, Color::White).unwrap(),
            1
        );
        assert_eq!(state.board[2][3].as_ref().unwrap().color, Color::White);
        assert_eq!(state.extra["othelloPending"]["white"], false);
        assert_eq!(
            settle_after_completed_turn(&mut state, Color::White).unwrap(),
            0
        );
    }

    #[test]
    fn source_target_filter_rejects_large_pieces_and_shotgun_campaign_king() {
        let mut state = bare_state();
        place(&mut state, Color::White, "rook", 4, 2, "left");
        place(&mut state, Color::Black, "colossus", 4, 3, "large");
        place(&mut state, Color::White, "rook", 4, 4, "right");
        assert!(!is_othello_target(&state, Color::White, 4, 3));
        place(&mut state, Color::Black, "shotgunKing", 4, 3, "shotgun");
        state
            .extra
            .insert("campaign".into(), json!({"setup":"shotgunKing"}));
        assert!(!is_othello_target(&state, Color::White, 4, 3));
    }

    #[test]
    fn regency_royal_flip_succeeds_and_callback_failure_rolls_back_flip_and_latch() {
        let mut baseline = bare_state();
        place(&mut baseline, Color::White, "rook", 4, 2, "left");
        place(&mut baseline, Color::Black, "king", 4, 3, "royal");
        place(&mut baseline, Color::White, "rook", 4, 4, "right");
        baseline
            .extra
            .insert("regency".into(), json!({"white":false,"black":true}));
        // main104987/109245: 전향 전 왕실의 상실을 처리한다. 계승자가 있으면
        // 표식을 부여하고 계속하며, 없으면 승리를 기록한다. 둘 다 지원 경로다.
        for has_heir in [false, true] {
            let mut state = baseline.clone();
            if has_heir {
                place(&mut state, Color::Black, "queen", 1, 1, "heir");
            }
            let rng = state.rng.clone();
            let history = state.history.clone();
            assert_eq!(activate_othello(&mut state, Color::White).unwrap(), 1);
            assert_eq!(state.board[4][3].as_ref().unwrap().color, Color::White);
            assert!(state.board[4][3].as_ref().unwrap().moved);
            assert_eq!(state.extra["othelloPending"]["white"], true);
            assert!(state.flag("kingDead", Color::Black));
            if has_heir {
                assert_eq!(state.mode, "play");
                assert!(state.winner.is_none());
                assert!(state.board[1][1].as_ref().unwrap().flag("regencyHeir"));
            } else {
                assert_eq!(state.mode, "gameover");
                assert_eq!(state.winner.as_deref(), Some("white"));
                assert_eq!(state.extra["replayEndReason"], "흑 킹이 잡혔습니다.");
            }
            assert!(state.captures.white.is_empty() && state.captures.black.is_empty());
            assert_eq!(state.rng, rng);
            assert_eq!(state.history, history);
        }

        // source markPieceForAnimation도 truthy 비-Set의 add 호출은 실패한다.
        // 전향과 latch 기록 후 발생하는 실제 콜백 오류가 원본에 남지 않아야 한다.
        let mut invalid = baseline;
        invalid
            .extra
            .insert("forceAnimatedPieceIds".into(), json!([]));
        let before = invalid.clone();
        let error = activate_othello(&mut invalid, Color::White).unwrap_err();
        assert!(matches!(&error, EngineError::InvalidState(_)));
        assert_eq!(
            error.to_string(),
            "invalid state: forceAnimatedPieceIds must encode a Set"
        );
        assert_eq!(invalid, before);
    }

    #[test]
    fn pending_recheck_uses_the_already_settled_predecessor_state() {
        let mut state = bare_state();
        place(&mut state, Color::White, "rook", 4, 2, "left");
        place(&mut state, Color::Black, "pawn", 4, 3, "victim");
        place(&mut state, Color::White, "rook", 4, 4, "right");
        state
            .extra
            .insert("othelloPending".into(), json!({"white":true,"black":false}));
        for (field, value) in [
            ("platformRule", json!({"enabled":true,"fixed":false})),
            ("crownRule", json!({"holderId":"left"})),
        ] {
            let mut callback = state.clone();
            callback.extra.insert(field.into(), value.clone());
            // Source resolveOthelloAll operates on the predecessor's result;
            // the common turn flow has already settled these other effects.
            assert_eq!(
                settle_after_completed_turn(&mut callback, Color::White).unwrap(),
                1
            );
            assert_eq!(callback.board[4][3].as_ref().unwrap().color, Color::White);
            assert_eq!(callback.extra["othelloPending"]["white"], false);
            assert_eq!(callback.extra[field], value);
        }
        state.board[5][0] = Some(Piece::new("pawn", Color::White, "lunchbox"));
        state.board[5][0]
            .as_mut()
            .unwrap()
            .extra
            .insert("emptyLunchbox".into(), json!({"deadlineTurn":1}));
        assert_eq!(
            settle_after_completed_turn(&mut state, Color::White).unwrap(),
            1
        );
        assert_eq!(state.extra["othelloPending"]["white"], false);
        assert_eq!(
            state.board[5][0].as_ref().unwrap().extra["emptyLunchbox"],
            json!({"deadlineTurn":1})
        );
    }
}
