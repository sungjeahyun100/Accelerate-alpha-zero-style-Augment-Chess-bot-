//! 동결 원문 실행 프로필의 identity와 replay 상수 authority.
//!
//! 원문 카드 hash와 실행 프로필을 함께 catalogVersion에 묶는다. 같은 원문
//! SHA라도 초기화·bootstrap 정책이 달라지면 기존 Position을 거절한다.
use crate::{EngineError, RULES_VERSION_V7, Result};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::OnceLock;

const MAIN_SHA: &str = "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c";
pub(crate) const SOURCE_CATALOG_HASH: &str = "yt63f_Lcq4xUPu4GbbdwAyv5IUTuC079nNxvF3IFrj4";
const PARSER_SHA: &str = "fdb08546776ec6228b03e8d02b40d4ab3255bae5f401adba7ff5dad927ac5c9c";
const PROFILE_VERSION: &str = "accelerate-headless-semantic-v7-faithful-init-v1";

struct ExecutionProfile {
    catalog_version: String,
    replay: Value,
}

fn digest(value: &Value) -> Result<String> {
    let bytes = serde_jcs::to_vec(value).map_err(EngineError::serialization)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn load() -> Result<ExecutionProfile> {
    let manifest: Value = serde_json::from_str(include_str!(
        "../../contracts/catalog/execution-profile-20260928.json"
    ))
    .map_err(EngineError::serialization)?;
    let site: Value =
        serde_json::from_str(include_str!("../../contracts/catalog/site-20260928.json"))
            .map_err(EngineError::serialization)?;
    if manifest["schemaVersion"] != 1
        || manifest["manifestVersion"] != "augment-v7-execution-profile-v1"
        || manifest["profileVersion"] != PROFILE_VERSION
        || manifest["rulesVersion"] != RULES_VERSION_V7
        || manifest["sourceMainSha256"] != MAIN_SHA
        || manifest["parserSha256"] != PARSER_SHA
        || manifest["sourcePublicCatalogHash"] != SOURCE_CATALOG_HASH
        || site["rulesVersion"] != RULES_VERSION_V7
        || site["sourcePublicCatalogHash"] != SOURCE_CATALOG_HASH
        || site["executionProfile"]["version"] != PROFILE_VERSION
        || site["executionProfile"]["manifest"] != "execution-profile-20260928.json"
    {
        return Err(EngineError::InvalidState(
            "v7 execution profile source identity mismatch".into(),
        ));
    }
    let profile_sha = digest(&manifest)?;
    if site["executionProfile"]["sha256"] != profile_sha {
        return Err(EngineError::InvalidState(
            "v7 execution profile manifest digest mismatch".into(),
        ));
    }
    let catalog_version = digest(&json!({
        "contractVersion": "augment-v7-execution-catalog-v1",
        "sourcePublicCatalogHash": SOURCE_CATALOG_HASH,
        "executionProfileSha256": profile_sha,
    }))?;
    if site["catalogVersion"] != catalog_version {
        return Err(EngineError::InvalidState(
            "v7 execution catalog digest mismatch".into(),
        ));
    }
    let replay = manifest.get("replayMetadata").cloned().ok_or_else(|| {
        EngineError::InvalidState("v7 execution profile replay metadata missing".into())
    })?;
    if replay["frameKeys"]
        .as_array()
        .is_none_or(|keys| keys.len() != 222)
        || replay["codes"]
            .as_object()
            .is_none_or(|codes| codes.len() != 79)
        || replay["labels"]
            .as_object()
            .is_none_or(|labels| labels.len() != 79)
    {
        return Err(EngineError::InvalidState(
            "v7 execution profile replay metadata count mismatch".into(),
        ));
    }
    Ok(ExecutionProfile {
        catalog_version,
        replay,
    })
}

fn checked_profile() -> Result<&'static ExecutionProfile> {
    static PROFILE: OnceLock<Result<ExecutionProfile>> = OnceLock::new();
    PROFILE.get_or_init(load).as_ref().map_err(Clone::clone)
}

pub(crate) fn catalog_version() -> Result<String> {
    Ok(checked_profile()?.catalog_version.clone())
}

pub(crate) fn replay_metadata() -> &'static Value {
    // These bytes are a checked compile-time contract, not an external file.
    // Position and catalog admission also return the precise load error.
    &checked_profile()
        .expect("compiled v7 execution profile must pass identity checks")
        .replay
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_identity_pins_full_initializer_and_replay_authority() {
        let version = catalog_version().unwrap();
        assert_eq!(version.len(), 64);
        assert_ne!(version, SOURCE_CATALOG_HASH);
        let replay = replay_metadata();
        assert_eq!(replay["labels"]["bigRook"], "빅룩");
        assert_eq!(replay["codes"]["medium"], "GR");
        assert!(replay["labels"].get("football").is_some());
    }
}
