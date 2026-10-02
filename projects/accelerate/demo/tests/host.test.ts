import test from 'node:test';
import assert from 'node:assert/strict';
import {GameHost} from '../src/game-host.ts';
import type {WasmSession} from '../src/game-host.ts';
import type {EngineInfo, Request} from '../src/protocol.ts';
import {MAX_BYTES, boundedJson} from '../src/protocol.ts';
const schema = {id: 'public-test', sha256: '0'.repeat(64)};
const info: EngineInfo = {rulesVersion: 'v7', catalogVersion: 'public', protocolVersion: 'test', projectionVersion: 'public', observationPolicyHash: 'hash', implementationVersion: 'test', executionProfileVersion: 'test', executionProfileSha256: 'hash', descriptors: [{projectId: 'augment-chess', adapterId: 'test', implementationVersion: 'test', contractVersion: {major: 1, minor: 0}, callLimits: {maxWork: 10, maxResults: 1}, capabilities: ['observe', 'apply-public-intent'].map(id => ({id, access: id === 'observe' ? 'read_only' : 'transactional', requestSchema: schema, responseSchema: schema}))}]};
class Session implements WasmSession {
  revisionValue = 'r1'; freed = false; failProjection = false;
  metadata() { return JSON.stringify(info); } revision() { return this.revisionValue; } decision_actor() {return 'white';} result() {return undefined;} free() {this.freed = true;}
  invoke_json(json: string) {
    const request = JSON.parse(json);
    if (request.payload.kind === 'apply_public_intent') this.revisionValue = 'r2';
    if (this.failProjection && this.revisionValue === 'r2' && request.payload.kind === 'observe') throw {code: 'projection_failure', kind: 'execution_failed', message: 'exact projection error'};
    const result = request.payload.kind === 'observe' ? {kind: 'observation', observation: {viewer: request.payload.viewer}} : {kind: 'applied_public_intent'};
    return JSON.stringify({ok: true, response: {requestId: request.requestId, snapshotRevision: this.revisionValue, responseSchema: schema, result, diagnostics: [{severity: 'warning', code: 'continued', message: 'Exact continuing warning'}]}});
  }
}
const createRequest: Request = {requestId: 'new', gameId: 'public-game', revision: null, command: {type: 'new-game', config: {gameStyle: 'normal'}, seed: 19, viewer: 'white'}};
const applyRequest: Request = {requestId: 'apply', gameId: 'public-game', revision: 'r1', command: {type: 'apply', intent: {type: 'move', color: 'white'}, viewer: 'white'}};
test('host enforces revision/game, returns engine warnings, and accepts new committed write revision', () => {
  const session = new Session(); const host = new GameHost(() => session);
  const initial = host.execute(createRequest); assert.equal(initial.diagnostics[0].message, 'Exact continuing warning');
  assert.throws(() => host.execute({...applyRequest, revision: 'old'}), /revision/);
  assert.throws(() => host.execute({...applyRequest, gameId: 'old-game'}), /another game/);
  const next = host.execute(applyRequest); assert.equal(next.revision, 'r2'); assert.equal(next.diagnostics.length, 2);
});
test('projection and rollback failures discard corrupt session and preserve both exact causes', () => {
  const session = new Session(); let created = 0;
  const host = new GameHost(() => { if (++created > 1) throw {code: 'recreate_failure', message: 'exact recreate error'}; return session; });
  host.execute(createRequest); session.failProjection = true;
  assert.throws(() => host.execute(applyRequest), error => error instanceof Error && error.message.includes('exact projection error') && error.message.includes('exact recreate error'));
  assert.equal(session.freed, true);
  assert.throws(() => host.execute(applyRequest), /No active engine session/);
});

test('combined reply size failure rolls back before acknowledging an applied intent', () => {
  class LargeSession extends Session {
    warningBytes = 0;
    invoke_json(json: string) {
      const outcome = JSON.parse(super.invoke_json(json));
      outcome.response.diagnostics[0].message = 'w'.repeat(this.warningBytes);
      return boundedJson(outcome); // Every individual native response fits.
    }
  }
  const sessions: LargeSession[] = [];
  const host = new GameHost(() => { const session = new LargeSession(); sessions.push(session); return session; });
  host.execute(createRequest);
  sessions[0].warningBytes = MAX_BYTES / 2 + 1024;
  assert.throws(() => host.execute(applyRequest), error => error instanceof Error && error.message.includes('Message exceeds'));
  assert.equal(sessions[0].freed, true);
  const restored = host.execute({...applyRequest, command: {type: 'observe', viewer: 'white'}});
  assert.equal(restored.revision, 'r1');
  assert.equal(host.execute(applyRequest).revision, 'r2');
  host.dispose();
});
