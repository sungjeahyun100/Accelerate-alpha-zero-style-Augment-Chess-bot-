import { boundedJson, diagnostic, EngineFault, fault, jsonClone } from './protocol.ts';
import type { Color, Command, Diagnostic, EngineInfo, GameConfig, GameSnapshot, Journal, PublicIntent, Reply, Request } from './protocol.ts';
export type WorkerPort = { postMessage(message: Request): void; terminate(): void; onmessage: ((event: MessageEvent<Reply>) => void) | null; onerror: ((event: ErrorEvent) => void) | null; onmessageerror: ((event: MessageEvent) => void) | null };
export class BrowserEngineClient {
  private worker: WorkerPort | null = null;
  private factory: () => WorkerPort;
  private baseUrl: string;
  private timeout: number;
  private serial = 0;
  private generation = 0;
  private ready = false;
  private journal: Journal | null = null;
  private confirmed: GameSnapshot | null = null;
  private pending: {request: Request; resolve: (value: EngineInfo | GameSnapshot) => void; reject: (error: EngineFault) => void; timer: ReturnType<typeof setTimeout>} | null = null;
  private disposed = false;
  diagnostics: Diagnostic[] = [];
  constructor(options: {workerFactory?: () => WorkerPort; baseUrl?: string; timeoutMs?: number} = {}) {
    this.factory = options.workerFactory ?? (() => new Worker(new URL('./game.worker.ts', import.meta.url), {type: 'module'}));
    this.baseUrl = options.baseUrl ?? new URL('./', document.baseURI).href;
    this.timeout = options.timeoutMs ?? 15000;
  }
  get snapshot(): GameSnapshot | null { return this.confirmed; }
  private stop(error: EngineFault): void {
    const pending = this.pending; this.pending = null;
    if (pending) { clearTimeout(pending.timer); pending.reject(error); }
    this.worker?.terminate(); this.worker = null; this.ready = false;
  }
  private record(error: EngineFault): void { this.diagnostics.push(error.diagnostic); if (this.diagnostics.length > 200) this.diagnostics.shift(); }
  private send(command: Command, gameId: string | null, revision: string | null): Promise<EngineInfo | GameSnapshot> {
    if (this.disposed) return Promise.reject(fault('client_disposed', 'Engine client has been disposed'));
    if (this.pending) return Promise.reject(fault('request_in_progress', 'An engine request is already running'));
    if (!this.worker) {
      this.worker = this.factory();
      this.worker.onmessage = event => {
        const pending = this.pending;
        if (!pending || event.data.requestId !== pending.request.requestId || event.data.gameId !== pending.request.gameId) return;
        clearTimeout(pending.timer); this.pending = null;
        try {
          boundedJson(event.data);
          if (!event.data.ok) { const error = new EngineFault(event.data.error); this.record(error); if (error.diagnostic.code === 'rollback_failed') this.stop(error); pending.reject(error); }
          else pending.resolve(event.data.value);
        } catch (error) { const item = new EngineFault(diagnostic(error, pending.request.command.type, pending.request.requestId, pending.request.gameId)); this.record(item); this.stop(item); pending.reject(item); }
      };
      const failed = (error: unknown) => { const p = this.pending; const item = new EngineFault(diagnostic(error, 'worker', p?.request.requestId ?? '', p?.request.gameId ?? null)); this.record(item); this.stop(item); };
      this.worker.onerror = event => failed(event.message);
      this.worker.onmessageerror = () => failed(fault('worker_message_failed', 'Worker message could not be decoded'));
    }
    const request: Request = {requestId: `${this.generation}:${++this.serial}`, gameId, revision, command};
    boundedJson(request);
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => { const item = new EngineFault(diagnostic(fault('worker_timeout', `Worker exceeded ${this.timeout} ms`), command.type, request.requestId, gameId)); this.record(item); this.stop(item); }, this.timeout);
      this.pending = {request, resolve, reject, timer};
      try { this.worker!.postMessage(request); } catch (error) { const item = new EngineFault(diagnostic(error, command.type, request.requestId, gameId)); this.record(item); this.stop(item); }
    });
  }
  async initialize(): Promise<EngineInfo> {
    if (this.pending) throw fault('request_in_progress', 'An engine request is already running');
    const info = await this.send({type: 'initialize', baseUrl: this.baseUrl}, null, null) as EngineInfo;
    this.ready = true; return info;
  }
  private async ensureReady(): Promise<void> {
    if (this.ready) return;
    await this.initialize();
    if (this.confirmed && this.journal) {
      try {
        const value = await this.send({type: 'restore', journal: this.journal, viewer: this.confirmed.viewer}, this.confirmed.gameId, null) as GameSnapshot;
        if (value.revision !== this.confirmed.revision || JSON.stringify(value.observation) !== JSON.stringify(this.confirmed.observation)) throw fault('recovery_mismatch', 'Recovered engine differs from last acknowledged public state', 'recovery');
      } catch (error) { const item = new EngineFault(diagnostic(error, 'restore', '', this.confirmed.gameId)); this.record(item); this.stop(item); throw item; }
    }
  }
  async newGame(config: GameConfig, seed: number, viewer: Color): Promise<GameSnapshot> {
    config = jsonClone(config);
    this.generation++;
    if (this.pending) this.stop(fault('superseded_game', 'Pending request was superseded by a new game'));
    // A fresh Worker prevents the previous game's late reply committing here.
    this.worker?.terminate(); this.worker = null; this.ready = false;
    await this.initialize();
    const generation = this.generation;
    const gameId = `game-${generation}`;
    const value = await this.send({type: 'new-game', config, seed, viewer}, gameId, null) as GameSnapshot;
    if (generation !== this.generation) throw fault('stale_game', 'Game creation response is stale');
    this.journal = {config: structuredClone(config), seed, intents: []}; this.confirmed = value;
    return value;
  }
  async observe(viewer: Color): Promise<GameSnapshot> {
    await this.ensureReady();
    if (!this.confirmed) throw fault('game_not_ready', 'Start a game first');
    const value = await this.send({type: 'observe', viewer}, this.confirmed.gameId, this.confirmed.revision) as GameSnapshot;
    this.confirmed = value; return value;
  }
  async applyIntent(intent: PublicIntent): Promise<GameSnapshot> {
    const submitted = jsonClone(intent);
    await this.ensureReady();
    if (!this.confirmed || !this.journal) throw fault('game_not_ready', 'Start a game first');
    const value = await this.send({type: 'apply', intent: submitted, viewer: this.confirmed.viewer}, this.confirmed.gameId, this.confirmed.revision) as GameSnapshot;
    this.journal.intents.push(submitted); this.confirmed = value; return value;
  }
  cancelPending(): void { if (this.pending) { const p = this.pending; const item = new EngineFault(diagnostic(fault('cancelled', 'Worker operation cancelled; unacknowledged changes discarded'), p.request.command.type, p.request.requestId, p.request.gameId)); this.record(item); this.stop(item); } }
  dispose(): void { this.disposed = true; this.stop(fault('client_disposed', 'Engine client disposed')); this.confirmed = null; this.journal = null; }
}
