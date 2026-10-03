use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use adapter_runtime::{
    AdapterDescriptor, AdapterError, AdapterErrorKind, AdapterHost, AdapterObject, AdapterOutcome,
    AdapterOutput, AdapterRegistry, AdapterRequest, AdapterResponse, AdapterSelection, CallLimits,
    CallMeter, Cancellation, CapabilityAccess, CapabilityDescriptor, CommittedRevision,
    ContractVersion, InvocationControl, NeverCancelled, PageInfo, SchemaRef,
};

const HASH: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn schema(name: &str) -> SchemaRef {
    SchemaRef {
        id: format!("urn:test:{name}"),
        sha256: HASH.to_owned(),
    }
}

fn descriptor(
    project: &str,
    adapter: &str,
    capability: &str,
    access: CapabilityAccess,
) -> AdapterDescriptor {
    AdapterDescriptor {
        project_id: project.to_owned(),
        adapter_id: adapter.to_owned(),
        contract_version: ContractVersion { major: 1, minor: 0 },
        implementation_version: "1.0.0".to_owned(),
        capabilities: vec![CapabilityDescriptor {
            id: capability.to_owned(),
            request_schema: schema("request"),
            response_schema: schema("response"),
            access,
        }],
        deterministic: true,
        call_limits: CallLimits {
            max_work: 50,
            max_results: 50,
        },
    }
}

fn request<P>(descriptor: &AdapterDescriptor, payload: P) -> AdapterRequest<P> {
    let capability = &descriptor.capabilities[0];
    AdapterRequest {
        request_id: "req-1".to_owned(),
        selection: AdapterSelection {
            project_id: descriptor.project_id.clone(),
            adapter_id: descriptor.adapter_id.clone(),
            contract_version: descriptor.contract_version,
            implementation_version: descriptor.implementation_version.clone(),
            capability_id: capability.id.clone(),
            request_schema: capability.request_schema.clone(),
            response_schema: capability.response_schema.clone(),
        },
        snapshot_revision: "r0".to_owned(),
        call_limits: CallLimits {
            max_work: 10,
            max_results: 10,
        },
        payload,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Ledger {
    balance: i64,
    journal: Vec<i64>,
}

struct LedgerTxn(Box<Ledger>);

impl Deref for LedgerTxn {
    type Target = Ledger;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for LedgerTxn {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

struct LedgerHost {
    state: Ledger,
    revision: String,
    begun: usize,
    committed: usize,
    rolled_back: usize,
    fail_commit: bool,
}

impl LedgerHost {
    fn new() -> Self {
        Self {
            state: Ledger {
                balance: 5,
                journal: vec![],
            },
            revision: "r0".to_owned(),
            begun: 0,
            committed: 0,
            rolled_back: 0,
            fail_commit: false,
        }
    }
}

impl AdapterHost<Ledger> for LedgerHost {
    type Transaction = LedgerTxn;

    fn revision(&self) -> &str {
        &self.revision
    }

    fn read_state(&self) -> &Ledger {
        &self.state
    }

    fn begin_transaction(&mut self) -> Result<Self::Transaction, AdapterError> {
        self.begun += 1;
        Ok(LedgerTxn(Box::new(self.state.clone())))
    }

    fn commit_transaction(
        &mut self,
        transaction: Self::Transaction,
        expected_revision: &str,
    ) -> Result<CommittedRevision, (AdapterError, Self::Transaction)> {
        if self.fail_commit || self.revision != expected_revision {
            return Err((
                AdapterError::new(
                    AdapterErrorKind::StaleRevision,
                    "ledger_commit_rejected",
                    "ledger commit rejected at host boundary",
                ),
                transaction,
            ));
        }
        let new_revision = CommittedRevision::new("r1").unwrap();
        self.state = *transaction.0;
        self.revision = "r1".to_owned();
        self.committed += 1;
        Ok(new_revision)
    }

    fn rollback_transaction(&mut self, _transaction: Self::Transaction) {
        self.rolled_back += 1;
    }
}

#[derive(Clone, Copy)]
struct Delta(i64);

struct LedgerObject {
    descriptor: AdapterDescriptor,
    fail_result_validation: bool,
}

impl LedgerObject {
    fn new() -> Self {
        Self {
            descriptor: descriptor(
                "ledger",
                "counter",
                "execute",
                CapabilityAccess::Transactional,
            ),
            fail_result_validation: false,
        }
    }
}

impl AdapterObject<Ledger, Delta, i64> for LedgerObject {
    fn descriptor(&self) -> &AdapterDescriptor {
        &self.descriptor
    }

    fn invoke_write(
        &self,
        working: &mut Ledger,
        request: &AdapterRequest<Delta>,
        meter: &mut CallMeter<'_>,
    ) -> Result<AdapterOutput<i64>, AdapterError> {
        meter.consume_work(1)?;
        working.balance += request.payload.0;
        working.journal.push(request.payload.0);
        meter.checkpoint()?;
        meter.consume_results(1)?;
        Ok(AdapterOutput::value(working.balance))
    }

    fn validate_output(&self, _capability_id: &str, result: &i64) -> Result<u64, AdapterError> {
        if self.fail_result_validation || *result < 0 {
            Err(AdapterError::execution_failed(
                "ledger_result_invalid",
                "ledger result failed project schema validation",
            ))
        } else {
            Ok(1)
        }
    }
}

fn ledger_registry<T>(object: T) -> AdapterRegistry<Ledger, Delta, i64>
where
    T: AdapterObject<Ledger, Delta, i64> + 'static,
{
    let mut registry = AdapterRegistry::new();
    registry.register(object).unwrap();
    registry.seal();
    registry
}

#[test]
fn ledger_transaction_commits_only_after_validation() {
    let object = LedgerObject::new();
    let request = request(&object.descriptor, Delta(3));
    let registry = ledger_registry(object);
    let mut host = LedgerHost::new();
    let response = registry
        .invoke(
            &mut host,
            &request,
            &InvocationControl::unlimited_time(&NeverCancelled),
        )
        .unwrap();
    assert_eq!(response.result, 8);
    assert_eq!(response.snapshot_revision, "r1");
    assert_eq!(response.response_schema, schema("response"));
    assert_eq!(host.state.journal, vec![3]);
    assert_eq!((host.begun, host.committed, host.rolled_back), (1, 1, 0));
}

#[test]
fn ledger_failures_preserve_original_state_and_revision() {
    let object = LedgerObject::new();
    let mut request = request(&object.descriptor, Delta(4));
    let registry = ledger_registry(object);
    let mut host = LedgerHost::new();
    request.call_limits.max_results = 0;
    let error = registry
        .invoke(
            &mut host,
            &request,
            &InvocationControl::unlimited_time(&NeverCancelled),
        )
        .unwrap_err();
    assert_eq!(error.kind, AdapterErrorKind::LimitExceeded);
    assert_eq!(error.code, "result_limit_exceeded");
    assert_eq!(host.state.balance, 5);
    assert_eq!(host.revision, "r0");
    assert_eq!((host.begun, host.committed, host.rolled_back), (1, 0, 1));

    host.fail_commit = true;
    request.call_limits.max_results = 10;
    let error = registry
        .invoke(
            &mut host,
            &request,
            &InvocationControl::unlimited_time(&NeverCancelled),
        )
        .unwrap_err();
    assert_eq!(error.code, "ledger_commit_rejected");
    assert_eq!(host.state.balance, 5);
    assert_eq!(host.revision, "r0");
    assert_eq!((host.begun, host.committed, host.rolled_back), (2, 0, 2));

    request.snapshot_revision = "stale".to_owned();
    let error = registry
        .invoke(
            &mut host,
            &request,
            &InvocationControl::unlimited_time(&NeverCancelled),
        )
        .unwrap_err();
    assert_eq!(error.kind, AdapterErrorKind::StaleRevision);
    assert_eq!((host.begun, host.committed, host.rolled_back), (2, 0, 2));
}

#[test]
fn ledger_result_validation_and_work_limit_rollback() {
    let mut object = LedgerObject::new();
    let invalid_result_request = request(&object.descriptor, Delta(3));
    object.fail_result_validation = true;
    let registry = ledger_registry(object);
    let mut host = LedgerHost::new();
    let error = registry
        .invoke(
            &mut host,
            &invalid_result_request,
            &InvocationControl::unlimited_time(&NeverCancelled),
        )
        .unwrap_err();
    assert_eq!(error.code, "ledger_result_invalid");
    assert_eq!(host.state.balance, 5);
    assert_eq!(host.rolled_back, 1);

    let object = LedgerObject::new();
    let mut request = request(&object.descriptor, Delta(3));
    request.call_limits.max_work = 1;
    let registry = ledger_registry(object);
    let cancelled = AtomicBool::new(false);
    let deadline = Instant::now() - Duration::from_millis(1);
    let error = registry
        .invoke(
            &mut host,
            &request,
            &InvocationControl {
                cancellation: &FlagCancellation(&cancelled),
                deadline: Some(deadline),
            },
        )
        .unwrap_err();
    assert_eq!(error.code, "deadline_exceeded");
    assert_eq!(host.state.balance, 5);
}

struct CancelAfterMutation {
    descriptor: AdapterDescriptor,
    flag: std::sync::Arc<AtomicBool>,
}

impl AdapterObject<Ledger, Delta, i64> for CancelAfterMutation {
    fn descriptor(&self) -> &AdapterDescriptor {
        &self.descriptor
    }

    fn invoke_write(
        &self,
        working: &mut Ledger,
        request: &AdapterRequest<Delta>,
        meter: &mut CallMeter<'_>,
    ) -> Result<AdapterOutput<i64>, AdapterError> {
        meter.consume_work(1)?;
        working.balance += request.payload.0;
        working.journal.push(request.payload.0);
        meter.consume_results(1)?;
        self.flag.store(true, Ordering::Relaxed);
        Ok(AdapterOutput::value(working.balance))
    }

    fn validate_output(&self, _capability_id: &str, _result: &i64) -> Result<u64, AdapterError> {
        Ok(1)
    }
}

#[test]
fn cancellation_after_work_rolls_back_state_rng_equivalent_journal() {
    let flag = std::sync::Arc::new(AtomicBool::new(false));
    let object = CancelAfterMutation {
        descriptor: descriptor(
            "ledger",
            "counter",
            "execute",
            CapabilityAccess::Transactional,
        ),
        flag: flag.clone(),
    };
    let request = request(&object.descriptor, Delta(7));
    let registry = ledger_registry(object);
    let mut host = LedgerHost::new();
    let error = registry
        .invoke(
            &mut host,
            &request,
            &InvocationControl::unlimited_time(&FlagCancellation(&flag)),
        )
        .unwrap_err();
    assert_eq!(error.kind, AdapterErrorKind::Cancelled);
    assert_eq!(host.state.balance, 5);
    assert!(host.state.journal.is_empty());
    assert_eq!((host.begun, host.committed, host.rolled_back), (1, 0, 1));
}

struct FlagCancellation<'a>(&'a AtomicBool);

impl Cancellation for FlagCancellation<'_> {
    fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

#[derive(Clone, Copy, Deserialize, Serialize)]
struct CatalogQuery {
    start: usize,
    page_size: usize,
}

struct Catalog {
    items: Vec<String>,
}

struct CatalogHost {
    state: Catalog,
    transaction_begun: bool,
}

impl AdapterHost<Catalog> for CatalogHost {
    type Transaction = Box<Catalog>;

    fn revision(&self) -> &str {
        "r0"
    }

    fn read_state(&self) -> &Catalog {
        &self.state
    }

    fn begin_transaction(&mut self) -> Result<Self::Transaction, AdapterError> {
        self.transaction_begun = true;
        Err(AdapterError::execution_failed(
            "read_used_transaction",
            "catalog query must not begin a transaction",
        ))
    }

    fn commit_transaction(
        &mut self,
        transaction: Self::Transaction,
        _expected_revision: &str,
    ) -> Result<CommittedRevision, (AdapterError, Self::Transaction)> {
        Err((
            AdapterError::execution_failed("read_used_commit", "catalog query must not commit"),
            transaction,
        ))
    }

    fn rollback_transaction(&mut self, _transaction: Self::Transaction) {
        panic!("catalog query must not roll back")
    }
}

struct CatalogObject {
    descriptor: AdapterDescriptor,
}

impl CatalogObject {
    fn new() -> Self {
        Self {
            descriptor: descriptor(
                "inventory",
                "catalog",
                "enumerate",
                CapabilityAccess::ReadOnly,
            ),
        }
    }
}

impl AdapterObject<Catalog, CatalogQuery, Vec<String>> for CatalogObject {
    fn descriptor(&self) -> &AdapterDescriptor {
        &self.descriptor
    }

    fn invoke_read(
        &self,
        state: &Catalog,
        request: &AdapterRequest<CatalogQuery>,
        meter: &mut CallMeter<'_>,
    ) -> Result<AdapterOutput<Vec<String>>, AdapterError> {
        let query = request.payload;
        if query.start > state.items.len() || query.page_size == 0 {
            return Err(AdapterError::invalid_input(
                "catalog_query_invalid",
                "start is out of range or pageSize is zero",
            ));
        }
        let end = state
            .items
            .len()
            .min(query.start.saturating_add(query.page_size));
        let count = u64::try_from(end - query.start).unwrap();
        meter.consume_examined(count)?;
        meter.consume_results(count)?;
        Ok(AdapterOutput {
            value: state.items[query.start..end].to_vec(),
            page: Some(PageInfo {
                examined: count,
                cursor: (end < state.items.len()).then(|| end.to_string()),
                exhausted: end == state.items.len(),
            }),
            diagnostics: vec![],
        })
    }

    fn validate_output(
        &self,
        _capability_id: &str,
        result: &Vec<String>,
    ) -> Result<u64, AdapterError> {
        Ok(u64::try_from(result.len()).unwrap())
    }
}

#[test]
fn catalog_pages_are_ordered_and_read_does_not_begin_transaction() {
    let object = CatalogObject::new();
    let mut request = request(
        &object.descriptor,
        CatalogQuery {
            start: 0,
            page_size: 2,
        },
    );
    let mut registry = AdapterRegistry::new();
    registry.register(object).unwrap();
    registry.seal();
    let mut host = CatalogHost {
        state: Catalog {
            items: ["alpha", "beta", "gamma"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
        },
        transaction_begun: false,
    };
    let first = registry
        .invoke(
            &mut host,
            &request,
            &InvocationControl::unlimited_time(&NeverCancelled),
        )
        .unwrap();
    assert_eq!(first.result, vec!["alpha", "beta"]);
    let first_wire = serde_json::to_value(&first).unwrap();
    let first_again: AdapterResponse<Vec<String>> = serde_json::from_value(first_wire).unwrap();
    assert_eq!(first_again, first);
    assert_eq!(first.page.unwrap().cursor.as_deref(), Some("2"));
    request.payload.start = 2;
    let last = registry
        .invoke(
            &mut host,
            &request,
            &InvocationControl::unlimited_time(&NeverCancelled),
        )
        .unwrap();
    assert_eq!(last.result, vec!["gamma"]);
    assert!(last.page.unwrap().exhausted);
    assert!(!host.transaction_begun);
}

#[test]
fn two_project_descriptors_and_requests_round_trip_through_common_wire() {
    for descriptor in [
        LedgerObject::new().descriptor,
        CatalogObject::new().descriptor,
    ] {
        let wire = serde_json::to_value(&descriptor).unwrap();
        let decoded: AdapterDescriptor = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(decoded, descriptor);
        assert_eq!(wire["contractVersion"]["major"], 1);
        assert_eq!(wire["capabilities"][0]["requestSchema"]["sha256"], HASH);
    }
    let catalog = CatalogObject::new();
    let request = request(
        &catalog.descriptor,
        CatalogQuery {
            start: 1,
            page_size: 2,
        },
    );
    let wire = serde_json::to_value(&request).unwrap();
    let decoded: AdapterRequest<CatalogQuery> = serde_json::from_value(wire).unwrap();
    assert_eq!(decoded.selection.project_id, "inventory");
    assert_eq!(decoded.payload.start, 1);
}

#[test]
fn registry_fails_closed_on_duplicate_version_schema_and_capability() {
    let object = LedgerObject::new();
    let mut request = request(&object.descriptor, Delta(1));
    let mut registry = AdapterRegistry::new();
    registry.register(object).unwrap();
    let duplicate = registry.register(LedgerObject::new()).unwrap_err();
    assert_eq!(duplicate.code, "duplicate_adapter");
    registry.seal();
    let mut host = LedgerHost::new();

    request.selection.capability_id = "missing".to_owned();
    let error = registry
        .invoke(
            &mut host,
            &request,
            &InvocationControl::unlimited_time(&NeverCancelled),
        )
        .unwrap_err();
    assert_eq!(error.code, "capability_unregistered");
    request.selection.capability_id = "execute".to_owned();

    request.selection.contract_version.minor = 1;
    let error = registry
        .invoke(
            &mut host,
            &request,
            &InvocationControl::unlimited_time(&NeverCancelled),
        )
        .unwrap_err();
    assert_eq!(error.code, "contract_version_mismatch");
    request.selection.contract_version.minor = 0;

    request.selection.request_schema.id = "urn:test:other".to_owned();
    let error = registry
        .invoke(
            &mut host,
            &request,
            &InvocationControl::unlimited_time(&NeverCancelled),
        )
        .unwrap_err();
    assert_eq!(error.code, "schema_mismatch");
    assert_eq!(host.begun, 0);
}

#[test]
fn request_wire_selection_is_flat_and_not_a_rust_layout_artifact() {
    let object = LedgerObject::new();
    let request = request(&object.descriptor, 2_i64);
    let mut wire = serde_json::to_value(&request).unwrap();
    assert_eq!(wire["projectId"], "ledger");
    assert_eq!(wire["capabilityId"], "execute");
    assert_eq!(wire["limits"]["maxWork"], 10);
    assert!(wire.get("selection").is_none());
    assert!(serde_json::from_value::<AdapterRequest<i64>>(wire.clone()).is_ok());
    wire["unexpectedField"] = serde_json::json!(true);
    assert!(serde_json::from_value::<AdapterRequest<i64>>(wire).is_err());
}

#[test]
fn outcome_wire_distinguishes_success_and_failure() {
    let object = LedgerObject::new();
    let request = request(&object.descriptor, Delta(2));
    let registry = ledger_registry(object);
    let mut host = LedgerHost::new();
    let success: AdapterOutcome<i64> = registry
        .invoke(
            &mut host,
            &request,
            &InvocationControl::unlimited_time(&NeverCancelled),
        )
        .into();
    let success_wire = serde_json::to_value(&success).unwrap();
    assert_eq!(success_wire["ok"], true);
    assert_eq!(success_wire["response"]["result"], 7);
    assert!(serde_json::from_value::<AdapterOutcome<i64>>(success_wire).is_ok());

    let mut stale = request;
    stale.snapshot_revision = "r0".to_owned();
    let failure: AdapterOutcome<i64> = registry
        .invoke(
            &mut host,
            &stale,
            &InvocationControl::unlimited_time(&NeverCancelled),
        )
        .into();
    let mut failure_wire = serde_json::to_value(&failure).unwrap();
    assert_eq!(failure_wire["ok"], false);
    assert_eq!(failure_wire["error"]["kind"], "stale_revision");
    assert_eq!(failure_wire["error"]["adapterId"], "counter");
    assert!(serde_json::from_value::<AdapterOutcome<i64>>(failure_wire.clone()).is_ok());
    failure_wire["ok"] = serde_json::json!(true);
    assert!(serde_json::from_value::<AdapterOutcome<i64>>(failure_wire).is_err());
}
