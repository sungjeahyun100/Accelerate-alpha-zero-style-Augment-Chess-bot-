import { fault, jsonClone } from './protocol.ts';
import type { Color, Observation, PublicIntent } from './protocol.ts';
export type BotStatus = {status: 'backend-pending' | 'model-missing' | 'ready' | 'incompatible' | 'error'; message: string};
export type BotInput = {requestId: string; gameId: string; decision: number; viewer: Color; observation: Observation; publicHistory: unknown[]; searchSeed: number; budget: {maxNodes: number; maxMilliseconds: number}};
export type BotChoice = {requestId: string; gameId: string; decision: number; intent: PublicIntent; stopReason: 'budget' | 'completed' | 'cancelled'};
export interface BrowserBotDriver {
  initialize(signal: AbortSignal): Promise<BotStatus>;
  capabilities(): BotStatus;
  choose(input: BotInput, signal: AbortSignal): Promise<BotChoice>;
  cancel(requestId: string): void;
  dispose(): void;
}
export function getBrowserBotStatus(): BotStatus { return {status: 'backend-pending', message: 'PR #28에서 브라우저 AI 실행 방식의 결정을 기다리고 있습니다. 학습 모델도 아직 제공되지 않았습니다.'}; }
export class PendingBrowserBotDriver implements BrowserBotDriver {
  async initialize(): Promise<BotStatus> { return getBrowserBotStatus(); }
  capabilities(): BotStatus { return getBrowserBotStatus(); }
  async choose(): Promise<BotChoice> { throw fault('backend-pending', getBrowserBotStatus().message, 'ai'); }
  cancel(): void { /* No AI Worker exists before the backend is chosen. */ }
  dispose(): void { /* No resources have been allocated. */ }
}
// Called by a future AI Worker bridge before handing an intent to the game
// Worker; identifiers here are public decision counters, never Position IDs.
export function validateBotChoice(input: BotInput, choice: BotChoice, signal: AbortSignal): PublicIntent {
  if (signal.aborted || choice.stopReason === 'cancelled') throw fault('cancelled', 'AI search was cancelled', 'ai');
  if (choice.requestId !== input.requestId || choice.gameId !== input.gameId || choice.decision !== input.decision) throw fault('stale_bot_response', 'AI response belongs to an older public decision', 'ai');
  if (!['budget', 'completed'].includes(choice.stopReason) || !choice.intent || typeof choice.intent.type !== 'string' || choice.intent.color !== input.viewer) throw fault('invalid_bot_intent', 'AI returned an invalid public intent', 'ai');
  if ('positionKey' in choice.intent || 'rngState' in choice.intent || 'actionId' in choice.intent) throw fault('private_bot_payload', 'AI output includes private execution identity', 'ai');
  return jsonClone(choice.intent);
}
