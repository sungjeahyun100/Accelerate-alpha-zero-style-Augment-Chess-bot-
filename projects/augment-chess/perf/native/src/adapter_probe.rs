//! A synthetic read-only probe isolates generic registry dispatch cost.
//! It does not estimate game rule cost or transactional host overhead.
use adapter_runtime::{
    AdapterDescriptor, AdapterError, AdapterHost, AdapterObject, AdapterOutput, AdapterRegistry,
    AdapterRequest, AdapterSelection, CallLimits, CallMeter, CapabilityAccess,
    CapabilityDescriptor, CommittedRevision, ContractVersion, InvocationControl, NeverCancelled,
    SchemaRef,
};
use serde_json::{Value, json};
use std::hint::black_box;

use super::measure;

const HASH: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

struct ReadHost(u64);

impl AdapterHost<u64> for ReadHost {
    type Transaction = Box<u64>;

    fn revision(&self) -> &str {
        "fixed-revision"
    }

    fn read_state(&self) -> &u64 {
        &self.0
    }

    fn begin_transaction(&mut self) -> Result<Self::Transaction, AdapterError> {
        Ok(Box::new(self.0))
    }

    fn commit_transaction(
        &mut self,
        transaction: Self::Transaction,
        _expected_revision: &str,
    ) -> Result<CommittedRevision, (AdapterError, Self::Transaction)> {
        let revision = CommittedRevision::new("fixed-revision").expect("nonempty revision");
        self.0 = *transaction;
        Ok(revision)
    }

    fn rollback_transaction(&mut self, _transaction: Self::Transaction) {}
}

struct ReadObject(AdapterDescriptor);

impl AdapterObject<u64, u64, u64> for ReadObject {
    fn descriptor(&self) -> &AdapterDescriptor {
        &self.0
    }

    fn invoke_read(
        &self,
        state: &u64,
        request: &AdapterRequest<u64>,
        meter: &mut CallMeter<'_>,
    ) -> Result<AdapterOutput<u64>, AdapterError> {
        meter.consume_work(1)?;
        meter.consume_results(1)?;
        Ok(AdapterOutput::value(*state + request.payload))
    }

    fn validate_output(&self, _capability_id: &str, _result: &u64) -> Result<u64, AdapterError> {
        Ok(1)
    }
}

pub(super) fn measure_dispatch(samples: u64, warmups: u64) -> Result<Value, String> {
    let schema = |name: &str| SchemaRef {
        id: format!("urn:perf:{name}"),
        sha256: HASH.to_owned(),
    };
    let descriptor = AdapterDescriptor {
        project_id: "perf-fixture".into(),
        adapter_id: "read-counter".into(),
        contract_version: ContractVersion { major: 1, minor: 0 },
        implementation_version: "1.0.0".into(),
        capabilities: vec![CapabilityDescriptor {
            id: "read".into(),
            request_schema: schema("request"),
            response_schema: schema("response"),
            access: CapabilityAccess::ReadOnly,
        }],
        deterministic: true,
        call_limits: CallLimits {
            max_work: 1,
            max_results: 1,
        },
    };
    let request = AdapterRequest {
        request_id: "measurement".into(),
        selection: AdapterSelection {
            project_id: descriptor.project_id.clone(),
            adapter_id: descriptor.adapter_id.clone(),
            contract_version: descriptor.contract_version,
            implementation_version: descriptor.implementation_version.clone(),
            capability_id: "read".into(),
            request_schema: schema("request"),
            response_schema: schema("response"),
        },
        snapshot_revision: "fixed-revision".into(),
        call_limits: descriptor.call_limits,
        payload: 3u64,
    };
    let mut registry = AdapterRegistry::<u64, u64, u64>::new();
    registry
        .register(ReadObject(descriptor))
        .map_err(|error| format!("synthetic register: {error}"))?;
    registry.seal();
    let mut host = ReadHost(5);
    let cancellation = NeverCancelled;
    let control = InvocationControl::unlimited_time(&cancellation);
    let direct = measure(samples, warmups, 100, || {
        Ok::<u64, String>(black_box(host.0).wrapping_add(black_box(request.payload)))
    })?;
    let via_registry = measure(samples, warmups, 100, || {
        registry
            .invoke(&mut host, &request, &control)
            .map(|response| response.result)
            .map_err(|error| format!("synthetic invoke: {error}"))
    })?;
    Ok(json!({
        "scope": "synthetic-read-only-registry-dispatch",
        "direct": direct,
        "viaRegistry": via_registry,
        "interpretation": "The direct branch performs one u64 addition. Registry dispatch also checks exact selection, revision, limits and response. This synthetic delta is not an Augment Chess call cost.",
    }))
}
