<script lang="ts">
  import type { CardView } from '../presentation';
  let { card, selected = false, disabled = false, compact = false, onSelect = undefined }:
    { card: CardView; selected?: boolean; disabled?: boolean; compact?: boolean; onSelect?: () => void } = $props();
</script>

<button type="button" class="card" class:selected class:compact class:spent={card.used || card.recovering} disabled={disabled || !onSelect} onclick={onSelect} aria-pressed={onSelect ? selected : undefined} title={`${card.name}\n${card.text}${card.help ? `\n${card.help}` : ''}`}>
  <span class="card-top"><span class="phase">{card.phase || 'CARD'}</span><span class="stars">★ {card.stars}</span></span>
  <span class="card-name">{card.name}</span>
  {#if !compact}<span class="card-text">{card.text}</span>{/if}
  {#if Object.keys(card.revealed ?? {}).length}<span class="revealed">{Object.entries(card.revealed ?? {}).map(([key, value]) => `${key}: ${String(value)}`).join(' · ')}</span>{/if}
  <span class="card-bottom">{card.used ? '사용 완료' : card.recovering ? '회복 중' : card.target ? '대상 선택' : '공개 카드'}{card.slot !== null ? ` · ${card.slot + 1}번` : ''}</span>
</button>

<style>
  .card { display: flex; text-align: left; flex-direction: column; width: 100%; min-height: 136px; padding: 12px; gap: 8px; border: 2px solid #233a2b; border-radius: 9px; background: #fffdf4; color: #1d3225; box-shadow: 0 3px 0 #233a2b; transition: transform .1s; }
  .card:not(:disabled):hover { transform: translateY(-2px); background: #fff4ce; }
  .card:focus-visible { outline: 3px solid #e5a521; outline-offset: 3px; }
  .card:disabled { cursor: default; opacity: 1; }
  .selected { border-color: #ba7d0d; background: #ffefbb; box-shadow: 0 3px 0 #ba7d0d; }
  .spent { background: #eff0e8; color: #627163; }
  .card-top { display: flex; align-items: center; justify-content: space-between; font-size: .65rem; font-weight: 850; }
  .phase { letter-spacing: .1em; }
  .stars { color: #9d6d11; background: #fff0bf; border-radius: 4px; padding: 2px 5px; }
  .card-name { font-size: .93rem; font-weight: 900; }
  .card-text { line-height: 1.55; font-size: .74rem; flex: 1; white-space: normal; }
  .card-bottom { color: #657363; font-size: .65rem; }
  .revealed { color: #657363; font-size: .63rem; overflow-wrap: anywhere; }
  .compact { min-height: 80px; padding: 9px; gap: 5px; }
  .compact .card-name { font-size: .8rem; }
</style>
