export type Color = 'white' | 'black';
export type PublicIntent = Record<string, unknown> & { type: string; color: Color };
export type GameConfig = { gameStyle: 'normal' | 'chaos' | 'grand'; draftDelete?: boolean; ruleCardIds?: string[]; starWinLimit?: number; deathmatchEnabled?: boolean; deathmatchLimitTurns?: number };
export type Observation = {
  protocolVersion: string; viewer: Color; turn: Color;
  board: (Record<string, unknown> | null)[][]; ownCards: Record<string, unknown>[];
  opponentHandCount: number; publicState: Record<string, unknown>; history: unknown[];
  informationStateKey: string;
};
export type Diagnostic = { severity: 'warning' | 'error' | 'info'; kind: string; code: string; message: string; stage: string; requestId: string; gameId: string | null };
export type SchemaRef = { id: string; sha256: string };
export type Descriptor = { projectId: string; adapterId: string; contractVersion: {major: number; minor: number}; implementationVersion: string; capabilities: {id: string; requestSchema: SchemaRef; responseSchema: SchemaRef; access: string}[]; callLimits: {maxWork: number; maxResults: number} };
export type EngineInfo = { rulesVersion: string; catalogVersion: string; protocolVersion: string; projectionVersion: string; observationPolicyHash: string; implementationVersion: string; executionProfileVersion: string; executionProfileSha256: string; descriptors: Descriptor[]; sourceCommit?: string; diagnostics?: Diagnostic[] };
export type GameSnapshot = { gameId: string; revision: string; viewer: Color; decisionActor: Color; result: 'white' | 'black' | 'draw' | null; observation: Observation; diagnostics: Diagnostic[] };
export type Journal = { config: GameConfig; seed: number; intents: PublicIntent[] };
export type Command =
  | { type: 'initialize'; baseUrl: string }
  | { type: 'new-game'; config: GameConfig; seed: number; viewer: Color }
  | { type: 'observe'; viewer: Color }
  | { type: 'apply'; intent: PublicIntent; viewer: Color }
  | { type: 'restore'; journal: Journal; viewer: Color };
export type Request = { requestId: string; gameId: string | null; revision: string | null; command: Command };
export type Reply = { requestId: string; gameId: string | null; ok: true; value: EngineInfo | GameSnapshot } | { requestId: string; gameId: string | null; ok: false; error: Diagnostic };
export const MAX_BYTES = 8_388_608;
export const MAX_HISTORY = 512;
export function diagnostic(error: unknown, stage: string, requestId: string, gameId: string | null): Diagnostic {
  if (error instanceof EngineFault) return {...error.diagnostic, stage: error.diagnostic.stage === 'transport' ? stage : error.diagnostic.stage, requestId, gameId};
  let value: unknown = error;
  if (typeof value === 'string') { try { value = JSON.parse(value); } catch { /* Preserve raw thrown text below. */ } }
  if (value && typeof value === 'object' && 'message' in value) {
    const item = value as {kind?: unknown; code?: unknown; message: unknown};
    return {severity: 'error', kind: typeof item.kind === 'string' ? item.kind : 'execution_failed', code: typeof item.code === 'string' ? item.code : 'browser_failure', message: String(item.message), stage, requestId, gameId};
  }
  return {severity: 'error', kind: 'execution_failed', code: 'browser_failure', message: String(error), stage, requestId, gameId};
}
export class EngineFault extends Error {
  diagnostic: Diagnostic;
  constructor(item: Diagnostic) { super(item.message); this.name = 'EngineFault'; this.diagnostic = item; }
}
export function fault(code: string, message: string, stage = 'transport'): EngineFault {
  return new EngineFault({severity: 'error', kind: 'invalid_input', code, message, stage, requestId: '', gameId: null});
}
export function boundedJson(value: unknown): string {
  const json = JSON.stringify(value);
  if (new TextEncoder().encode(json).length > MAX_BYTES) throw fault('message_size_exceeded', `Message exceeds ${MAX_BYTES} bytes`);
  return json;
}
// Svelte state objects are proxies and cannot be structuredClone'd. The
// transport is JSON, so normalize its public inputs before transfer/storage.
export function jsonClone<T>(value: T): T { return JSON.parse(boundedJson(value)) as T; }
