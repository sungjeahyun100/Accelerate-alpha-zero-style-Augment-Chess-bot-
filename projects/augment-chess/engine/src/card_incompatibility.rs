//! Shared card-group incompatibility, independent of draft mode.
use crate::{RULES_VERSION_V6, RULES_VERSION_V7};
use serde_json::Value;
use std::collections::BTreeSet;

pub(crate) fn conflicts(rules_version: &str, id: &str, unavailable: &BTreeSet<String>) -> bool {
    // Source checks pawn direction before catalog-backed groups.
    if id == "reverse-pawns"
        && (unavailable.contains("rule-ticket") || unavailable.contains("macho-chess"))
        || matches!(id, "rule-ticket" | "macho-chess") && unavailable.contains("reverse-pawns")
    {
        return true;
    }
    let definitions = match rules_version {
        RULES_VERSION_V6 => crate::draft::definitions(),
        RULES_VERSION_V7 => {
            crate::draft::definitions_for_ruleset(RULES_VERSION_V7).expect("adopted v7 definitions")
        }
        _ => return true,
    };
    definitions.constants["LATEST_MUTUALLY_EXCLUSIVE_DRAFT_CARD_GROUPS"]
        .as_array()
        .expect("conflict groups")
        .iter()
        .any(|group| {
            group
                .as_array()
                .expect("conflict group")
                .iter()
                .any(|value| value.as_str() == Some(id))
                && group
                    .as_array()
                    .expect("group")
                    .iter()
                    .filter_map(Value::as_str)
                    .any(|other| other != id && unavailable.contains(other))
        })
}

pub(crate) fn grand_conflicts(
    rules_version: &str,
    id: &str,
    unavailable: &BTreeSet<String>,
) -> bool {
    conflicts(rules_version, id, unavailable)
        || matches!(id, "democracy" | "queens-gambit")
            && unavailable.contains(if id == "democracy" {
                "queens-gambit"
            } else {
                "democracy"
            })
}
