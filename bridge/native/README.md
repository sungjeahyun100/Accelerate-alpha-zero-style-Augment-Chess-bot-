# Native Python boundary

`accelerate-native` builds `accelerate_chess._native` with PyO3 0.29 and
maturin. It depends on the independent engine and owns no chess, search or
training implementation. The root Cargo workspace and `pyproject.toml` own
dependency versions and locks.

`Position.new_game(config=None, seed=0)` accepts a direct configuration mapping.
`Position.from_state(mapping)` initializes a source-shaped state through the
engine's source-preserving import. `Position.from_snapshot(mapping)` and
`Position.from_json(text)` validate the exact position v1 version and JCS SHA-256
identity before importing its state, RNG and history. `snapshot()` and
`to_json()` produce the same persistence data. Snapshot import rejects metadata
duplication and normalization that would change that persisted identity.

`legal_actions()` returns immutable `Action` objects attached to that private
position identity. `action_stream().next_page(limit=256)` returns
`{"actions": tuple[Action, ...], "exhausted": bool}` in the same order, with a
bounded page size of 1–4096. The stream owns its immutable position and
serializes concurrent cursor access; its pages survive the stream's lifetime.
Use it to enumerate large candidate sets without building a full action list.
`bind_action(payload)` asks the engine to validate a single exact semantic payload;
it does not invent flags or targets. `bind_snapshot(action_v1)` also validates
the persisted action's version, position identity and semantic SHA-256 before
binding. `apply(action)` returns an immutable
`StepResult` containing a separately owned position, the actor, turn marker,
captures and result. A stale action raises `StaleActionError`, and unsupported
engine features raise `UnsupportedFeatureError`. Valid public frames that cannot
match a particle raise `ConditioningMismatchError`; malformed frames and unsupported
rules keep their distinct errors. All other engine errors remain
explicit `NativeError` failures. Native methods do not catch a rejected rule and
substitute a successful transition.

`Action.public_intent()` delegates the source UI choice identity to its owned
immutable engine position. Move intents omit world-dependent execution flags;
card branches and ordered targets retain their source meaning.
`Position.bind_public_intent(intent)` resolves that choice locally and returns
an execution action. Use the intent for policy features and the lossless
`as_payload()` only for execution/replay. `sample_initial_public(config,
observation, seed)` accepts a public configuration, an initial public frame and
an independent bounded seed; it receives no actual private position or RNG.
`condition_public_identities(observation)` delegates source-valid identity
conditioning of an immutable particle. Unsupported conditional families remain
explicit engine errors; these helpers do not implement filtering or search.
`site_catalog()` returns an owned mapping of the compiled frozen catalog, so an
installed wheel can construct its encoder specification without a source checkout.

Positions share immutable `Arc` snapshots. Python mappings and arrays are copied
into owned values while attached to Python; rule calls then run with
`Python::detach`. No borrowed Python object or NumPy buffer survives that
boundary. Returned mappings and arrays are independent copies, so a caller can
mutate or release them without changing a position. The binding requires finite
numbers, exact JSON integer range, depth at most 64, at most 100,000 nodes, and
at most 8 MiB of converted data. Object-array input is rejected. Supported
numerical NumPy input is copied in logical order, including strided input.

`observe(viewer)` delegates public projection to the engine; `board(viewer)`
returns an owned 8×8 NumPy object array of that same public board. These are not
private board accessors. `position_id` and `Action.snapshot()` are control and
persistence APIs: they must not become encoder features. Production candidate
features use `Action.public_intent()`; `as_payload()` preserves the lossless
execution/replay action. `legal_actions()` is available for an environment or a
sampled belief position, and search must not inspect the actual hidden position's
legal actions. Value sign changes use the decision actors before and after a
transition, rather than assuming every action changes the player.

See [Python setup](../../python/README.md) for builds and validation and the
[project rules](../../AGENTS.md) for generated-file locations. The native tests
reuse small states for persistence equivalence, null card slots, multi-cell
identity, immutable branches, stale actions, direct NumPy conversion, and
concurrent calls. They validate the FFI boundary; they do not prove completion
of the site's full rule catalog.

The extension also exports `InferenceSession` for the independent
[ONNX CPU runtime](../runtime/README.md). It accepts verified bundle metadata
and owned FP32 inputs and does not depend on the Python reference evaluator.
