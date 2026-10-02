import type { Diagnostic, EngineInfo, GameSnapshot } from './protocol.ts';
import {jsonClone} from './protocol.ts';
export function createPublicReport(snapshot: GameSnapshot, info: EngineInfo, diagnostics: Diagnostic[]) {
  const {descriptors: _descriptors, diagnostics: engineDiagnostics = [], ...versions} = info;
  // No seed, private journal, envelope or native Position identity is exported.
  // Local path fragments are replaced in diagnostic text, not silently dropped.
  const redact = (value: string) => value.replace(/[A-Za-z]:[\\/][^\r\n"<>]*/g, '[local-path]').replace(/\/(?:Users|home|mnt\/c\/Users)\/[^\s"<>]+/g, '[local-path]');
  const publicDiagnostics = [...engineDiagnostics, ...diagnostics].map(d => ({...d, message: redact(d.message)}));
  return {schemaVersion: 1, scope: 'public-observation-and-interaction-research', limitations: '비공개 전체 상태의 완전 재현 자료가 아닙니다.', versions: jsonClone(versions), viewer: snapshot.viewer, observation: jsonClone(snapshot.observation), publicHistory: jsonClone(snapshot.observation.history), diagnostics: publicDiagnostics, search: {status: 'backend-pending', model: 'model-missing', stopReason: null}};
}
export function exportPublicReport(snapshot: GameSnapshot, info: EngineInfo, diagnostics: Diagnostic[]): void {
  const blob = new Blob([JSON.stringify(createPublicReport(snapshot, info, diagnostics), null, 2)], {type: 'application/json'});
  const url = URL.createObjectURL(blob); const anchor = document.createElement('a');
  anchor.href = url; anchor.download = 'augment-chess-public-research.json'; anchor.click();
  setTimeout(() => URL.revokeObjectURL(url), 0);
}
