import { GameHost } from './game-host.ts';
import { boundedJson, diagnostic, fault } from './protocol.ts';
import type { Diagnostic, EngineInfo, Reply, Request } from './protocol.ts';

let host: GameHost | null = null;
let info: EngineInfo | null = null;
async function initialize(baseUrl: string): Promise<EngineInfo> {
  const base = new URL(baseUrl);
  if (!['https:', 'http:'].includes(base.protocol) || base.origin !== self.location.origin) throw fault('invalid_asset_origin', 'Engine assets must use the current static origin', 'load');
  const jsUrl = new URL('wasm/augment_chess_browser.js', base);
  const wasmUrl = new URL('wasm/augment_chess_browser_bg.wasm', base);
  const response = await fetch(wasmUrl);
  if (!response.ok) throw fault('wasm_download_failed', `WASM download: HTTP ${response.status} ${response.statusText}`, 'load');
  const bytes = await response.arrayBuffer();
  if (bytes.byteLength > 32 * 1024 * 1024) throw fault('wasm_size_exceeded', 'WASM exceeds 32 MiB', 'load');
  // A final HF bundle has a manifest. Development/CI preview uses the same
  // real binaries before packaging and reports the unbundled state explicitly.
  const manifestResponse = await fetch(new URL('source-manifest.json', base));
  const jsResponse = await fetch(jsUrl);
  if (!jsResponse.ok) throw fault('binding_download_failed', `Binding download: HTTP ${jsResponse.status}`, 'load');
  const jsBytes = await jsResponse.arrayBuffer();
  if (jsBytes.byteLength > 32 * 1024 * 1024) throw fault('binding_size_exceeded', 'Binding exceeds 32 MiB', 'load');
  let sourceCommit: string | undefined;
  const diagnostics: Diagnostic[] = [];
  if (manifestResponse.ok && manifestResponse.headers.get('content-type')?.includes('json')) {
    const manifest = await manifestResponse.json();
    if (manifest.schemaVersion !== 1 || !/^[0-9a-f]{40}$/.test(manifest.sourceCommit)) throw fault('invalid_asset_manifest', 'Invalid source manifest', 'integrity');
    const entry = manifest.files?.find((file: {path: string}) => file.path === 'wasm/augment_chess_browser_bg.wasm');
    const hash = [...new Uint8Array(await crypto.subtle.digest('SHA-256', bytes))].map(b => b.toString(16).padStart(2, '0')).join('');
    if (entry?.sha256 !== hash || entry?.bytes !== bytes.byteLength) throw fault('wasm_integrity_failed', 'WASM bytes/hash differ from pinned manifest', 'integrity');
    const jsEntry = manifest.files?.find((file: {path: string}) => file.path === 'wasm/augment_chess_browser.js');
    const jsHash = [...new Uint8Array(await crypto.subtle.digest('SHA-256', jsBytes))].map(b => b.toString(16).padStart(2, '0')).join('');
    if (jsEntry?.sha256 !== jsHash || jsEntry?.bytes !== jsBytes.byteLength) throw fault('binding_integrity_failed', 'Binding bytes/hash differ from pinned manifest', 'integrity');
    sourceCommit = manifest.sourceCommit;
  } else if (!manifestResponse.ok && manifestResponse.status !== 404) throw fault('manifest_download_failed', `Manifest download: HTTP ${manifestResponse.status}`, 'load');
  if (!sourceCommit) diagnostics.push({severity: 'warning', kind: 'integrity', code: 'unbundled_preview', message: '정적 빌드 미리보기에는 배포 manifest가 없습니다. 최종 HF 묶음의 해시 검증은 아직 적용되지 않았습니다.', stage: 'initialize', requestId: '', gameId: null});
  const code = new TextDecoder().decode(jsBytes);
  if (/\b(?:import|export)\s.*\bfrom\s*['"]|\bimport\s*\(/.test(code)) throw fault('binding_not_standalone', 'Pinned WASM binding must be a standalone module', 'integrity');
  // Execute exactly the checked bytes, so a deployment update between fetch
  // and import cannot substitute a different binding on a second URL request.
  const moduleUrl = URL.createObjectURL(new Blob([jsBytes], {type: 'text/javascript'}));
  let binding;
  try { binding = await import(/* @vite-ignore */ moduleUrl); await binding.default({module_or_path: bytes}); }
  finally { URL.revokeObjectURL(moduleUrl); }
  host?.dispose(); host = new GameHost((config, seed) => binding.BrowserGameSession.new_game(config, seed));
  info = {...host.info(), sourceCommit, diagnostics};
  return info;
}
self.onmessage = async (event: MessageEvent<Request>) => {
  const request = event.data;
  let reply: Reply;
  try {
    boundedJson(request);
    const value = request.command.type === 'initialize' ? await initialize(request.command.baseUrl) : host ? host.execute(request) : (() => { throw fault('engine_not_ready', 'WASM engine has not initialized'); })();
    reply = {requestId: request.requestId, gameId: request.gameId, ok: true, value};
    boundedJson(reply);
  } catch (error) { reply = {requestId: request.requestId, gameId: request.gameId, ok: false, error: diagnostic(error, request.command.type, request.requestId, request.gameId)}; }
  self.postMessage(reply);
};
