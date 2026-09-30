//! Checked board extents and signed coordinates for the compositional engine.
//! Source v6's `Square` remains the fixed 8x8 wire representation.

use crate::{EngineError, Result};
use serde::{Deserialize, Serialize};

pub const MAX_ENGINE_CELLS: usize = 4_096;

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(deny_unknown_fields)]
pub struct Coord {
    pub row: i32,
    pub col: i32,
}

impl Coord {
    pub const fn new(row: i32, col: i32) -> Self {
        Self { row, col }
    }

    pub fn offset(self, by: Offset) -> Option<Self> {
        Some(Self {
            row: self.row.checked_add(by.row)?,
            col: self.col.checked_add(by.col)?,
        })
    }
}

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(deny_unknown_fields)]
pub struct Offset {
    pub row: i32,
    pub col: i32,
}

impl Offset {
    pub const fn new(row: i32, col: i32) -> Self {
        Self { row, col }
    }

    pub fn between(anchor: Coord, cell: Coord) -> Option<Self> {
        Some(Self {
            row: cell.row.checked_sub(anchor.row)?,
            col: cell.col.checked_sub(anchor.col)?,
        })
    }
}

/// Row-major rectangle. The coordinate origin is stable when the rectangle
/// expands or contracts; only the allocation index changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BoardGeometry {
    min_row: i32,
    min_col: i32,
    height: u16,
    width: u16,
}

impl<'de> Deserialize<'de> for BoardGeometry {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct RawGeometry {
            min_row: i32,
            min_col: i32,
            height: u16,
            width: u16,
        }

        let raw = RawGeometry::deserialize(deserializer)?;
        Self::new(raw.min_row, raw.min_col, raw.height, raw.width).map_err(serde::de::Error::custom)
    }
}

impl BoardGeometry {
    pub fn new(min_row: i32, min_col: i32, height: u16, width: u16) -> Result<Self> {
        let geometry = Self {
            min_row,
            min_col,
            height,
            width,
        };
        geometry.validate()?;
        Ok(geometry)
    }

    pub fn validate(self) -> Result<()> {
        let area = usize::from(self.height)
            .checked_mul(usize::from(self.width))
            .ok_or_else(|| EngineError::InvalidState("board area overflow".into()))?;
        if self.height == 0 || self.width == 0 || area > MAX_ENGINE_CELLS {
            return Err(EngineError::InvalidState(format!(
                "board area {area} must be within 1..={MAX_ENGINE_CELLS}"
            )));
        }
        self.min_row
            .checked_add(i32::from(self.height) - 1)
            .ok_or_else(|| EngineError::InvalidState("board row extent overflows i32".into()))?;
        self.min_col
            .checked_add(i32::from(self.width) - 1)
            .ok_or_else(|| EngineError::InvalidState("board column extent overflows i32".into()))?;
        Ok(())
    }

    pub const fn min_row(self) -> i32 {
        self.min_row
    }

    pub const fn min_col(self) -> i32 {
        self.min_col
    }

    pub const fn height(self) -> u16 {
        self.height
    }

    pub const fn width(self) -> u16 {
        self.width
    }

    pub fn area(self) -> usize {
        usize::from(self.height) * usize::from(self.width)
    }

    pub fn index(self, coord: Coord) -> Option<usize> {
        let row = i64::from(coord.row) - i64::from(self.min_row);
        let col = i64::from(coord.col) - i64::from(self.min_col);
        if row < 0 || col < 0 || row >= i64::from(self.height) || col >= i64::from(self.width) {
            return None;
        }
        Some(row as usize * usize::from(self.width) + col as usize)
    }

    pub fn contains(self, coord: Coord) -> bool {
        self.index(coord).is_some()
    }

    pub fn coord_at(self, index: usize) -> Option<Coord> {
        if index >= self.area() {
            return None;
        }
        let row = index / usize::from(self.width);
        let col = index % usize::from(self.width);
        Some(Coord {
            row: self.min_row + row as i32,
            col: self.min_col + col as i32,
        })
    }

    pub fn coordinates(self) -> impl ExactSizeIterator<Item = Coord> {
        (0..self.area()).map(move |index| {
            self.coord_at(index)
                .expect("index from validated geometry area")
        })
    }
}
