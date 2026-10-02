import test from 'node:test';
import assert from 'node:assert/strict';
import { BrowserEngineClient } from '../src/engine-client.ts';
import type { WorkerPort } from '../src/engine-client.ts';
import type { EngineInfo, GameSnapshot, Reply, Request } from '../src/protocol.ts';
import { PendingBrowserBotDriver, validateBotChoice } from '../src/bot-driver.ts';
import type { BotInput } from '../src/bot-driver.ts';
import { createPublicReport } from '../src/report.ts';

const info = {rulesVersion: 'test', descriptors: []} as unknown as EngineInfo;
const snapshot = (id = 'game-1', revision = 'r1'): GameSnapshot => ({gameId: id, revision, viewer: 'white', decisionActor: 'white', result: null, observation: {protocolVersion: 'test', viewer: 'white', turn: 'white', board: [], ownCards: [], publicState: {}, history: [], opponentHandCount: 0, informationStateKey: 'public'}, diagnostics: []});
class Port implements WorkerPort {
  onmessage: WorkerPort['onmessage'] = null; onerror: WorkerPort['onerror'] = null; onmessageerror: WorkerPort['onmessageerror'] = null;
  requests: Request[] = []; terminated = false; hold = false; revision = 'r1';
  postMessage(request: Request): void { this.requests.push(request); if (!this.hold) queueMicrotask(() => this.reply(request)); }
  reply(request: Request, ok = true): void {
    if (ok && request.command.type === 'apply') this.revision = 'r2';
    if (ok && request.command.type === 'restore') this.revision = request.command.journal.intents.length ? 'r2' : 'r1';
    const value = request.command.type === 'initialize' ? info : snapshot(request.gameId!, this.revision);
    const reply: Reply = ok ? {requestId: request.requestId, gameId: request.gameId, ok: true, value} : {requestId: request.requestId, gameId: request.gameId, ok: false, error: {severity: 'error', kind: 'invalid_input', code: 'illegal_action', message: 'original native failure', stage: 'apply', requestId: request.requestId, gameId: request.gameId}};
    this.onmessage?.({data: reply} as MessageEvent<Reply>);
  }
  terminate(): void { this.terminated = true; }
}
function client() { const ports: Port[] = []; const engine = new BrowserEngineClient({baseUrl: 'https://example.org/demo/', workerFactory: () => {const port = new Port(); ports.push(port); return port;}, timeoutMs: 1000}); return {engine, ports}; }
test('failed engine reply preserves acknowledged board and exact diagnostic', async () => {
  const {engine, ports} = client(); await engine.newGame({gameStyle: 'normal'}, 19, 'white'); ports[0].hold = true;
  const previous = engine.snapshot; const pending = engine.applyIntent({type: 'bad', color: 'white'});
  await new Promise(resolve => setImmediate(resolve)); ports[0].reply(ports[0].requests.at(-1)!, false);
  await assert.rejects(pending, /original native failure/); assert.equal(engine.snapshot, previous); assert.equal(engine.diagnostics.at(-1)?.code, 'illegal_action'); engine.dispose();
});
test('intent object mutation cannot change the recovery journal', async () => {
  const {engine, ports} = client(); await engine.newGame({gameStyle: 'normal'}, 19, 'white'); ports[0].hold = true;
  const intent = {type: 'move', color: 'white' as const, from: {row: 6, col: 0}};
  const pending = engine.applyIntent(intent); await new Promise(resolve => setImmediate(resolve));
  intent.from.row = 1; ports[0].reply(ports[0].requests.at(-1)!); await pending;
  const active = engine.observe('white'); const rejected = assert.rejects(active); await new Promise(resolve => setImmediate(resolve)); engine.cancelPending(); await rejected;
  await engine.observe('white'); const restore = ports[1].requests.find(r => r.command.type === 'restore')!;
  assert.equal(restore.command.type === 'restore' && (restore.command.journal.intents[0].from as {row: number}).row, 6); engine.dispose();
});
test('hard cancel discards unacknowledged apply and restores before next read', async () => {
  const {engine, ports} = client(); await engine.newGame({gameStyle: 'normal'}, 19, 'white'); ports[0].hold = true;
  const pending = engine.applyIntent({type: 'move', color: 'white'}); const rejected = assert.rejects(pending, /cancelled/);
  await new Promise(resolve => setImmediate(resolve)); engine.cancelPending(); await rejected; assert.equal(ports[0].terminated, true);
  await engine.observe('white'); assert.equal(ports[1].requests[1].command.type, 'restore'); assert.equal(engine.snapshot?.revision, 'r1'); engine.dispose();
});
test('late reply from previous game is ignored and repeated clicks are rejected', async () => {
  const {engine, ports} = client(); await engine.newGame({gameStyle: 'normal'}, 19, 'white'); ports[0].hold = true;
  const first = engine.observe('white'); const rejected = assert.rejects(first, /superseded/);
  await new Promise(resolve => setImmediate(resolve)); const stale = ports[0].requests.at(-1)!;
  const next = await engine.newGame({gameStyle: 'chaos'}, 20, 'black'); await rejected; ports[0].reply(stale); assert.equal(engine.snapshot?.gameId, next.gameId);
  ports[1].hold = true; const active = engine.observe('white'); await new Promise(resolve => setImmediate(resolve));
  await assert.rejects(engine.applyIntent({type: 'move', color: 'white'}), /already running/); const cancelled = assert.rejects(active); engine.cancelPending(); await cancelled; engine.dispose();
});
test('AI pending driver does not silently substitute a bot; late/private/invalid output rejected', async () => {
  const driver = new PendingBrowserBotDriver(); assert.equal((await driver.initialize()).status, 'backend-pending'); await assert.rejects(driver.choose(), /PR #28/);
  const input: BotInput = {requestId: 'bot-1', gameId: 'public-1', decision: 1, viewer: 'white', observation: snapshot().observation, publicHistory: [], searchSeed: 8, budget: {maxNodes: 10, maxMilliseconds: 10}};
  const choice = {requestId: 'bot-1', gameId: 'public-1', decision: 1, intent: {type: 'move', color: 'white' as const}, stopReason: 'completed' as const};
  const signal = new AbortController(); assert.equal(validateBotChoice(input, choice, signal.signal).type, 'move');
  assert.throws(() => validateBotChoice(input, {...choice, decision: 0}, signal.signal), /older/);
  assert.throws(() => validateBotChoice(input, {...choice, intent: {...choice.intent, positionKey: 'private'}}, signal.signal), /private/);
  signal.abort(); assert.throws(() => validateBotChoice(input, choice, signal.signal), /cancelled/);
});
test('research export contains public projection and strips local paths without private seed/journal', () => {
  const current = snapshot(); current.observation = new Proxy(current.observation, {});
  const report = createPublicReport(current, info, [{severity: 'error', kind: 'execution_failed', code: 'file', message: 'load C:\\Users\\private-user\\example', stage: 'load', requestId: '1', gameId: '1'}]);
  const json = JSON.stringify(report); assert.ok(json.includes('[local-path]')); assert.ok(!json.includes('private-user')); assert.ok(!json.includes('seed')); assert.ok(!json.includes('revision')); assert.deepEqual(report.publicHistory, []);
});
