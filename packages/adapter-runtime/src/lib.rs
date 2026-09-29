//! Typed, project-independent adapter registration and invocation.
//!
//! The host owns state, transactions, revision, RNG, and any external effects.
//! Read-only calls borrow the host state. Transactional calls use a host-owned
//! working transaction, which is committed only after all response and budget
//! checks pass. The runtime never serializes state or downcasts it.

use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::ops::DerefMut;
use std::sync::Arc;
use std::time::Instant;

use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The full version is checked at invocation; the major version participates
/// in the registry key. Compatibility between minor versions is not inferred.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContractVersion {
    pub major: u32,
    pub minor: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SchemaRef {
    pub id: String,
    pub sha256: String,
}

impl SchemaRef {
    fn validate(&self) -> Result<(), AdapterError> {
        if self.id.trim().is_empty()
            || self.sha256.len() != 64
            || !self
                .sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(AdapterError::invalid_input(
                "invalid_schema_reference",
                "schema id must be nonempty and sha256 must be 64 lowercase hex characters",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityAccess {
    ReadOnly,
    Transactional,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityDescriptor {
    pub id: String,
    pub request_schema: SchemaRef,
    pub response_schema: SchemaRef,
    pub access: CapabilityAccess,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CallLimits {
    pub max_work: u64,
    pub max_results: u64,
}

impl CallLimits {
    fn validate(self) -> Result<(), AdapterError> {
        if self.max_work == 0 {
            return Err(AdapterError::invalid_input(
                "invalid_call_limits",
                "maxWork must be greater than zero",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdapterDescriptor {
    pub project_id: String,
    pub adapter_id: String,
    pub contract_version: ContractVersion,
    pub implementation_version: String,
    pub capabilities: Vec<CapabilityDescriptor>,
    pub deterministic: bool,
    pub call_limits: CallLimits,
}

impl AdapterDescriptor {
    fn validate(&self) -> Result<(), AdapterError> {
        for (name, value) in [
            ("projectId", &self.project_id),
            ("adapterId", &self.adapter_id),
            ("implementationVersion", &self.implementation_version),
        ] {
            if value.trim().is_empty() {
                return Err(AdapterError::invalid_input(
                    "invalid_descriptor",
                    format!("{name} must be nonempty"),
                ));
            }
        }
        if self.contract_version.major == 0 {
            return Err(AdapterError::invalid_input(
                "invalid_descriptor",
                "contractVersion.major must be greater than zero",
            ));
        }
        self.call_limits.validate()?;
        if self.capabilities.is_empty() {
            return Err(AdapterError::invalid_input(
                "invalid_descriptor",
                "at least one capability is required",
            ));
        }
        let mut ids = std::collections::HashSet::new();
        for capability in &self.capabilities {
            if capability.id.trim().is_empty() || !ids.insert(&capability.id) {
                return Err(AdapterError::invalid_input(
                    "invalid_descriptor",
                    format!("empty or duplicate capability id: {}", capability.id),
                ));
            }
            capability.request_schema.validate()?;
            capability.response_schema.validate()?;
        }
        Ok(())
    }
}

/// The caller must name an exact registered implementation and schema pair.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdapterSelection {
    pub project_id: String,
    pub adapter_id: String,
    pub contract_version: ContractVersion,
    pub implementation_version: String,
    pub capability_id: String,
    pub request_schema: SchemaRef,
    pub response_schema: SchemaRef,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdapterRequest<P> {
    pub request_id: String,
    #[serde(flatten)]
    pub selection: AdapterSelection,
    pub snapshot_revision: String,
    #[serde(rename = "limits")]
    pub call_limits: CallLimits,
    pub payload: P,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PageInfo {
    pub examined: u64,
    pub cursor: Option<String>,
    pub exhausted: bool,
}

impl PageInfo {
    fn validate(&self, metered_examined: u64) -> Result<(), AdapterError> {
        if self.examined != metered_examined {
            return Err(AdapterError::execution_failed(
                "invalid_page_accounting",
                format!(
                    "page examined {} differs from metered examined {}",
                    self.examined, metered_examined
                ),
            ));
        }
        if self.exhausted == self.cursor.is_some() {
            return Err(AdapterError::execution_failed(
                "invalid_page_cursor",
                "an unfinished page requires a cursor and an exhausted page must omit it",
            ));
        }
        if self.cursor.as_deref().is_some_and(str::is_empty) {
            return Err(AdapterError::execution_failed(
                "invalid_page_cursor",
                "an unfinished page cursor must be nonempty",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSeverity {
    Info,
    Warning,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdapterDiagnostic {
    pub severity: DiagnosticSeverity,
    pub code: String,
    pub message: String,
}

/// The object produces a typed value and explicit diagnostics. The registry
/// supplies the response identity, revision and validated schema reference.
#[derive(Clone, Debug, PartialEq)]
pub struct AdapterOutput<R> {
    pub value: R,
    pub page: Option<PageInfo>,
    pub diagnostics: Vec<AdapterDiagnostic>,
}

impl<R> AdapterOutput<R> {
    pub fn value(value: R) -> Self {
        Self {
            value,
            page: None,
            diagnostics: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdapterResponse<R> {
    pub request_id: String,
    pub snapshot_revision: String,
    pub response_schema: SchemaRef,
    pub result: R,
    pub page: Option<PageInfo>,
    pub diagnostics: Vec<AdapterDiagnostic>,
}

/// The wire discriminants are JSON booleans. These private marker types reject
/// mismatched `ok` values during decoding instead of accepting a misleading
/// success or failure envelope.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SuccessMarker;

impl Serialize for SuccessMarker {
    fn serialize<T: Serializer>(&self, serializer: T) -> Result<T::Ok, T::Error> {
        serializer.serialize_bool(true)
    }
}

impl<'de> Deserialize<'de> for SuccessMarker {
    fn deserialize<T: Deserializer<'de>>(deserializer: T) -> Result<Self, T::Error> {
        if bool::deserialize(deserializer)? {
            Ok(Self)
        } else {
            Err(T::Error::custom("success outcome requires ok: true"))
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FailureMarker;

impl Serialize for FailureMarker {
    fn serialize<T: Serializer>(&self, serializer: T) -> Result<T::Ok, T::Error> {
        serializer.serialize_bool(false)
    }
}

impl<'de> Deserialize<'de> for FailureMarker {
    fn deserialize<T: Deserializer<'de>>(deserializer: T) -> Result<Self, T::Error> {
        if bool::deserialize(deserializer)? {
            Err(T::Error::custom("failure outcome requires ok: false"))
        } else {
            Ok(Self)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdapterSuccess<R> {
    pub ok: SuccessMarker,
    pub response: AdapterResponse<R>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdapterFailure {
    pub ok: FailureMarker,
    pub error: AdapterError,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AdapterOutcome<R> {
    Success(AdapterSuccess<R>),
    Failure(AdapterFailure),
}

impl<R> From<Result<AdapterResponse<R>, AdapterError>> for AdapterOutcome<R> {
    fn from(result: Result<AdapterResponse<R>, AdapterError>) -> Self {
        match result {
            Ok(response) => Self::Success(AdapterSuccess {
                ok: SuccessMarker,
                response,
            }),
            Err(error) => Self::Failure(AdapterFailure {
                ok: FailureMarker,
                error,
            }),
        }
    }
}

/// A commit revision is validated before the host publishes its transaction.
/// A host must construct it before mutating the live state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommittedRevision(String);

impl CommittedRevision {
    pub fn new(value: impl Into<String>) -> Result<Self, AdapterError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(AdapterError::invalid_input(
                "invalid_committed_revision",
                "committed revision must be nonempty",
            ));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> String {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterErrorKind {
    Unsupported,
    InvalidInput,
    StaleRevision,
    LimitExceeded,
    Cancelled,
    ExecutionFailed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdapterError {
    pub kind: AdapterErrorKind,
    pub code: String,
    pub message: String,
    pub adapter_id: Option<String>,
    pub capability_id: Option<String>,
}

impl AdapterError {
    pub fn new(
        kind: AdapterErrorKind,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            code: code.into(),
            message: message.into(),
            adapter_id: None,
            capability_id: None,
        }
    }

    pub fn unsupported(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(AdapterErrorKind::Unsupported, code, message)
    }

    pub fn invalid_input(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(AdapterErrorKind::InvalidInput, code, message)
    }

    pub fn execution_failed(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(AdapterErrorKind::ExecutionFailed, code, message)
    }

    fn with_context(mut self, selection: &AdapterSelection) -> Self {
        if self.code.trim().is_empty() || self.message.trim().is_empty() {
            let invalid_field = if self.code.trim().is_empty() {
                "code"
            } else {
                "message"
            };
            self = Self::execution_failed(
                "invalid_adapter_error",
                format!("adapter returned an error with empty {invalid_field}"),
            );
        }
        self.adapter_id = Some(selection.adapter_id.clone());
        self.capability_id = Some(selection.capability_id.clone());
        self
    }
}

impl fmt::Display for AdapterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{:?} [{}]: {}",
            self.kind, self.code, self.message
        )
    }
}

impl Error for AdapterError {}

pub trait Cancellation: Send + Sync {
    fn is_cancelled(&self) -> bool;
}

pub struct NeverCancelled;

impl Cancellation for NeverCancelled {
    fn is_cancelled(&self) -> bool {
        false
    }
}

/// The monotonic deadline is supplied by the host, not decoded from untrusted
/// wall-clock text. Each nested operation uses the same meter.
pub struct InvocationControl<'a> {
    pub cancellation: &'a dyn Cancellation,
    pub deadline: Option<Instant>,
}

impl<'a> InvocationControl<'a> {
    pub fn unlimited_time(cancellation: &'a dyn Cancellation) -> Self {
        Self {
            cancellation,
            deadline: None,
        }
    }
}

pub struct CallMeter<'a> {
    limits: CallLimits,
    control: &'a InvocationControl<'a>,
    work: u64,
    results: u64,
    examined: u64,
}

impl CallMeter<'_> {
    pub fn checkpoint(&self) -> Result<(), AdapterError> {
        if self.control.cancellation.is_cancelled() {
            return Err(AdapterError::new(
                AdapterErrorKind::Cancelled,
                "cancelled",
                "adapter invocation was cancelled",
            ));
        }
        if self
            .control
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return Err(AdapterError::new(
                AdapterErrorKind::LimitExceeded,
                "deadline_exceeded",
                "adapter invocation deadline was exceeded",
            ));
        }
        Ok(())
    }

    pub fn consume_work(&mut self, amount: u64) -> Result<(), AdapterError> {
        self.checkpoint()?;
        let next = self.work.checked_add(amount).ok_or_else(|| {
            AdapterError::new(
                AdapterErrorKind::LimitExceeded,
                "work_limit_exceeded",
                "adapter work counter overflowed",
            )
        })?;
        if next > self.limits.max_work {
            return Err(AdapterError::new(
                AdapterErrorKind::LimitExceeded,
                "work_limit_exceeded",
                format!("adapter work {next} exceeds limit {}", self.limits.max_work),
            ));
        }
        self.work = next;
        Ok(())
    }

    pub fn consume_results(&mut self, amount: u64) -> Result<(), AdapterError> {
        self.checkpoint()?;
        let next = self.results.checked_add(amount).ok_or_else(|| {
            AdapterError::new(
                AdapterErrorKind::LimitExceeded,
                "result_limit_exceeded",
                "adapter result counter overflowed",
            )
        })?;
        if next > self.limits.max_results {
            return Err(AdapterError::new(
                AdapterErrorKind::LimitExceeded,
                "result_limit_exceeded",
                format!(
                    "adapter results {next} exceeds limit {}",
                    self.limits.max_results
                ),
            ));
        }
        self.results = next;
        Ok(())
    }

    /// Count source candidates examined for a paged enumeration. This also
    /// consumes work and is checked against `PageInfo.examined` at the boundary.
    pub fn consume_examined(&mut self, amount: u64) -> Result<(), AdapterError> {
        let next = self.examined.checked_add(amount).ok_or_else(|| {
            AdapterError::new(
                AdapterErrorKind::LimitExceeded,
                "examined_counter_overflow",
                "adapter examined counter overflowed",
            )
        })?;
        self.consume_work(amount)?;
        self.examined = next;
        Ok(())
    }

    pub fn work(&self) -> u64 {
        self.work
    }

    pub fn results(&self) -> u64 {
        self.results
    }

    pub fn examined(&self) -> u64 {
        self.examined
    }
}

/// A project object chooses which of the two entrypoints it implements. The
/// registry selects exactly one using the capability's declared access mode.
/// `validate_output` must check the project's result schema and return the
/// true number of emitted results, which must match the results charged to the
/// common meter before a transaction commits.
pub trait AdapterObject<S, P, R>: Send + Sync {
    fn descriptor(&self) -> &AdapterDescriptor;

    fn invoke_read(
        &self,
        _state: &S,
        _request: &AdapterRequest<P>,
        _meter: &mut CallMeter<'_>,
    ) -> Result<AdapterOutput<R>, AdapterError> {
        Err(AdapterError::unsupported(
            "read_not_implemented",
            "read-only capability has no implementation",
        ))
    }

    fn invoke_write(
        &self,
        _working: &mut S,
        _request: &AdapterRequest<P>,
        _meter: &mut CallMeter<'_>,
    ) -> Result<AdapterOutput<R>, AdapterError> {
        Err(AdapterError::unsupported(
            "write_not_implemented",
            "transactional capability has no implementation",
        ))
    }

    fn validate_output(&self, capability_id: &str, result: &R) -> Result<u64, AdapterError>;
}

/// Hosts must keep `begin_transaction` changes isolated. `commit_transaction`
/// must be atomic and return the still-owned transaction on failure; the
/// registry then rolls it back. Rollback is also called after object errors,
/// cancellation, budget exhaustion, or response validation errors.
pub trait AdapterHost<S> {
    type Transaction: DerefMut<Target = S>;

    fn revision(&self) -> &str;
    fn read_state(&self) -> &S;
    fn begin_transaction(&mut self) -> Result<Self::Transaction, AdapterError>;
    fn commit_transaction(
        &mut self,
        transaction: Self::Transaction,
        expected_revision: &str,
    ) -> Result<CommittedRevision, (AdapterError, Self::Transaction)>;
    fn rollback_transaction(&mut self, transaction: Self::Transaction);
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct RegistryKey {
    project_id: String,
    adapter_id: String,
    contract_major: u32,
    implementation_version: String,
}

impl RegistryKey {
    fn from_descriptor(descriptor: &AdapterDescriptor) -> Self {
        Self {
            project_id: descriptor.project_id.clone(),
            adapter_id: descriptor.adapter_id.clone(),
            contract_major: descriptor.contract_version.major,
            implementation_version: descriptor.implementation_version.clone(),
        }
    }

    fn from_selection(selection: &AdapterSelection) -> Self {
        Self {
            project_id: selection.project_id.clone(),
            adapter_id: selection.adapter_id.clone(),
            contract_major: selection.contract_version.major,
            implementation_version: selection.implementation_version.clone(),
        }
    }
}

/// Registration is allowed only before `seal`. No version or implementation
/// fallback is performed during lookup.
pub struct AdapterRegistry<S, P, R> {
    objects: HashMap<RegistryKey, Arc<dyn AdapterObject<S, P, R>>>,
    sealed: bool,
}

impl<S, P, R> Default for AdapterRegistry<S, P, R> {
    fn default() -> Self {
        Self {
            objects: HashMap::new(),
            sealed: false,
        }
    }
}

impl<S, P, R> AdapterRegistry<S, P, R> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<T>(&mut self, object: T) -> Result<(), AdapterError>
    where
        T: AdapterObject<S, P, R> + 'static,
    {
        if self.sealed {
            return Err(AdapterError::invalid_input(
                "registry_sealed",
                "adapter registration is closed for this session",
            ));
        }
        let descriptor = object.descriptor();
        descriptor.validate()?;
        let key = RegistryKey::from_descriptor(descriptor);
        if self.objects.contains_key(&key) {
            return Err(AdapterError::invalid_input(
                "duplicate_adapter",
                format!(
                    "adapter {} / {} / contract major {} / implementation {} is already registered",
                    key.project_id, key.adapter_id, key.contract_major, key.implementation_version
                ),
            ));
        }
        self.objects.insert(key, Arc::new(object));
        Ok(())
    }

    pub fn seal(&mut self) {
        self.sealed = true;
    }

    pub fn is_sealed(&self) -> bool {
        self.sealed
    }

    pub fn descriptors(&self) -> Vec<&AdapterDescriptor> {
        let mut descriptors: Vec<_> = self
            .objects
            .values()
            .map(|object| object.descriptor())
            .collect();
        descriptors.sort_by(|left, right| {
            (
                &left.project_id,
                &left.adapter_id,
                left.contract_version.major,
                &left.implementation_version,
            )
                .cmp(&(
                    &right.project_id,
                    &right.adapter_id,
                    right.contract_version.major,
                    &right.implementation_version,
                ))
        });
        descriptors
    }

    pub fn invoke<H>(
        &self,
        host: &mut H,
        request: &AdapterRequest<P>,
        control: &InvocationControl<'_>,
    ) -> Result<AdapterResponse<R>, AdapterError>
    where
        H: AdapterHost<S>,
    {
        self.invoke_inner(host, request, control)
            .map_err(|error| error.with_context(&request.selection))
    }

    fn invoke_inner<H>(
        &self,
        host: &mut H,
        request: &AdapterRequest<P>,
        control: &InvocationControl<'_>,
    ) -> Result<AdapterResponse<R>, AdapterError>
    where
        H: AdapterHost<S>,
    {
        if !self.sealed {
            return Err(AdapterError::invalid_input(
                "registry_unsealed",
                "seal the static adapter registry before invocation",
            ));
        }
        if request.request_id.trim().is_empty() || request.snapshot_revision.trim().is_empty() {
            return Err(AdapterError::invalid_input(
                "invalid_request",
                "requestId and snapshotRevision must be nonempty",
            ));
        }
        request.call_limits.validate()?;
        request.selection.request_schema.validate()?;
        request.selection.response_schema.validate()?;
        let key = RegistryKey::from_selection(&request.selection);
        let object = self.objects.get(&key).ok_or_else(|| {
            AdapterError::unsupported(
                "adapter_unregistered",
                format!(
                    "no adapter for project {} / adapter {} / contract major {} / implementation {}",
                    key.project_id, key.adapter_id, key.contract_major, key.implementation_version
                ),
            )
        })?;
        let descriptor = object.descriptor();
        if descriptor.contract_version != request.selection.contract_version {
            return Err(AdapterError::unsupported(
                "contract_version_mismatch",
                "requested contract version differs from registered descriptor",
            ));
        }
        let capability = descriptor
            .capabilities
            .iter()
            .find(|candidate| candidate.id == request.selection.capability_id)
            .ok_or_else(|| {
                AdapterError::unsupported(
                    "capability_unregistered",
                    format!(
                        "adapter {} does not expose capability {}",
                        descriptor.adapter_id, request.selection.capability_id
                    ),
                )
            })?;
        if capability.request_schema != request.selection.request_schema
            || capability.response_schema != request.selection.response_schema
        {
            return Err(AdapterError::unsupported(
                "schema_mismatch",
                format!(
                    "capability {} request/response schema id or hash differs from descriptor",
                    capability.id
                ),
            ));
        }
        if request.call_limits.max_work > descriptor.call_limits.max_work
            || request.call_limits.max_results > descriptor.call_limits.max_results
        {
            return Err(AdapterError::new(
                AdapterErrorKind::LimitExceeded,
                "requested_limit_exceeds_descriptor",
                "requested work or result limit exceeds adapter descriptor maximum",
            ));
        }
        let before_revision = host.revision().to_owned();
        if request.snapshot_revision != before_revision {
            return Err(AdapterError::new(
                AdapterErrorKind::StaleRevision,
                "stale_revision",
                format!(
                    "requested revision {} differs from host revision {}",
                    request.snapshot_revision, before_revision
                ),
            ));
        }
        let mut meter = CallMeter {
            limits: request.call_limits,
            control,
            work: 0,
            results: 0,
            examined: 0,
        };
        meter.checkpoint()?;
        match capability.access {
            CapabilityAccess::ReadOnly => {
                let output = object.invoke_read(host.read_state(), request, &mut meter)?;
                validate_output(object.as_ref(), capability, &output, &mut meter)?;
                if host.revision() != before_revision {
                    return Err(AdapterError::new(
                        AdapterErrorKind::StaleRevision,
                        "revision_changed_during_read",
                        "host revision changed during a read-only call",
                    ));
                }
                Ok(response_from_output(
                    request,
                    capability,
                    before_revision,
                    output,
                ))
            }
            CapabilityAccess::Transactional => {
                let mut transaction = Some(host.begin_transaction()?);
                let output = object.invoke_write(
                    transaction
                        .as_mut()
                        .expect("transaction exists")
                        .deref_mut(),
                    request,
                    &mut meter,
                );
                let output = match output.and_then(|output| {
                    validate_output(object.as_ref(), capability, &output, &mut meter)?;
                    Ok(output)
                }) {
                    Ok(output) => output,
                    Err(error) => {
                        host.rollback_transaction(transaction.take().expect("transaction exists"));
                        return Err(error);
                    }
                };
                if host.revision() != before_revision {
                    host.rollback_transaction(transaction.take().expect("transaction exists"));
                    return Err(AdapterError::new(
                        AdapterErrorKind::StaleRevision,
                        "revision_changed_before_commit",
                        "host revision changed while a transaction was running",
                    ));
                }
                match host.commit_transaction(
                    transaction.take().expect("transaction exists"),
                    &before_revision,
                ) {
                    Ok(new_revision) => Ok(response_from_output(
                        request,
                        capability,
                        new_revision.into_inner(),
                        output,
                    )),
                    Err((error, transaction)) => {
                        host.rollback_transaction(transaction);
                        Err(error)
                    }
                }
            }
        }
    }
}

fn validate_output<S, P, R>(
    object: &dyn AdapterObject<S, P, R>,
    capability: &CapabilityDescriptor,
    output: &AdapterOutput<R>,
    meter: &mut CallMeter<'_>,
) -> Result<(), AdapterError> {
    meter.checkpoint()?;
    let actual_results = object.validate_output(&capability.id, &output.value)?;
    if actual_results != meter.results() {
        return Err(AdapterError::execution_failed(
            "invalid_result_accounting",
            format!(
                "validated result count {actual_results} differs from metered count {}",
                meter.results()
            ),
        ));
    }
    if let Some(page) = &output.page {
        page.validate(meter.examined())?;
    }
    for diagnostic in &output.diagnostics {
        if diagnostic.code.trim().is_empty() || diagnostic.message.trim().is_empty() {
            return Err(AdapterError::execution_failed(
                "invalid_diagnostic",
                "diagnostic code and message must be nonempty",
            ));
        }
    }
    meter.checkpoint()?;
    Ok(())
}

fn response_from_output<P, R>(
    request: &AdapterRequest<P>,
    capability: &CapabilityDescriptor,
    revision: String,
    output: AdapterOutput<R>,
) -> AdapterResponse<R> {
    AdapterResponse {
        request_id: request.request_id.clone(),
        snapshot_revision: revision,
        response_schema: capability.response_schema.clone(),
        result: output.value,
        page: output.page,
        diagnostics: output.diagnostics,
    }
}
