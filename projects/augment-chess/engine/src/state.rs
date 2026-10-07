use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fmt};

pub type Result<T> = std::result::Result<T, EngineError>;
pub type Fields = Map<String, Value>;

pub(crate) fn validate_json_value(value: &Value, initial_depth: usize) -> Result<()> {
    let mut pending = vec![(value, initial_depth)];
    while let Some((value, depth)) = pending.pop() {
        if depth > 64 {
            return Err(EngineError::InvalidState(
                "JSON nesting exceeds depth 64".into(),
            ));
        }
        match value {
            Value::Number(number) => {
                let number = number.as_f64().ok_or_else(|| {
                    EngineError::InvalidState("number is outside the JSON execution range".into())
                })?;
                if !number.is_finite()
                    || (number.fract() == 0.0 && number.abs() > 9_007_199_254_740_991.0)
                {
                    return Err(EngineError::InvalidState(
                        "JSON numbers must be finite and integers must be JavaScript-safe".into(),
                    ));
                }
            }
            Value::Array(items) => pending.extend(items.iter().map(|item| (item, depth + 1))),
            Value::Object(items) => pending.extend(items.values().map(|item| (item, depth + 1))),
            _ => {}
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineError {
    InvalidState(String),
    InvalidConfig(String),
    Serialization(String),
    UnsupportedFeature(String),
    /// A well-formed public frame is incompatible with this sampled world.
    /// Particle filtering may reject this candidate; malformed inputs and
    /// unsupported rule semantics remain distinct errors.
    ConditioningMismatch(String),
    IllegalAction,
    WrongActor,
    StaleAction,
    Terminal,
}
impl EngineError {
    pub(crate) fn serialization(error: serde_json::Error) -> Self {
        Self::Serialization(error.to_string())
    }
}
impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidState(s) => write!(f, "invalid state: {s}"),
            Self::InvalidConfig(s) => write!(f, "invalid config: {s}"),
            Self::Serialization(s) => write!(f, "serialization: {s}"),
            Self::UnsupportedFeature(s) => write!(f, "unsupported feature: {s}"),
            Self::ConditioningMismatch(s) => write!(f, "conditioning mismatch: {s}"),
            Self::IllegalAction => f.write_str("illegal action"),
            Self::WrongActor => f.write_str("wrong actor"),
            Self::StaleAction => f.write_str("action belongs to another position"),
            Self::Terminal => f.write_str("game is terminal"),
        }
    }
}
impl std::error::Error for EngineError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Color {
    White,
    Black,
}
impl Color {
    pub fn opponent(self) -> Self {
        match self {
            Self::White => Self::Black,
            Self::Black => Self::White,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::White => "white",
            Self::Black => "black",
        }
    }
    pub fn pawn_dir(self) -> i8 {
        match self {
            Self::White => -1,
            Self::Black => 1,
        }
    }
    pub fn home_row(self) -> u8 {
        match self {
            Self::White => 7,
            Self::Black => 0,
        }
    }
    pub fn promotion_row(self) -> u8 {
        self.opponent().home_row()
    }
}

/// Board allegiance is separate from the player making a decision. Neutral
/// obstacles have no deck, turn counter, or player-specific passive effects.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PieceColor {
    White,
    Black,
    Neutral,
}
impl PieceColor {
    pub fn owner(self) -> Option<Color> {
        match self {
            Self::White => Some(Color::White),
            Self::Black => Some(Color::Black),
            Self::Neutral => None,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::White => "white",
            Self::Black => "black",
            Self::Neutral => "neutral",
        }
    }
}
impl From<Color> for PieceColor {
    fn from(color: Color) -> Self {
        match color {
            Color::White => Self::White,
            Color::Black => Self::Black,
        }
    }
}
impl PartialEq<Color> for PieceColor {
    fn eq(&self, color: &Color) -> bool {
        self.owner() == Some(*color)
    }
}
impl PartialEq<PieceColor> for Color {
    fn eq(&self, color: &PieceColor) -> bool {
        color == self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Square {
    pub row: u8,
    pub col: u8,
}
impl Square {
    pub fn new(row: u8, col: u8) -> Result<Self> {
        if row < 8 && col < 8 {
            Ok(Self { row, col })
        } else {
            Err(EngineError::InvalidState("square outside 8x8 board".into()))
        }
    }
    pub fn offset(self, dr: i8, dc: i8) -> Option<Self> {
        let row = i16::from(self.row) + i16::from(dr);
        let col = i16::from(self.col) + i16::from(dc);
        ((0..8).contains(&row) && (0..8).contains(&col)).then_some(Self {
            row: row as u8,
            col: col as u8,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Piece {
    pub kind: String,
    pub color: PieceColor,
    pub moved: bool,
    pub id: String,
    pub extra: Fields,
    pub(crate) source_order: Vec<String>,
}
impl Serialize for Piece {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut fields = self.extra.clone();
        fields.insert("type".into(), json!(self.kind));
        fields.insert("color".into(), json!(self.color));
        // A source piece can exist without a `moved` property (notably the
        // neutral wall made by Barricade). Preserve that shape until the
        // source would create the property by setting it to true. Native
        // pieces with no source ordering still use the historical full shape.
        if self.moved
            || self.source_order.is_empty()
            || self.source_order.iter().any(|field| field == "moved")
        {
            fields.insert("moved".into(), json!(self.moved));
        }
        fields.insert("id".into(), json!(self.id));
        let mut map = serializer.serialize_map(Some(fields.len()))?;
        for key in &self.source_order {
            if let Some(value) = fields.shift_remove(key) {
                map.serialize_entry(key, &value)?;
            }
        }
        for (key, value) in fields {
            map.serialize_entry(&key, &value)?;
        }
        map.end()
    }
}
impl<'de> Deserialize<'de> for Piece {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        let mut fields = Fields::deserialize(deserializer)?;
        let source_order = fields.keys().cloned().collect();
        let kind = fields
            .shift_remove("type")
            .and_then(|v| v.as_str().map(str::to_owned))
            .ok_or_else(|| serde::de::Error::missing_field("type"))?;
        let color = serde_json::from_value(
            fields
                .shift_remove("color")
                .ok_or_else(|| serde::de::Error::missing_field("color"))?,
        )
        .map_err(serde::de::Error::custom)?;
        let moved = match fields.shift_remove("moved") {
            Some(v) => serde_json::from_value(v).map_err(serde::de::Error::custom)?,
            None => false,
        };
        let id = match fields.shift_remove("id") {
            Some(v) => serde_json::from_value(v).map_err(serde::de::Error::custom)?,
            None => String::new(),
        };
        Ok(Self {
            kind,
            color,
            moved,
            id,
            extra: fields,
            source_order,
        })
    }
}
impl Piece {
    /// Source pieceAbilityType uses a validated trickster ability before the
    /// visible physical type. Capture and movement share this exact identity.
    pub(crate) fn ability_kind(&self) -> &str {
        const TRICKSTER: &[&str] = &[
            "queen",
            "rook",
            "bishop",
            "missionary",
            "knight",
            "pawn",
            "protestant",
            "herald",
            "cannon",
            "fanatic",
            "primeMinister",
            "eagle",
            "amazon",
            "cardinal",
            "pegasus",
            "jester",
            "camel",
            "hook",
            "grasshopper",
            "dragon",
            "man",
            "assassin",
            "reaper",
            "knightmaster",
            "standardBearer",
            "guard",
            "recruiter",
            "squire",
            "checker",
            "checkerKing",
            "wizard",
            "alfil",
            "windmill",
            "idol",
            "lobster",
            "bear",
            "siegeRam",
            "magicGirl",
            "berserker",
            "slime",
            "siren",
            "undead",
            "campfire",
            "hedgehog",
            "princess",
            "thief",
            "brutus",
            "clockwork",
            "parrot",
            "paladin",
            "octopus",
            "grappler",
            "revolvingDoor",
            "donQuixote",
            "medium",
        ];
        if self.kind == "trickster"
            && let Some(kind) = self.extra.get("tricksterMoveType").and_then(Value::as_str)
            && TRICKSTER.contains(&kind)
        {
            return kind;
        }
        &self.kind
    }
    pub fn new(
        kind: impl Into<String>,
        color: impl Into<PieceColor>,
        id: impl Into<String>,
    ) -> Self {
        Self {
            kind: kind.into(),
            color: color.into(),
            id: id.into(),
            moved: false,
            extra: Fields::new(),
            source_order: ["color", "type", "moved", "id"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
        }
    }
    pub fn flag(&self, name: &str) -> bool {
        self.extra
            .get(name)
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }
    pub fn number(&self, name: &str) -> i64 {
        self.extra.get(name).and_then(Value::as_i64).unwrap_or(0)
    }
    pub fn is_large(&self) -> bool {
        matches!(self.kind.as_str(), "colossus" | "bigRook" | "bigBishop")
    }
    pub fn is_royal(&self) -> bool {
        self.flag("crownRoyal")
            || self.flag("editorRoyal")
            || matches!(
                self.kind.as_str(),
                "king" | "royalKnight" | "shotgunKing" | "darkWizard" | "merchant"
            )
    }
    pub fn is_defeat_royal(&self) -> bool {
        self.is_royal() || self.kind == "vip"
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MoveTarget {
    pub row: u8,
    pub col: u8,
    #[serde(flatten)]
    pub flags: Fields,
}
impl MoveTarget {
    pub fn at(square: Square) -> Self {
        Self {
            row: square.row,
            col: square.col,
            flags: Fields::new(),
        }
    }
    pub fn square(&self) -> Square {
        Square {
            row: self.row,
            col: self.col,
        }
    }
    pub fn flag(&self, name: &str) -> bool {
        match self.flags.get(name) {
            None | Some(Value::Null) => false,
            Some(Value::Bool(value)) => *value,
            Some(Value::String(value)) => !value.is_empty(),
            Some(Value::Number(value)) => value.as_f64().is_some_and(|number| number != 0.0),
            Some(Value::Array(_) | Value::Object(_)) => true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ActionKind {
    Move,
    Card,
    Promotion,
    PromotionChoice,
    ShotgunReload,
    WizardSpell,
    FileSurgeSkip,
    DraftPick,
    DraftBundlePick,
    TrolleyChoice,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Action {
    #[serde(rename = "type")]
    pub kind: ActionKind,
    pub color: Color,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<Square>,
    #[serde(rename = "move", default, skip_serializing_if = "Option::is_none")]
    pub destination: Option<MoveTarget>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card_instance_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position_key: Option<String>,
    #[serde(flatten)]
    pub extra: Fields,
}
impl Action {
    pub fn movement(color: Color, from: Square, to: MoveTarget) -> Self {
        Self {
            kind: ActionKind::Move,
            color,
            from: Some(from),
            destination: Some(to),
            card_id: None,
            card_instance_id: None,
            target: None,
            position_key: None,
            extra: Fields::new(),
        }
    }
    pub fn card(color: Color, card: &CardSlot, target: Option<Value>) -> Self {
        Self {
            kind: ActionKind::Card,
            color,
            from: None,
            destination: None,
            card_id: Some(card.id.clone()),
            card_instance_id: Some(card.instance_id.clone()),
            target,
            position_key: None,
            extra: Fields::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CardSlot {
    pub id: String,
    pub effect: String,
    pub instance_id: String,
    pub stars: f64,
    pub used: bool,
    pub recovering: bool,
    pub vacant: bool,
    pub extra: Fields,
    pub(crate) source_order: Vec<String>,
}
impl Serialize for CardSlot {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut fields = self.extra.clone();
        fields.insert("id".into(), Value::String(self.id.clone()));
        fields.insert("effect".into(), Value::String(self.effect.clone()));
        if !self.instance_id.is_empty() || self.source_order.iter().any(|k| k == "instanceId") {
            fields.insert("instanceId".into(), Value::String(self.instance_id.clone()));
        }
        if self.stars != 0.0
            || self.source_order.is_empty()
            || self.source_order.iter().any(|k| k == "stars")
        {
            // Source RULE templates use null. Its arithmetic value is zero,
            // while the original null remains part of the source DTO.
            if self.stars != 0.0 || fields.get("stars") != Some(&Value::Null) {
                fields.insert(
                    "stars".into(),
                    serde_json::to_value(self.stars).map_err(serde::ser::Error::custom)?,
                );
            }
        }
        for (name, value) in [("used", self.used), ("recovering", self.recovering)] {
            if value || self.source_order.iter().any(|k| k == name) {
                fields.insert(name.into(), Value::Bool(value));
            }
        }
        let mut map = serializer.serialize_map(Some(fields.len()))?;
        for key in &self.source_order {
            if let Some(value) = fields.shift_remove(key) {
                map.serialize_entry(key, &value)?;
            }
        }
        for (key, value) in fields {
            map.serialize_entry(&key, &value)?;
        }
        map.end()
    }
}
impl<'de> Deserialize<'de> for CardSlot {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        let mut fields = Fields::deserialize(deserializer)?;
        let source_order = fields.keys().cloned().collect();
        let id = fields
            .shift_remove("id")
            .and_then(|v| v.as_str().map(str::to_owned))
            .ok_or_else(|| serde::de::Error::missing_field("id"))?;
        let effect = fields
            .shift_remove("effect")
            .and_then(|v| v.as_str().map(str::to_owned))
            .ok_or_else(|| serde::de::Error::missing_field("effect"))?;
        let instance_id = fields
            .shift_remove("instanceId")
            .map(serde_json::from_value)
            .transpose()
            .map_err(serde::de::Error::custom)?
            .unwrap_or_default();
        let stars = if fields.get("stars") == Some(&Value::Null) {
            0.0
        } else {
            fields
                .shift_remove("stars")
                .map(serde_json::from_value)
                .transpose()
                .map_err(serde::de::Error::custom)?
                .unwrap_or_default()
        };
        let used = fields
            .shift_remove("used")
            .map(serde_json::from_value)
            .transpose()
            .map_err(serde::de::Error::custom)?
            .unwrap_or_default();
        let recovering = fields
            .shift_remove("recovering")
            .map(serde_json::from_value)
            .transpose()
            .map_err(serde::de::Error::custom)?
            .unwrap_or_default();
        Ok(Self {
            id,
            effect,
            instance_id,
            stars,
            used,
            recovering,
            vacant: false,
            extra: fields,
            source_order,
        })
    }
}
impl CardSlot {
    pub fn star_value(&self) -> f64 {
        self.extra
            .get("ratingHalfStars")
            .and_then(Value::as_u64)
            .map(|half| half as f64 / 2.0)
            .unwrap_or(self.stars)
    }
}
fn deserialize_decks<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Sides<Vec<CardSlot>>, D::Error> {
    let raw = Sides::<Vec<Option<CardSlot>>>::deserialize(deserializer)?;
    let convert = |slots: Vec<Option<CardSlot>>| {
        slots
            .into_iter()
            .map(|slot| {
                slot.unwrap_or(CardSlot {
                    id: String::new(),
                    effect: String::new(),
                    instance_id: String::new(),
                    stars: 0.0,
                    used: false,
                    recovering: false,
                    vacant: true,
                    extra: Fields::new(),
                    source_order: Vec::new(),
                })
            })
            .collect()
    };
    Ok(Sides {
        white: convert(raw.white),
        black: convert(raw.black),
        white_first: raw.white_first,
    })
}
fn serialize_decks<S: serde::Serializer>(
    decks: &Sides<Vec<CardSlot>>,
    serializer: S,
) -> std::result::Result<S::Ok, S::Error> {
    fn convert(slots: &[CardSlot]) -> Vec<Option<&CardSlot>> {
        slots
            .iter()
            .map(|slot| (!slot.vacant).then_some(slot))
            .collect()
    }
    Sides {
        white: convert(&decks.white),
        black: convert(&decks.black),
        white_first: decks.white_first,
    }
    .serialize(serializer)
}

#[derive(Clone, Debug, PartialEq)]
pub struct Sides<T> {
    pub white: T,
    pub black: T,
    // Input/source map insertion order affects JSON.stringify replay deltas.
    // It is control metadata, never an extra JSON field.
    pub(crate) white_first: bool,
}
impl<T: Serialize> Serialize for Sides<T> {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(2))?;
        if self.white_first {
            map.serialize_entry("white", &self.white)?;
            map.serialize_entry("black", &self.black)?;
        } else {
            map.serialize_entry("black", &self.black)?;
            map.serialize_entry("white", &self.white)?;
        }
        map.end()
    }
}
impl<'de, T: Deserialize<'de>> Deserialize<'de> for Sides<T> {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        struct Visitor<T>(std::marker::PhantomData<T>);
        impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for Visitor<T> {
            type Value = Sides<T>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a white/black player map")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut white = None;
                let mut black = None;
                let mut first = None;
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "white" => {
                            if white.is_some() {
                                return Err(serde::de::Error::duplicate_field("white"));
                            }
                            first.get_or_insert(true);
                            white = Some(map.next_value()?);
                        }
                        "black" => {
                            if black.is_some() {
                                return Err(serde::de::Error::duplicate_field("black"));
                            }
                            first.get_or_insert(false);
                            black = Some(map.next_value()?);
                        }
                        _ => {
                            map.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(Sides {
                    white: white.ok_or_else(|| serde::de::Error::missing_field("white"))?,
                    black: black.ok_or_else(|| serde::de::Error::missing_field("black"))?,
                    white_first: first.unwrap_or(true),
                })
            }
        }
        deserializer.deserialize_map(Visitor(std::marker::PhantomData))
    }
}
impl<T: Default> Default for Sides<T> {
    fn default() -> Self {
        Self {
            white: T::default(),
            black: T::default(),
            white_first: true,
        }
    }
}
impl<T> Sides<T> {
    pub fn new(white: T, black: T) -> Self {
        Self {
            white,
            black,
            white_first: true,
        }
    }
    pub fn get(&self, color: Color) -> &T {
        match color {
            Color::White => &self.white,
            Color::Black => &self.black,
        }
    }
    pub fn get_mut(&mut self, color: Color) -> &mut T {
        match color {
            Color::White => &mut self.white,
            Color::Black => &mut self.black,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnPassant {
    pub row: u8,
    pub col: u8,
    pub captured_row: u8,
    pub captured_col: u8,
    pub color: Color,
    /// Source activeEnPassantStates also consumes `additional` rights. Preserve
    /// source metadata losslessly at this shared DTO boundary.
    #[serde(flatten, default)]
    pub extra: Fields,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RngState {
    pub algorithm: String,
    pub state: u32,
    #[serde(default)]
    pub tape: Vec<f64>,
    #[serde(default)]
    pub cursor: usize,
    /// 공개 전이 제안의 실행 전용 난수 분류. 복제 시에도 다른 입자와 공유하지 않는다.
    #[serde(skip)]
    pub(crate) source_chance_trace: Option<Box<crate::source_chance_trace::SourceChanceTrace>>,
}
impl RngState {
    pub fn seeded(seed: u64) -> Self {
        Self {
            algorithm: "lcg32-v1".into(),
            state: seed as u32,
            tape: Vec::new(),
            cursor: 0,
            source_chance_trace: None,
        }
    }
    #[track_caller]
    pub fn sample(&mut self) -> Result<f64> {
        if self.algorithm != "lcg32-v1" {
            return Err(EngineError::UnsupportedFeature(format!(
                "RNG {}",
                self.algorithm
            )));
        }
        if self.cursor as u64 >= 9_007_199_254_740_991u64 || self.cursor == usize::MAX {
            return Err(EngineError::InvalidState("RNG cursor overflow".into()));
        }
        if let Some(trace) = self.source_chance_trace.as_mut() {
            trace.record(self.cursor, std::panic::Location::caller())?;
        }
        self.state = self.state.wrapping_mul(1664525).wrapping_add(1013904223);
        let value = self
            .tape
            .get(self.cursor)
            .copied()
            .unwrap_or(f64::from(self.state) / 4294967296.0);
        self.cursor += 1;
        Ok(value)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameState {
    pub board: Vec<Vec<Option<Piece>>>,
    pub turn: Color,
    #[serde(default = "play")]
    pub mode: String,
    #[serde(default)]
    pub winner: Option<String>,
    #[serde(default = "one")]
    pub actions_remaining: u32,
    #[serde(default)]
    pub move_count: u32,
    #[serde(default = "one")]
    pub full_move: u32,
    #[serde(default)]
    pub turns_taken: Sides<u32>,
    #[serde(default)]
    pub cards_used_this_turn: Sides<u32>,
    #[serde(
        default,
        deserialize_with = "deserialize_decks",
        serialize_with = "serialize_decks"
    )]
    pub deck_slots: Sides<Vec<CardSlot>>,
    #[serde(default)]
    pub captures: Sides<Vec<Piece>>,
    #[serde(default)]
    pub en_passant: Option<EnPassant>,
    #[serde(default = "default_ruleset")]
    pub ruleset_id: String,
    #[serde(default = "default_rng")]
    pub rng: RngState,
    #[serde(default)]
    pub history: Vec<Value>,
    #[serde(skip)]
    pub(crate) gameover_replay_pending: bool,
    /// Owned execution-only likelihood. Source identities and hypothetical
    /// availability probes do not enter this semantic outcome trace.
    #[serde(skip)]
    pub(crate) semantic_chance_probability: Option<f64>,
    /// One transaction's source-owned public draft proposal. It is consumed
    /// before observation/event construction and never serialized or imported.
    #[serde(skip)]
    pub(crate) source_offer_condition: Option<crate::draft::SourceOfferCondition>,
    /// Source WeakMap의 보드 행동 원점. 실행 중에만 존재하며 공개/저장 DTO에 넣지 않는다.
    #[serde(skip)]
    pub(crate) board_action_origins: Option<Vec<crate::v7_card_context::BoardActionOrigin>>,
    /// Source activeMoveReplayCapture lives outside the serialized state.
    #[serde(skip)]
    pub(crate) active_move_replay_before: Option<crate::replay::MoveReplayCapture>,
    /// 위협 probe의 임시 state 교체를 넘어 유지되는 source 전역 capture 문맥.
    /// 명시적인 probe scope에서만 공유하며 Position/다른 session에는 저장하지 않는다.
    #[serde(skip)]
    pub(crate) move_replay_scope: Option<crate::replay::ReplayCaptureScope>,
    /// movePiece wrapper가 소유하며 중첩 호출 뒤 복원하는 실행 문맥.
    #[serde(skip)]
    pub(crate) active_v7_move_context: Option<crate::v7_move_execution::V7MoveExecutionContext>,
    /// withSaturationAttack의 실행 시작 시 포획 잠금. 저장 DTO에는 포함하지 않는다.
    #[serde(skip)]
    pub(crate) active_v7_saturation_attack: Option<(String, bool)>,
    #[serde(skip)]
    pub(crate) free_move_resolution: Option<Color>,
    /// 원문 Free Move의 Don Quixote 자동 기보 context. wire 상태에는 없다.
    #[serde(skip)]
    pub(crate) free_move_don_quixote: bool,
    /// 실제 Colossus callback을 예약한 host의 owner. snapshot import는 복원하지 않는다.
    #[serde(skip)]
    pub(crate) pending_colossus_actor: Option<Color>,
    /// 누락한 source 덱과 명시적인 양쪽 빈 덱을 구분한다.
    #[serde(skip)]
    pub(crate) source_deck_slots_absent: bool,
    #[serde(skip)]
    pub(crate) virtual_card_depth: u16,
    /// Availability simulation is separate from the royal-threat probe scope.
    #[serde(skip)]
    pub(crate) ai_simulation_depth: u32,
    #[serde(skip)]
    pub(crate) threat_probe_depth: u32,
    #[serde(flatten)]
    pub extra: Fields,
}
fn one() -> u32 {
    1
}
fn play() -> String {
    "play".into()
}
pub const RULES_VERSION_V6: &str = "augment-site-20260927-abfe01a035813875";
pub const RULES_VERSION_V7: &str = "augment-site-20260928-e5ed84fcf8e72a24";
fn default_ruleset() -> String {
    RULES_VERSION_V6.into()
}
fn default_rng() -> RngState {
    RngState::seeded(0)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GameResult {
    White,
    Black,
    Draw,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GameConfig {
    #[serde(default = "normal")]
    pub game_style: String,
    #[serde(default)]
    pub draft_delete: bool,
    #[serde(default)]
    pub rule_card_ids: Vec<String>,
    #[serde(default = "star_limit")]
    pub star_win_limit: u32,
    #[serde(default = "enabled")]
    pub deathmatch_enabled: bool,
    #[serde(default = "deathmatch_limit")]
    pub deathmatch_limit_turns: u32,
}
fn normal() -> String {
    "normal".into()
}
fn star_limit() -> u32 {
    45
}
fn deathmatch_limit() -> u32 {
    10
}
fn enabled() -> bool {
    true
}
impl Default for GameConfig {
    fn default() -> Self {
        Self {
            game_style: normal(),
            draft_delete: false,
            rule_card_ids: Vec::new(),
            star_win_limit: star_limit(),
            deathmatch_enabled: true,
            deathmatch_limit_turns: deathmatch_limit(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PublicEvent {
    pub protocol_version: String,
    pub actor: Color,
    pub action: Action,
    pub turn_changed: bool,
    pub public: Sides<PublicTransition>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BoardChange {
    pub square: Square,
    pub before: Option<Value>,
    pub after: Option<Value>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PublicTransition {
    pub kind: String,
    pub actor: Color,
    pub next_actor: Color,
    pub phase: String,
    pub board_changes: Vec<BoardChange>,
    pub own_cards: Vec<Value>,
    pub revealed_opponent_cards: Vec<Value>,
    pub captures: Sides<Vec<Value>>,
    pub result: Value,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResultRecord {
    pub protocol_version: String,
    pub status: String,
    pub winner: Option<Color>,
    pub outcome: Option<GameResult>,
    pub reason: String,
}
impl ResultRecord {
    fn validate(&self) -> Result<()> {
        let expected = match self.outcome {
            Some(GameResult::White) => Some(Color::White),
            Some(GameResult::Black) => Some(Color::Black),
            _ => None,
        };
        if self.protocol_version != "accelerate-result-v1"
            || !matches!(self.status.as_str(), "ongoing" | "terminal")
            || self.winner != expected
            || (self.status == "ongoing" && (self.outcome.is_some() || !self.reason.is_empty()))
            || (self.status == "terminal" && self.outcome.is_none())
        {
            return Err(EngineError::InvalidState(
                "invalid result record in history".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Observation {
    pub protocol_version: String,
    pub viewer: Color,
    pub turn: Color,
    pub board: Vec<Vec<Option<Value>>>,
    pub own_cards: Vec<Value>,
    pub opponent_hand_count: usize,
    pub public_state: Fields,
    pub history: Vec<Value>,
    pub information_state_key: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ObservationPolicy {
    protocol_version: String,
    projection_version: String,
    rules_version: String,
    pub(crate) state_public_fields: Vec<String>,
    pub(crate) piece_public_fields: Vec<String>,
    pub(crate) card_public_fields: Vec<String>,
    pub(crate) derived_public_fields: Vec<String>,
    pub(crate) state_value_schemas: Fields,
    pub(crate) surface_schemas: Fields,
    pub(crate) public_piece_schema: Value,
    pub(crate) card_revelation_schema: Value,
    pub(crate) selection_schema: Value,
    pub(crate) deathmatch_schema: Value,
}
struct ObservationSource {
    policy: ObservationPolicy,
    hash: String,
}

fn parse_observation_source(source: &str, ruleset_id: &str) -> Result<ObservationSource> {
    let value: Value = serde_json::from_str(source).map_err(EngineError::serialization)?;
    let hash = format!(
        "{:x}",
        Sha256::digest(serde_jcs::to_vec(&value).map_err(EngineError::serialization)?)
    );
    let policy: ObservationPolicy =
        serde_json::from_value(value).map_err(EngineError::serialization)?;
    if policy.rules_version != ruleset_id {
        return Err(EngineError::InvalidState(
            "observation policy and rules version disagree".into(),
        ));
    }
    Ok(ObservationSource { policy, hash })
}

fn observation_source_for_ruleset(ruleset_id: &str) -> Result<&'static ObservationSource> {
    static V6: std::sync::OnceLock<Result<ObservationSource>> = std::sync::OnceLock::new();
    static V7: std::sync::OnceLock<Result<ObservationSource>> = std::sync::OnceLock::new();
    let source = match ruleset_id {
        RULES_VERSION_V6 => V6.get_or_init(|| {
            parse_observation_source(
                include_str!("../../contracts/catalog/observation-20260927.json"),
                RULES_VERSION_V6,
            )
        }),
        RULES_VERSION_V7 => V7.get_or_init(|| {
            parse_observation_source(
                include_str!("../../contracts/catalog/observation-20260928.json"),
                RULES_VERSION_V7,
            )
        }),
        other => {
            return Err(EngineError::UnsupportedFeature(format!(
                "observation policy for rules version {other}"
            )));
        }
    };
    source.as_ref().map_err(Clone::clone)
}

pub(crate) fn observation_policy_for_ruleset(
    ruleset_id: &str,
) -> Result<&'static ObservationPolicy> {
    Ok(&observation_source_for_ruleset(ruleset_id)?.policy)
}

pub(crate) fn observation_policy_hash_for_ruleset(ruleset_id: &str) -> Result<&'static str> {
    Ok(&observation_source_for_ruleset(ruleset_id)?.hash)
}

pub(crate) fn observation_protocol_for_ruleset(ruleset_id: &str) -> Result<&'static str> {
    Ok(&observation_policy_for_ruleset(ruleset_id)?.protocol_version)
}

pub(crate) fn observation_projection_for_ruleset(ruleset_id: &str) -> Result<&'static str> {
    Ok(&observation_policy_for_ruleset(ruleset_id)?.projection_version)
}

pub(crate) fn observation_protocol() -> &'static str {
    observation_protocol_for_ruleset(RULES_VERSION_V6).expect("frozen v6 observation policy")
}
pub(crate) fn observation_projection() -> &'static str {
    observation_projection_for_ruleset(RULES_VERSION_V6).expect("frozen v6 observation policy")
}
pub(crate) fn observation_policy_hash() -> &'static str {
    observation_policy_hash_for_ruleset(RULES_VERSION_V6).expect("frozen v6 observation policy")
}
pub(crate) fn observation_policy() -> &'static ObservationPolicy {
    observation_policy_for_ruleset(RULES_VERSION_V6).expect("frozen v6 observation policy")
}
fn public_object(value: Value, names: &[String]) -> Value {
    Value::Object(
        value
            .as_object()
            .expect("public source object")
            .iter()
            .filter(|(name, _)| names.contains(name))
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect(),
    )
}

impl GameState {
    pub(crate) fn is_ai_simulation(&self) -> bool {
        self.ai_simulation_depth > 0 || self.threat_probe_depth > 0
    }
    pub fn new(config: GameConfig, seed: u64) -> Result<Self> {
        crate::draft::initialize(config, seed)
    }
    pub fn at(&self, square: Square) -> Option<&Piece> {
        self.board
            .get(square.row as usize)?
            .get(square.col as usize)?
            .as_ref()
    }
    pub(crate) fn royal_identity(&self, piece: &Piece) -> bool {
        piece.is_royal()
            || (piece.flag("regencyHeir")
                && self.flag("kingDead", piece.color)
                && self.flag("regency", piece.color))
    }
    pub(crate) fn democracy_protects_royal(&self, piece: &Piece) -> bool {
        self.flag("democracy", piece.color)
            && (piece.flag("regencyHeir")
                || piece.flag("crownRoyal")
                || matches!(piece.kind.as_str(), "king" | "royalKnight" | "shotgunKing"))
    }
    pub fn decision_actor(&self) -> Color {
        let decision_color = |name| {
            self.extra
                .get(name)
                .and_then(|window| window.get("color"))
                .and_then(Value::as_str)
                .and_then(|color| match color {
                    "white" => Some(Color::White),
                    "black" => Some(Color::Black),
                    _ => None,
                })
        };
        if self.mode == "draft" {
            return decision_color("draft").unwrap_or(self.turn);
        }
        [
            "pendingPromotion",
            "activeTrolley",
            "ruleTicketChoice",
            "jokerChoice",
            "barricadeDirectionChoice",
        ]
        .into_iter()
        .find_map(decision_color)
        .unwrap_or(self.turn)
    }
    pub fn at_mut(&mut self, square: Square) -> Option<&mut Piece> {
        self.board
            .get_mut(square.row as usize)?
            .get_mut(square.col as usize)?
            .as_mut()
    }
    pub fn flag(&self, name: &str, color: impl Into<PieceColor>) -> bool {
        let Some(color) = color.into().owner() else {
            return false;
        };
        match self.extra.get(name) {
            Some(Value::Bool(value)) => *value,
            Some(Value::Object(sides)) => sides
                .get(color.as_str())
                .and_then(|value| {
                    value
                        .as_bool()
                        .or_else(|| value.as_f64().map(|number| number != 0.0))
                })
                .unwrap_or(false),
            _ => false,
        }
    }
    pub fn result(&self) -> Option<GameResult> {
        match self.winner.as_deref() {
            Some("white") => Some(GameResult::White),
            Some("black") => Some(GameResult::Black),
            Some("draw") => Some(GameResult::Draw),
            _ if self.mode == "gameover" => Some(GameResult::Draw),
            _ => None,
        }
    }
    pub fn validate_and_identify(&mut self) -> Result<()> {
        if self.ruleset_id != RULES_VERSION_V6 {
            return Err(if self.ruleset_id == RULES_VERSION_V7 {
                EngineError::UnsupportedFeature("v7 rules profile is not executable".into())
            } else {
                EngineError::InvalidState("unknown rules version".into())
            });
        }
        self.validate_source_shape_and_identify()
    }

    /// Validate the source-shaped v7 DTO and repeated piece identities only.
    /// The caller must set `ruleset_id` from the verified outer Position
    /// envelope first. This does not admit v7 to any executable Position API;
    /// source action, result, and observation semantics still need porting.
    pub fn validate_v7_snapshot_shape_and_identify(&mut self) -> Result<()> {
        if self.ruleset_id != RULES_VERSION_V7 {
            return Err(EngineError::InvalidState(
                "v7 snapshot shape requires the v7 rules version".into(),
            ));
        }
        self.validate_source_shape_and_identify()
    }

    fn validate_source_shape_and_identify(&mut self) -> Result<()> {
        for value in self.extra.values() {
            validate_json_value(value, 1)?;
        }
        for piece in self
            .board
            .iter()
            .flatten()
            .flatten()
            .chain(self.captures.white.iter())
            .chain(self.captures.black.iter())
        {
            for value in piece.extra.values() {
                validate_json_value(value, 4)?;
            }
        }
        for card in self.deck_slots.white.iter().chain(&self.deck_slots.black) {
            for value in card.extra.values() {
                validate_json_value(value, 4)?;
            }
        }
        for event in &self.history {
            validate_json_value(event, 2)?;
        }
        // GameState is the frozen source DTO for official v6/v7 games. Synthetic
        // variable geometry is validated by BoardGeometry in SpatialState.
        if self.board.len() != 8 || self.board.iter().any(|row| row.len() != 8) {
            return Err(EngineError::InvalidState("board must be 8x8".into()));
        }
        if self.actions_remaining > 32 {
            return Err(EngineError::InvalidState(
                "actionsRemaining exceeds supported bound".into(),
            ));
        }
        if self.winner.as_deref() == Some("") {
            self.winner = None;
        }
        if self
            .winner
            .as_deref()
            .is_some_and(|value| !matches!(value, "white" | "black" | "draw"))
        {
            return Err(EngineError::InvalidState("invalid winner".into()));
        }
        if self
            .rng
            .tape
            .iter()
            .any(|value| !value.is_finite() || !(0.0..1.0).contains(value))
            || self.rng.cursor as u64 > 9_007_199_254_740_991u64
        {
            return Err(EngineError::InvalidState("invalid random tape".into()));
        }
        for card in self
            .deck_slots
            .white
            .iter()
            .chain(&self.deck_slots.black)
            .filter(|card| !card.vacant)
        {
            if !card.stars.is_finite()
                || card.stars < 0.0
                || card
                    .extra
                    .get("ratingHalfStars")
                    .is_some_and(|rating| rating.as_u64().is_none())
            {
                return Err(EngineError::InvalidState(
                    "card rating must be a finite nonnegative half-star value".into(),
                ));
            }
            if card.stars != 0.0
                && card.extra.contains_key("ratingHalfStars")
                && card.stars != card.star_value()
            {
                return Err(EngineError::InvalidState(
                    "card stars and ratingHalfStars disagree".into(),
                ));
            }
        }
        // PublicEvent is the v6 replay protocol. A v7 snapshot may carry a
        // different history shape, which is bounded above but not interpreted
        // until the v7 replay contract is implemented.
        if self.ruleset_id == RULES_VERSION_V6 {
            for event in &self.history {
                let event: PublicEvent =
                    serde_json::from_value(event.clone()).map_err(|error| {
                        EngineError::InvalidState(format!("invalid game history event: {error}"))
                    })?;
                if event.protocol_version != "accelerate-game-event-v1"
                    || event.action.position_key.is_some()
                    || event.actor != event.action.color
                    || event
                        .action
                        .from
                        .is_some_and(|square| square.row >= 8 || square.col >= 8)
                    || event
                        .action
                        .destination
                        .as_ref()
                        .is_some_and(|square| square.row >= 8 || square.col >= 8)
                {
                    return Err(EngineError::InvalidState(
                        "unsupported history protocol, action actor or internal action binding"
                            .into(),
                    ));
                }
                for transition in [&event.public.white, &event.public.black] {
                    if transition.kind != "transition"
                        || transition.phase.is_empty()
                        || transition.actor != event.actor
                        || transition
                            .board_changes
                            .iter()
                            .any(|change| change.square.row >= 8 || change.square.col >= 8)
                    {
                        return Err(EngineError::InvalidState(
                            "invalid public history transition".into(),
                        ));
                    }
                    let result: ResultRecord = serde_json::from_value(transition.result.clone())
                        .map_err(|error| {
                            EngineError::InvalidState(format!("invalid history result: {error}"))
                        })?;
                    result.validate()?;
                    for value in transition
                        .board_changes
                        .iter()
                        .flat_map(|change| [&change.before, &change.after])
                        .flatten()
                    {
                        crate::observation::validate_public_piece(value, "history.piece")?;
                    }
                    crate::observation::validate_public_cards(
                        &transition.own_cards,
                        "history.ownCards",
                    )?;
                    crate::observation::validate_public_cards(
                        &transition.revealed_opponent_cards,
                        "history.revealedOpponentCards",
                    )?;
                    for capture in transition
                        .captures
                        .white
                        .iter()
                        .chain(&transition.captures.black)
                    {
                        if capture.as_object().is_none_or(|object| {
                            !object.contains_key("type")
                                || !object.contains_key("color")
                                || object.keys().any(|name| {
                                    !matches!(
                                        name.as_str(),
                                        "type" | "color" | "logDir" | "windmillMode"
                                    )
                                })
                        }) {
                            return Err(EngineError::InvalidState(
                                "nonpublic capture data in history".into(),
                            ));
                        }
                    }
                    if transition.captures.white.len() > 12 || transition.captures.black.len() > 12
                    {
                        return Err(EngineError::InvalidState(
                            "public captures exceed the site display window".into(),
                        ));
                    }
                    for card in transition
                        .own_cards
                        .iter()
                        .chain(&transition.revealed_opponent_cards)
                    {
                        if card.as_object().is_none_or(|object| {
                            object
                                .keys()
                                .any(|name| !observation_policy().card_public_fields.contains(name))
                        }) {
                            return Err(EngineError::InvalidState(
                                "nonpublic card data in history".into(),
                            ));
                        }
                    }
                }
            }
        }
        let mut identities = BTreeMap::<String, Piece>::new();
        for row in 0..8 {
            for col in 0..8 {
                if let Some(piece) = &mut self.board[row][col] {
                    if piece.kind.is_empty() {
                        return Err(EngineError::InvalidState("piece type empty".into()));
                    }
                    if piece.id.is_empty() {
                        let anchor_row = piece
                            .extra
                            .get("anchorRow")
                            .and_then(Value::as_u64)
                            .unwrap_or(row as u64);
                        let anchor_col = piece
                            .extra
                            .get("anchorCol")
                            .and_then(Value::as_u64)
                            .unwrap_or(col as u64);
                        piece.id = format!("{}-{anchor_row}-{anchor_col}", piece.color.as_str());
                    }
                    if let Some(previous) = identities.get(&piece.id) {
                        if previous != piece || !piece.is_large() {
                            return Err(EngineError::InvalidState(format!(
                                "conflicting identity {}",
                                piece.id
                            )));
                        }
                    } else {
                        identities.insert(piece.id.clone(), piece.clone());
                    }
                }
            }
        }
        // Frozen source exile can relocate one cell of a large piece while
        // its other three cells still share the same object. Source restore
        // accepts that state. Identity and attribute consistency belong here;
        // canonical footprint geometry belongs to the action that places it.
        Ok(())
    }
    pub fn observe(&self, viewer: Color) -> Observation {
        assert_eq!(
            self.ruleset_id, RULES_VERSION_V6,
            "GameState::observe is the legacy v6-only infallible projection"
        );
        self.observe_checked(viewer)
            .expect("validated v6 state has a pinned observation policy")
    }

    pub(crate) fn observe_checked(&self, viewer: Color) -> Result<Observation> {
        let policy = observation_policy_for_ruleset(&self.ruleset_id)?;
        let v7 = self.ruleset_id == RULES_VERSION_V7;
        let fog = if v7 {
            crate::observation::fog_visible_squares_v7(self, viewer)?
        } else {
            None
        };
        let definitions = crate::draft::definitions_for_ruleset(&self.ruleset_id)?;
        let state_value = serde_json::to_value(self).expect("validated state serializes");
        let mut public_state = public_object(state_value, &policy.state_public_fields)
            .as_object()
            .expect("state object")
            .clone();
        public_state.insert(
            "projectionVersion".into(),
            json!(observation_projection_for_ruleset(&self.ruleset_id)?),
        );
        public_state.insert(
            "deathmatchStatus".into(),
            crate::observation::deathmatch_status(self),
        );
        public_state.extend(
            (if v7 {
                crate::observation::board_surface_v7_with_fog(self, viewer, fog.as_ref())?
            } else {
                crate::observation::board_surface(self, viewer)
            })
            .as_object()
            .expect("surface object")
            .clone(),
        );
        public_state.insert(
            "observationPolicyHash".into(),
            json!(observation_policy_hash_for_ruleset(&self.ruleset_id)?),
        );
        let project_cards = |color: Color| -> Result<Vec<Value>> {
            self.deck_slots
                .get(color)
                .iter()
                .enumerate()
                .filter(|(_, card)| !card.vacant)
                .map(|(slot, card)| {
                    let mut view = public_object(
                        serde_json::to_value(card).expect("card serializes"),
                        &policy.card_public_fields,
                    );
                    view.as_object_mut()
                        .expect("projected card")
                        .shift_remove("revealed");
                    view["slot"] = json!(slot);
                    if let Some(revealed) =
                        crate::observation::card_revelation_for_ruleset(card, &self.ruleset_id)?
                    {
                        view["revealed"] = revealed;
                    }
                    Ok(view)
                })
                .collect()
        };
        let own_cards = project_cards(viewer)?;
        let other_cards = project_cards(viewer.opponent())?;
        public_state.insert("revealedOpponentCards".into(), json!(other_cards));
        public_state.insert(
            "ownStarTotal".into(),
            json!(crate::flow::star_total(self, viewer)),
        );
        public_state.insert(
            "opponentStarTotal".into(),
            json!(crate::flow::star_total(self, viewer.opponent())),
        );
        public_state.insert("rulesVersion".into(), json!(self.ruleset_id));
        public_state.insert(
            "catalogVersion".into(),
            json!(crate::v7_execution_profile::catalog_version()?),
        );
        let style = self
            .extra
            .get("gameStyle")
            .and_then(Value::as_str)
            .unwrap_or("normal");
        let phase = if style == "grand" {
            "GRAND"
        } else if !self.flag("middleDraftDone", viewer) {
            "OPENING"
        } else if self.flag("endDraftDone", viewer) {
            "END"
        } else {
            "MIDDLE"
        };
        public_state.insert("phase".into(), json!(phase));
        let rule_ids = self
            .extra
            .get("appliedRuleCard")
            .and_then(|card| card.get("id"))
            .and_then(Value::as_str)
            .into_iter()
            .chain(
                self.extra
                    .get("additionalRuleCards")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|card| card.get("id").and_then(Value::as_str)),
            )
            .collect::<Vec<_>>();
        public_state.insert("ruleCardIds".into(), json!(rule_ids));
        public_state.insert(
            "pendingRuleCardIds".into(),
            json!(
                self.extra
                    .get("pendingRuleTickets")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|entry| entry.get("ruleId").and_then(Value::as_str))
                    .filter(|id| {
                        definitions
                            .definitions
                            .iter()
                            .any(|card| card.get("id").and_then(Value::as_str) == Some(*id))
                    })
                    .collect::<Vec<_>>()
            ),
        );
        public_state.insert("captures".into(), json!(self.public_captures()));
        let clock_fields = [
            "enabled",
            "initialMs",
            "incrementMs",
            "whiteMs",
            "blackMs",
            "runningColor",
            "timeoutWinner",
            "timeoutLoser",
        ]
        .map(str::to_owned);
        public_state.insert(
            "clock".into(),
            self.extra
                .get("clock")
                .filter(|v| v.is_object())
                .map(|v| public_object(v.clone(), &clock_fields))
                .unwrap_or(Value::Null),
        );
        let last_move = self.extra.get("lastMove").filter(|value| {
            value.is_object()
                && value.get("hiddenFrom").and_then(Value::as_str) != Some(viewer.as_str())
        });
        let last_move = last_move
            .map(|value| {
                let mut projected = public_object(
                    value.clone(),
                    &["from", "to", "color", "kind"].map(str::to_owned),
                );
                for field in ["from", "to"] {
                    if let Some(square) = projected.get_mut(field) {
                        *square = public_object(square.clone(), &["row", "col"].map(str::to_owned));
                    }
                }
                projected
            })
            .unwrap_or(Value::Null);
        public_state.insert("lastMove".into(), last_move);
        let selection = if let Some(pending) =
            self.extra.get("pendingPromotion").filter(|v| !v.is_null())
        {
            json!({"kind":"promotion","color":pending.get("color"),"row":pending.get("row"),"col":pending.get("col"),"choices":if pending.get("color").and_then(Value::as_str)==Some(viewer.as_str()){pending.get("choices").cloned().unwrap_or_else(||json!([]))}else{json!([])}})
        } else if let Some(trolley) = self.extra.get("activeTrolley").filter(|v| !v.is_null()) {
            let choices=trolley.get("choices").and_then(Value::as_array).into_iter().flatten().map(|choice|choice.get("pieces").and_then(Value::as_array).into_iter().flatten().map(|cell|json!({"type":cell.get("type").or_else(||cell.get("piece").and_then(|p|p.get("type"))),"color":cell.get("color").or_else(||cell.get("piece").and_then(|p|p.get("color")))})).collect::<Vec<_>>()).collect::<Vec<_>>();
            json!({"kind":"trolley","color":trolley.get("color"),"choices":choices})
        } else {
            Value::Null
        };
        public_state.insert("selectionPhase".into(), selection);
        let taboo = self
            .extra
            .get("tabooPending")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|entry| json!({"color":entry.get("color"),"square":entry.get("square")}))
            .collect::<Vec<_>>();
        public_state.insert("tabooPending".into(), json!(taboo));
        let plans=self.extra.get("pendingFreeMoves").and_then(Value::as_array).into_iter().flatten().filter(|entry|entry.get("color").and_then(Value::as_str)==Some(viewer.as_str())).map(|entry| {
            let moves=entry.get("moves").and_then(Value::as_array).into_iter().flatten().map(|m|json!({"from":m.get("from"),"to":m.get("to")})).collect::<Vec<_>>();
            json!({"kind":"premove","triggerColor":entry.get("triggerColor"),"triggerTurn":entry.get("triggerTurn"),"moves":moves})
        }).collect::<Vec<_>>();
        public_state.insert("ownPlans".into(), json!(plans));
        if let Some(winter) = self.extra.get("winterKingdom") {
            public_state.insert(
                "winterKingdom".into(),
                json!({"enabled":crate::observation::truth(winter.get("enabled"))}),
            );
        }
        if let Some(capture) = self.extra.get("captureTheFlag") {
            public_state.insert(
                "captureTheFlag".into(),
                json!({"enabled":crate::observation::truth(Some(capture))}),
            );
        }
        if self.mode == "draft"
            && let Some(draft) = self.extra.get("draft").and_then(Value::as_object)
            && (draft.get("color").and_then(Value::as_str) == Some(viewer.as_str())
                || draft.get("kind").and_then(Value::as_str) == Some("grand"))
        {
            let choices = draft
                .get("choices")
                .and_then(Value::as_array)
                .map(|cards| {
                    cards
                        .iter()
                        .map(|card| {
                            let mut view = public_object(card.clone(), &policy.card_public_fields);
                            view.as_object_mut()
                                .expect("projected card")
                                .shift_remove("revealed");
                            if let Ok(card) = serde_json::from_value::<CardSlot>(card.clone())
                                && let Some(revealed) =
                                    crate::observation::card_revelation_for_ruleset(
                                        &card,
                                        &self.ruleset_id,
                                    )?
                            {
                                view["revealed"] = revealed;
                            }
                            Ok(view)
                        })
                        .collect::<Result<Vec<_>>>()
                })
                .transpose()?
                .unwrap_or_default();
            public_state.insert("draft".into(),json!({"kind":draft.get("kind").and_then(Value::as_str).unwrap_or(style),"phase":draft.get("phase"),"color":draft.get("color"),"choices":choices}));
        }
        let board = self
            .board
            .iter()
            .enumerate()
            .map(|(row, cells)| {
                cells
                    .iter()
                    .enumerate()
                    .map(|(col, cell)| {
                        let Some(piece) = cell.as_ref() else {
                            return Ok(None);
                        };
                        let at = Square {
                            row: row as u8,
                            col: col as u8,
                        };
                        let visible = if v7 {
                            crate::observation::piece_visible_to_color_at_v7_with_fog(
                                self,
                                piece,
                                at,
                                viewer,
                                fog.as_ref(),
                            )?
                        } else {
                            self.piece_visible(piece, at, viewer)
                        };
                        if !visible {
                            return Ok(None);
                        }
                        let visible_type = crate::observation::visible_type(self, piece, viewer);
                        Ok(Some(crate::observation::piece_view(
                            self,
                            piece,
                            Square {
                                row: row as u8,
                                col: col as u8,
                            },
                            viewer,
                            &visible_type,
                        )))
                    })
                    .collect::<Result<Vec<_>>>()
            })
            .collect::<Result<Vec<_>>>()?;
        let history = self
            .history
            .iter()
            .map(|entry| {
                entry
                    .get("public")
                    .and_then(|public| public.get(viewer.as_str()))
                    .cloned()
                    .ok_or_else(|| {
                        EngineError::InvalidState(format!(
                            "history entry lacks the {} public projection",
                            viewer.as_str()
                        ))
                    })
            })
            .collect::<Result<Vec<_>>>()?;
        let mut observation = Observation {
            protocol_version: observation_protocol_for_ruleset(&self.ruleset_id)?.into(),
            viewer,
            turn: self.turn,
            board,
            own_cards,
            opponent_hand_count: other_cards.len(),
            public_state,
            history,
            information_state_key: String::new(),
        };
        let mut content = serde_json::to_value(&observation).expect("observation serializes");
        content
            .as_object_mut()
            .expect("observation object")
            .remove("informationStateKey");
        let bytes = serde_jcs::to_vec(&content).expect("validated observation canonicalizes");
        observation.information_state_key = format!("{:x}", Sha256::digest(bytes));
        Ok(observation)
    }
    /// Full viewer-facing projection, including the site's highlight surface.
    /// Unsupported active rules are explicit errors at the language boundary.
    pub fn try_observe(&self, viewer: Color) -> Result<Observation> {
        let hints = match self.ruleset_id.as_str() {
            RULES_VERSION_V6 => crate::movement::public_hints(self, viewer)?,
            RULES_VERSION_V7 => crate::observation::public_hints_v7(self, viewer)?,
            other => {
                return Err(EngineError::UnsupportedFeature(format!(
                    "observation execution for rules version {other}"
                )));
            }
        };
        let mut observation = self.observe_checked(viewer)?;
        observation.public_state.insert("legalHints".into(), hints);
        crate::observation::validate_projection_for_ruleset(&observation, &self.ruleset_id)?;
        observation.refresh_key();
        Ok(observation)
    }
    pub(crate) fn piece_visible(&self, piece: &Piece, square: Square, viewer: Color) -> bool {
        if self.mode == "gameover" {
            return true;
        }
        if piece.color == viewer {
            return true;
        }
        if let Some(hidden) = piece
            .extra
            .get("hiddenFrom")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
        {
            return hidden != viewer.as_str();
        }
        if piece.color.owner().is_some()
            && self.flag("camouflageRule", viewer)
            && !self.royal_identity(piece)
        {
            let row = piece
                .extra
                .get("anchorRow")
                .and_then(Value::as_u64)
                .unwrap_or(u64::from(square.row));
            let col = piece
                .extra
                .get("anchorCol")
                .and_then(Value::as_u64)
                .unwrap_or(u64::from(square.col));
            let light = (row + col).is_multiple_of(2);
            let matching = if piece.color == Color::White {
                light
            } else {
                !light
            };
            if matching {
                return false;
            }
        }
        true
    }
    pub(crate) fn public_captures(&self) -> Sides<Vec<Value>> {
        let names = ["type", "color", "logDir", "windmillMode"].map(str::to_owned);
        let project = |color| {
            self.captures
                .get(color)
                .iter()
                .skip(self.captures.get(color).len().saturating_sub(12))
                .map(|piece| {
                    public_object(
                        serde_json::to_value(piece).expect("piece serializes"),
                        &names,
                    )
                })
                .collect()
        };
        Sides {
            white: project(Color::White),
            black: project(Color::Black),
            white_first: true,
        }
    }
    pub(crate) fn set_flag(&mut self, name: &str, color: Color, value: bool) {
        let entry = self
            .extra
            .entry(name.to_string())
            .or_insert_with(|| json!({"white":false,"black":false}));
        if let Value::Object(sides) = entry {
            sides.insert(color.as_str().into(), Value::Bool(value));
        }
    }
}

impl Observation {
    pub(crate) fn refresh_key(&mut self) {
        let mut value = serde_json::to_value(&*self).expect("observation serializes");
        value
            .as_object_mut()
            .expect("observation object")
            .remove("informationStateKey");
        let bytes = serde_jcs::to_vec(&value).expect("validated observation canonicalizes");
        self.information_state_key = format!("{:x}", Sha256::digest(bytes));
    }
}

#[cfg(test)]
mod piece_source_shape_tests {
    use super::{Piece, PieceColor};

    #[test]
    fn barricade_wall_retains_frozen_source_shape_without_moved() {
        // Frozen main-OahWs0tU.js:105160 assigns exactly these properties.
        let source = r#"{"type":"wall","color":"neutral","id":"wall-4-3"}"#;
        let mut wall: Piece = serde_json::from_str(source).unwrap();
        assert_eq!(wall.color, PieceColor::Neutral);
        assert!(!wall.moved);
        assert_eq!(serde_json::to_string(&wall).unwrap(), source);

        wall.moved = true;
        let moved = serde_json::to_value(&wall).unwrap();
        assert_eq!(moved["moved"], true);
    }

    #[test]
    fn explicit_false_moved_property_and_native_piece_still_serialize() {
        let source = r#"{"type":"rook","color":"white","moved":false,"id":"rook-1"}"#;
        let source_piece: Piece = serde_json::from_str(source).unwrap();
        assert_eq!(serde_json::to_string(&source_piece).unwrap(), source);

        let native = Piece::new("rook", PieceColor::White, "rook-2");
        assert_eq!(serde_json::to_value(&native).unwrap()["moved"], false);
    }
}
