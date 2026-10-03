<script lang="ts">
  import { pieceName, pieceStatus, text, type PublicRecord } from '../presentation';
  let { piece }: { piece: PublicRecord } = $props();
  const paths: Record<string, string> = {
    pawn: 'M24 9a7 7 0 1 0 0 14 7 7 0 0 0 0-14Z M19 25h10l3 13H16l3-13Z M13 39h22v5H13Z',
    rook: 'M12 9h6v7h4V9h4v7h4V9h6v13l-5 4 2 12H15l2-12-5-4Z M12 39h24v5H12Z',
    knight: 'M16 38l4-12-7-1 1-9 8-8 4 3 6 8 2 19Z M12 39h25v5H12Z M17 21l7-4',
    bishop: 'M24 6l7 9c5 7 0 14-7 14s-12-7-7-14Z M22 13l5 7 M20 29h8l4 9H16Z M12 39h24v5H12Z',
    queen: 'M11 13l7 6 6-10 6 10 7-6-6 22H17Z M16 36h16v3H16Z M12 40h24v4H12Z',
    king: 'M21 5h6v6h6v5h-6v6h-6v-6h-6v-5h6Z M14 25l10-4 10 4-4 12H18Z M12 39h24v5H12Z',
  };
  let kind = $derived(text(piece.type));
  let title = $derived(`${pieceName(piece)}${pieceStatus(piece).length ? ` · ${pieceStatus(piece).join(', ')}` : ''}`);
</script>

<span class="piece" class:black={piece.color === 'black'} class:neutral={piece.color === 'neutral'} title={title}>
  {#if paths[kind]}
    <svg viewBox="0 0 48 48" role="img" aria-label={pieceName(piece)}>
      <path d={paths[kind]} />
      {#if kind === 'knight'}<circle cx="24" cy="15" r="1.8" class="eye" />{/if}
      {#if kind === 'queen'}<circle cx="11" cy="11" r="2" /><circle cx="24" cy="7" r="2" /><circle cx="37" cy="11" r="2" />{/if}
    </svg>
  {:else}
    <span class="special-symbol" aria-hidden="true">{pieceName(piece).slice(0, 2)}</span>
    <span class="special-label">{pieceName(piece)}</span>
  {/if}
  {#if typeof piece.hp === 'number'}<span class="piece-counter">♥{piece.hp}</span>{/if}
  {#if piece.shielded === true || piece.frozen === true}<span class="piece-badge">{piece.frozen === true ? '❄' : '◇'}</span>{/if}
</span>

<style>
  .piece { position: relative; display: flex; width: 86%; height: 86%; margin: auto; flex-direction: column; align-items: center; justify-content: center; color: #fffaf0; filter: drop-shadow(0 2px 0 #171b1926); }
  svg { width: 100%; height: 100%; overflow: visible; }
  path, circle { fill: currentColor; stroke: #1c2620; stroke-width: 2.4; stroke-linejoin: round; stroke-linecap: round; }
  .black { color: #263d33; }
  .black path, .black circle { stroke: #faf3db; }
  .neutral { color: #d6a642; }
  .eye { fill: #171b19; stroke: none; }
  .black .eye { fill: #fff6df; }
  .special-symbol { min-width: 2em; padding: .13em .1em; border: 2px solid #18221c; border-radius: 35% 35% 20% 20%; background: #fffaf0; color: #18221c; text-align: center; font-size: clamp(.7rem, 2vw, 1.4rem); box-shadow: 0 3px 0 #18221c; }
  .black .special-symbol { background: #263d33; color: #fff8df; }
  .neutral .special-symbol { background: #d6a642; }
  .special-label { color: #16231b; max-width: 100%; text-align: center; font-size: clamp(.48rem, .95vw, .65rem); font-weight: 900; background: #fff7de; border-radius: 3px; padding: 0 2px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; margin-top: 4px; }
  .piece-counter { position: absolute; right: -2px; bottom: -2px; padding: 0 3px; font-size: .6rem; font-weight: 900; color: #202822; background: #fff3ce; border: 1px solid #202822; border-radius: 3px; }
  .piece-badge { position: absolute; left: -2px; top: 0; font-size: .65rem; color: #143b35; background: #e6f8f6; border-radius: 50%; }
</style>
