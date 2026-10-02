import { boundedJson, diagnostic, EngineFault, fault, MAX_HISTORY } from './protocol.ts';
import type { Color, Descriptor, Diagnostic, EngineInfo, GameConfig, GameSnapshot, Journal, Observation, Request } from './protocol.ts';

export interface WasmSession { metadata(): string; revision(): string; decision_actor(): string; result(): string | undefined; invoke_json(request: string, timeout: number): string; free(): void }
export type SessionFactory = (config: string, seed: number) => WasmSession;
// This host is private to the game Worker. Neither legal-action enumeration nor
// raw state/envelope/RNG crosses the public screen or bot boundary.
export class GameHost {
  private factory: SessionFactory;
  private session: WasmSession | null = null;
  private gameId: string | null = null;
  private metadata: EngineInfo | null = null;
  private journal: Journal | null = null;
  private diagnostics: Diagnostic[] = [];
  constructor(factory: SessionFactory) { this.factory = factory; }
  info(): EngineInfo {
    const temporary = this.factory(JSON.stringify({gameStyle: 'normal'}), 0);
    try { return JSON.parse(temporary.metadata()) as EngineInfo; } finally { temporary.free(); }
  }
  private invoke(session: WasmSession, info: EngineInfo, capabilityId: string, payload: unknown, request: Request): {kind: string; observation?: Observation; [key: string]: unknown} {
    const descriptor = info.descriptors.find((d: Descriptor) => d.capabilities.some(c => c.id === capabilityId));
    const capability = descriptor?.capabilities.find(c => c.id === capabilityId);
    if (!descriptor || !capability) throw fault('capability_missing', `Required capability ${capabilityId} is absent`, 'engine-contract');
    const wire = {requestId: request.requestId, projectId: descriptor.projectId, adapterId: descriptor.adapterId, contractVersion: descriptor.contractVersion, implementationVersion: descriptor.implementationVersion, capabilityId, requestSchema: capability.requestSchema, responseSchema: capability.responseSchema, snapshotRevision: session.revision(), limits: descriptor.callLimits, payload};
    const outcome = JSON.parse(session.invoke_json(boundedJson(wire), 5000));
    if (outcome.ok !== true) throw outcome.error;
    const expectedRevision = capability.access === 'transactional' ? session.revision() : wire.snapshotRevision;
    if (outcome.response.requestId !== request.requestId || outcome.response.snapshotRevision !== expectedRevision || JSON.stringify(outcome.response.responseSchema) !== JSON.stringify(capability.responseSchema)) throw fault('invalid_adapter_response', 'Engine response does not match request/schema/revision', 'engine-contract');
    boundedJson(outcome);
    for (const item of outcome.response.diagnostics ?? []) this.diagnostics.push({...item, kind: 'engine_diagnostic', stage: capabilityId, requestId: request.requestId, gameId: request.gameId});
    return outcome.response.result;
  }
  private snapshot(session: WasmSession, info: EngineInfo, gameId: string, viewer: Color, request: Request): GameSnapshot {
    const value = this.invoke(session, info, 'observe', {kind: 'observe', viewer}, request);
    if (value.kind !== 'observation' || !value.observation || value.observation.viewer !== viewer) throw fault('invalid_observation', 'Engine returned an invalid viewer projection', 'observe');
    const actor = session.decision_actor();
    const result = session.result() ?? null;
    if (!['white', 'black'].includes(actor) || (result !== null && !['white', 'black', 'draw'].includes(result))) throw fault('invalid_engine_state', 'Invalid decision actor or result', 'observe');
    return {gameId, revision: session.revision(), viewer, decisionActor: actor as Color, result: result as GameSnapshot['result'], observation: value.observation, diagnostics: [...this.diagnostics]};
  }
  private create(config: GameConfig, seed: number): WasmSession {
    if (!Number.isSafeInteger(seed) || seed < 0 || seed > 0xffffffff) throw fault('invalid_seed', 'Game seed must be a u32 integer', 'new-game');
    return this.factory(boundedJson(config), seed);
  }
  private rebuild(journal: Journal, request: Request): WasmSession {
    const candidate = this.create(journal.config, journal.seed);
    try {
      const info = JSON.parse(candidate.metadata()) as EngineInfo;
      for (const intent of journal.intents) this.invoke(candidate, info, 'apply-public-intent', {kind: 'apply_public_intent', intent}, request);
      return candidate;
    } catch (error) { candidate.free(); throw error; }
  }
  execute(request: Request): GameSnapshot {
    this.diagnostics = [];
    boundedJson(request);
    const cmd = request.command;
    if (cmd.type === 'new-game' || cmd.type === 'restore') {
      if (!request.gameId) throw fault('missing_game_id', 'Game creation requires a game ID');
      const journal: Journal = cmd.type === 'restore' ? cmd.journal : {config: cmd.config, seed: cmd.seed, intents: []};
      if (journal.intents.length > MAX_HISTORY) throw fault('history_limit', `Recovery is limited to ${MAX_HISTORY} actions`);
      const candidate = this.rebuild(journal, request);
      try {
        const info = JSON.parse(candidate.metadata()) as EngineInfo;
        const snapshot = this.snapshot(candidate, info, request.gameId, cmd.viewer, request);
        this.session?.free(); this.session = candidate; this.metadata = info; this.gameId = request.gameId; this.journal = structuredClone(journal);
        return snapshot;
      } catch (error) { candidate.free(); throw error; }
    }
    if (!this.session || !this.metadata || !this.journal || !this.gameId) throw fault('game_not_ready', 'No active engine session');
    if (request.gameId !== this.gameId) throw fault('stale_game', 'Request belongs to another game');
    if (request.revision !== this.session.revision()) throw fault('stale_revision', 'Request revision differs from current engine revision');
    if (cmd.type === 'observe') return this.snapshot(this.session, this.metadata, this.gameId, cmd.viewer, request);
    if (cmd.type !== 'apply') throw fault('unknown_command', 'Invalid game command');
    if (this.journal.intents.length >= MAX_HISTORY) throw fault('history_limit', `Demo game is limited to ${MAX_HISTORY} committed actions`);
    // Final projection is part of the browser transaction. If it fails after
    // the native write, recreate the last acknowledged private state exactly.
    try {
      this.invoke(this.session, this.metadata, 'apply-public-intent', {kind: 'apply_public_intent', intent: cmd.intent}, request);
      const snapshot = this.snapshot(this.session, this.metadata, this.gameId, cmd.viewer, request);
      this.journal.intents.push(structuredClone(cmd.intent));
      return snapshot;
    } catch (error) {
      const previous = this.session;
      try { this.session = this.rebuild(this.journal, request); previous.free(); }
      catch (recoveryError) {
        previous.free(); this.session = null;
        const original = diagnostic(error, 'apply', request.requestId, request.gameId);
        const recovery = diagnostic(recoveryError, 'rollback', request.requestId, request.gameId);
        throw new EngineFault({...original, kind: 'execution_failed', code: 'rollback_failed', message: `Original failure [${original.code}]: ${original.message}\nRecovery failure [${recovery.code}]: ${recovery.message}`});
      }
      throw error;
    }
  }
  dispose(): void { this.session?.free(); this.session = null; this.journal = null; }
}
