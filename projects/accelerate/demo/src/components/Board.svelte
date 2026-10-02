<script lang="ts">
  import Piece from './Piece.svelte';
  import { anchorSquare, colorName, markLabel, pieceName, pieceStatus, record, records, sameSquare, square, squareName, type Color, type PublicObservation, type Square } from '../presentation';
  let { observation, viewer = 'white', selected = null, highlights = [], targets = [], disabled = false, onSquare }:
    { observation: PublicObservation | null; viewer?: Color; selected?: Square | null; highlights?: Square[]; targets?: Square[]; disabled?: boolean; onSquare: (at: Square) => void } = $props();
  let rows = $derived(viewer === 'black' ? [7, 6, 5, 4, 3, 2, 1, 0] : [0, 1, 2, 3, 4, 5, 6, 7]);
  let cols = $derived(viewer === 'black' ? [7, 6, 5, 4, 3, 2, 1, 0] : [0, 1, 2, 3, 4, 5, 6, 7]);
  let marks = $derived(records(observation?.publicState.boardMarks));
  let overlays = $derived(records(observation?.publicState.overlays));
  let relationships = $derived(records(observation?.publicState.relationships));
  let lastMove = $derived(record(observation?.publicState.lastMove));
  function marksAt(at: Square) { return marks.filter((mark) => sameSquare(square(mark.square), at)); }
  function ghostsAt(at: Square) {
    return overlays.filter((overlay) => overlay.kind === 'quantum' && Array.isArray(overlay.cells)
      && overlay.cells.some((cell) => sameSquare(square(cell), at)));
  }
  function displayPoint(at: Square | null) {
    if (!at) return { x: 0, y: 0 };
    return viewer === 'black' ? { x: 7.5 - at.col, y: 7.5 - at.row } : { x: at.col + .5, y: at.row + .5 };
  }
  function keyboard(event: KeyboardEvent, displayRow: number, displayCol: number) {
    const offsets: Record<string, [number, number]> = { ArrowUp: [-1, 0], ArrowDown: [1, 0], ArrowLeft: [0, -1], ArrowRight: [0, 1] };
    const offset = offsets[event.key];
    if (!offset) return;
    event.preventDefault();
    const row = Math.max(0, Math.min(7, displayRow + offset[0]));
    const col = Math.max(0, Math.min(7, displayCol + offset[1]));
    const board = (event.currentTarget as HTMLElement).closest('.board');
    (board?.querySelector(`[data-cell="${row}-${col}"]`) as HTMLButtonElement | null)?.focus();
  }
</script>

<div class="board-shell">
  <div class="board" role="group" aria-label="체스 보드. 방향키로 칸 이동, Enter 또는 Space로 선택">
    {#each rows as row, rowIndex}
      {#each cols as col, colIndex}
        {@const at = { row, col }}
        {@const piece = observation?.board[row]?.[col] ?? null}
        {@const anchor = piece ? anchorSquare(piece, at) : at}
        {@const bodyCell = piece !== null && !sameSquare(anchor, at)}
        {@const cellMarks = marksAt(at)}
        {@const ghosts = ghostsAt(at)}
        {@const hidden = cellMarks.some((mark) => mark.kind === 'fogHidden')}
        {@const isTarget = targets.some((target) => sameSquare(target, at))}
        {@const hinted = highlights.some((target) => sameSquare(target, at))}
        {@const label = `${squareName(at)}${piece ? ` ${colorName(piece.color)} ${pieceName(piece)} ${pieceStatus(piece).join(', ')}` : ' 빈 칸'}${cellMarks.length ? ` · ${cellMarks.map(markLabel).join(', ')}` : ''}${ghosts.length ? ` · 양자 잔상 ${ghosts.map((ghost) => pieceName(record(ghost.piece))).join(', ')}` : ''}`}
        <button type="button" class="cell" class:dark={(row + col) % 2 !== 0} class:fog={hidden} class:selected={sameSquare(selected, at) || (piece !== null && sameSquare(selected, anchor))} class:target={isTarget} class:hinted class:body={bodyCell} class:last={sameSquare(square(lastMove.from), at) || sameSquare(square(lastMove.to), at)}
          data-cell={`${rowIndex}-${colIndex}`} aria-label={label} aria-pressed={sameSquare(selected, at)} aria-disabled={disabled} title={label}
          onclick={() => { if (!disabled) onSquare(at); }} onkeydown={(event) => keyboard(event, rowIndex, colIndex)}>
          {#if colIndex === 0}<span class="rank">{8 - row}</span>{/if}
          {#if rowIndex === 7}<span class="file">{String.fromCharCode(97 + col)}</span>{/if}
          {#if piece && !bodyCell}<Piece {piece} />{:else if bodyCell}<span class="body-part" aria-hidden="true">⌑</span>{/if}
          {#each ghosts as ghost}
            <span class="quantum-ghost" class:over-piece={!!piece} title={`공개된 양자 잔상 · ${pieceName(record(ghost.piece))}`}>
              {#if Array.isArray(ghost.cells) && sameSquare(square(ghost.cells[0]), at)}<Piece piece={record(ghost.piece)} />{:else}<span class="body-part" aria-hidden="true">⌑</span>{/if}
            </span>
          {/each}
          {#if hinted}<span class="hint-dot" aria-hidden="true"></span>{/if}
          {#if isTarget}<span class="target-number">{targets.findIndex((target) => sameSquare(target, at)) + 1}</span>{/if}
          {#if cellMarks.some((mark) => mark.kind !== 'fogHidden')}<span class="mark-tag" title={cellMarks.map(markLabel).join(', ')}>{markLabel(cellMarks.find((mark) => mark.kind !== 'fogHidden') ?? {}).slice(0, 2)}</span>{/if}
        </button>
      {/each}
    {/each}
    <svg class="relationship-lines" viewBox="0 0 8 8" aria-label="공개 기물 연결">
      {#each relationships as relationship}
        {#if square(relationship.from) && square(relationship.to)}
          {@const from = displayPoint(square(relationship.from))}
          {@const to = displayPoint(square(relationship.to))}
          <line x1={from.x} y1={from.y} x2={to.x} y2={to.y} class:black-link={relationship.owner === 'black'}><title>{relationship.kind}: {squareName(square(relationship.from)!)} ↔ {squareName(square(relationship.to)!)}</title></line>
        {/if}
      {/each}
    </svg>
  </div>
  <p class="board-caption">{observation ? `${colorName(viewer)} 관점 · 엔진의 공개 표시` : '새 게임을 시작하면 엔진 보드가 표시됩니다.'}</p>
</div>

<style>
  .board-shell { width: 100%; }
  .board { position: relative; display: grid; grid-template-columns: repeat(8, minmax(0, 1fr)); aspect-ratio: 1; border: 4px solid #1c2921; border-radius: 8px; overflow: hidden; background: #f8eecf; box-shadow: 0 6px 0 #1c2921; }
  .cell { position: relative; display: flex; align-items: center; justify-content: center; padding: 3px; border: 0; border-radius: 0; background: #f5edcf; color: #384235; min-width: 0; aspect-ratio: 1; cursor: pointer; overflow: hidden; }
  .dark { background: #92ab88; }
  .cell:hover { box-shadow: inset 0 0 0 3px #fff7d3; }
  .cell:focus-visible { outline: 3px solid #17372b; outline-offset: -3px; z-index: 2; }
  .last { background-image: linear-gradient(#e7b8323c, #e7b8323c); }
  .selected { box-shadow: inset 0 0 0 4px #efab20 !important; background-image: linear-gradient(#f8ce6759, #f8ce6759); }
  .target { box-shadow: inset 0 0 0 3px #b5523b; }
  .fog { background-image: repeating-linear-gradient(135deg, #17362b38 0, #17362b38 3px, #17362b15 3px, #17362b15 8px); }
  .rank, .file { position: absolute; font-size: clamp(.5rem, 1.1vw, .7rem); font-weight: 900; z-index: 1; }
  .rank { top: 2px; left: 3px; }
  .file { bottom: 1px; right: 4px; }
  .hint-dot { position: absolute; width: 24%; height: 24%; border: 2px solid #294e36; background: #31613b70; border-radius: 50%; pointer-events: none; }
  .hinted:has(:global(.piece)) .hint-dot { width: 82%; height: 82%; background: transparent; border-width: 4px; }
  .target-number { position: absolute; top: 2px; right: 3px; color: #fff8e2; background: #a6432f; border-radius: 50%; font-size: .6rem; padding: 0 4px; font-weight: 900; }
  .mark-tag { position: absolute; bottom: 2px; left: 2px; background: #fff3c4e6; font-size: clamp(.45rem, .85vw, .6rem); border: 1px solid #72562c; border-radius: 2px; padding: 0 1px; pointer-events: none; }
  .body-part { width: 80%; height: 80%; display: grid; place-items: center; border: 2px dashed #213d3466; border-radius: 25%; color: #213d3466; font-size: 2rem; background: #ffffff14; }
  .quantum-ghost { position: absolute; inset: 4%; display: grid; place-items: center; opacity: .4; pointer-events: none; filter: hue-rotate(90deg); }
  .quantum-ghost.over-piece { inset: 0 0 45% 45%; opacity: .7; }
  .relationship-lines { position: absolute; inset: 0; width: 100%; height: 100%; pointer-events: none; }
  .relationship-lines line { stroke: #e3ae25bb; stroke-width: .055; stroke-dasharray: .09 .08; }
  .relationship-lines .black-link { stroke: #3f6961bb; }
  .board-caption { margin: 13px 0 0; color: #516655; font-size: .75rem; }
</style>
