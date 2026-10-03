<script lang="ts">
  import { onMount } from 'svelte';
  import Board from './components/Board.svelte';
  import Card from './components/Card.svelte';
  import { BrowserEngineClient } from './engine-client';
  import { getBrowserBotStatus } from './bot-driver';
  import { exportPublicReport } from './report';
  import { getCardSelectionForm, buildCardIntent, buildMoveIntent, buildPieceIntent, getPublicMoveModes, wizardSpellChoices, type PublicMoveMode } from './card-selection';
  import { diagnostic as createDiagnostic, EngineFault, fault, type Diagnostic, type EngineInfo, type GameSnapshot } from './protocol';
  import { anchorSquare, cardTargetHints, catalogProvenance, colorName, draftCards, historySummary, moveHints, numeric, opponentCards, ownCards, pieceName, pieceStatus, record, records, sameSquare, square, squareName, text, type CardView, type Color, type PublicIntent, type PublicObservation, type PublicRecord, type Square } from './presentation';

  const client = new BrowserEngineClient();
  let info = $state<EngineInfo | null>(null);
  let snapshot = $state<GameSnapshot | null>(null);
  let diagnostics = $state<Diagnostic[]>([]);
  let engineStatus = $state<'loading' | 'ready' | 'error'>('loading');
  let busy = $state(false);
  let activity = $state('엔진 준비 중');
  let lastError = $state('');
  let gameStyle = $state<'normal' | 'chaos' | 'grand'>('normal');
  let playMode = $state<'manual' | 'human-ai'>('manual');
  let humanColor = $state<Color>('white');
  let seed = $state(19);
  let selectedSquare = $state<Square | null>(null);
  let selectedGhost = $state<PublicRecord | null>(null);
  type VisibleOrigin = { kind: 'physical' | 'quantum'; at: Square; piece: PublicRecord };
  let originChoices = $state<VisibleOrigin[]>([]);
  let selectedCard = $state<CardView | null>(null);
  let targetValues = $state<Record<string, unknown>>({});
  let activeSlot = $state(0);
  let movementMode = $state<PublicMoveMode>('normal');
  let wizardSpell = $state('');
  let requestEpoch = 0;
  let bot = $state(getBrowserBotStatus());

  let observation = $derived(snapshot ? snapshot.observation as PublicObservation : null);
  let publicState = $derived(observation?.publicState ?? {});
  let viewer = $derived((snapshot?.viewer ?? humanColor) as Color);
  let actor = $derived((snapshot?.decisionActor ?? viewer) as Color);
  let mode = $derived(text(publicState.mode));
  let cards = $derived(ownCards(observation));
  let acquiredOpponent = $derived(opponentCards(observation));
  let draft = $derived(record(publicState.draft));
  let offers = $derived(draftCards(observation));
  let selectionPhase = $derived(record(publicState.selectionPhase));
  let canAct = $derived(!!snapshot && !snapshot.result && !busy && engineStatus === 'ready' && actor === viewer);
  let selectedPiece = $derived(selectedGhost ?? (selectedSquare ? observation?.board[selectedSquare.row]?.[selectedSquare.col] ?? null : null));
  let form = $derived(selectionForm());
  let slot = $derived(form?.slots[activeSlot]);
  let chosenTargets = $derived(Object.values(targetValues).flatMap((value) => Array.isArray(value)
    ? value.map(square).filter((at): at is Square => !!at)
    : square(value) ? [square(value)!] : []));
  let highlights = $derived(selectedCard ? chosenTargets.length === 0 ? cardTargetHints(observation, selectedCard.instanceId) : [] : selectedGhost ? [] : moveHints(observation, selectedSquare));
  let history = $derived(observation?.history ?? []);
  let completeTargets = $derived(!!form && form.slots.every((field) => field.kind === 'squares'
    ? Array.isArray(targetValues[field.key]) && (targetValues[field.key] as unknown[]).length >= (form.minSelections ?? 1)
      && (targetValues[field.key] as unknown[]).length <= (form.maxSelections ?? 64)
    : targetValues[field.key] !== undefined));

  function selectionForm() {
    if (!selectedCard) return null;
    try { return getCardSelectionForm(selectedCard.id, { cards }); }
    catch { return null; /* The selection handler records the exact grammar error. */ }
  }
  function diagnostic(error: unknown, stage: string) {
    const entry = error instanceof EngineFault ? error.diagnostic : createDiagnostic(error, stage, `ui-${requestEpoch}`, snapshot?.gameId ?? null);
    diagnostics = [...diagnostics.slice(-99), entry];
    lastError = entry.message;
    return entry;
  }
  function clearSelection() {
    selectedSquare = null;
    selectedGhost = null;
    originChoices = [];
    selectedCard = null;
    targetValues = {};
    activeSlot = 0;
    movementMode = 'normal';
    wizardSpell = '';
  }
  async function publish(next: GameSnapshot, epoch: number) {
    // Publish the acknowledged transition before attempting a perspective
    // refresh. A failed observe must not make an applied action look rejected.
    snapshot = next;
    clearSelection();
    lastError = '';
    if (playMode === 'manual' && !next.result && next.viewer !== next.decisionActor) {
      try {
        const refreshed = await client.observe(next.decisionActor as Color);
        if (epoch !== requestEpoch) return;
        next = refreshed;
      } catch (error) {
        if (epoch === requestEpoch) {
          diagnostic(error, 'observe');
          activity = '행동은 확정됐지만 다음 관점을 불러오지 못했습니다.';
        }
        return;
      }
    }
    if (epoch !== requestEpoch) return;
    snapshot = next;
    activity = next.result ? '대국 종료' : `${colorName(next.decisionActor)}의 선택`;
  }
  async function startGame() {
    if (busy || engineStatus !== 'ready' || (playMode === 'human-ai' && bot.status !== 'ready')) return;
    if (!Number.isInteger(seed) || seed < 0 || seed > 0xffff_ffff) {
      diagnostic(new Error('시드는 0부터 4294967295까지의 정수여야 합니다.'), 'new-game');
      return;
    }
    const epoch = ++requestEpoch;
    busy = true;
    activity = '새 게임 생성 중';
    try {
      const next = await client.newGame({ gameStyle }, seed, humanColor);
      if (epoch === requestEpoch) await publish(next, epoch);
    } catch (error) { if (epoch === requestEpoch) diagnostic(error, 'new-game'); }
    finally { if (epoch === requestEpoch) busy = false; }
  }
  async function apply(intent: PublicIntent) {
    if (!canAct) return;
    const epoch = ++requestEpoch;
    busy = true;
    activity = '엔진에서 행동 검증 중';
    try {
      const next = await client.applyIntent(intent);
      if (epoch === requestEpoch) await publish(next, epoch);
    } catch (error) {
      if (epoch === requestEpoch) {
        diagnostic(error, 'apply-intent');
        activity = '행동이 적용되지 않았습니다. 선택을 확인하세요.';
      }
    } finally { if (epoch === requestEpoch) busy = false; }
  }
  function cancel() {
    ++requestEpoch;
    client.cancelPending();
    const cancellation = client.diagnostics.at(-1);
    if (cancellation?.code === 'cancelled') diagnostics = [...diagnostics.slice(-99), cancellation];
    busy = false;
    clearSelection();
    activity = '요청 취소';
  }
  async function refreshPerspective() {
    if (!snapshot || busy || playMode !== 'manual') return;
    const epoch = ++requestEpoch;
    busy = true;
    try {
      const next = await client.observe(snapshot.decisionActor);
      if (epoch === requestEpoch) await publish(next, epoch);
    } catch (error) { if (epoch === requestEpoch) diagnostic(error, 'observe'); }
    finally { if (epoch === requestEpoch) busy = false; }
  }
  function visibleOrigins(at: Square): VisibleOrigin[] {
    const choices: VisibleOrigin[] = [];
    const physical = observation?.board[at.row]?.[at.col];
    if (physical && (physical.color === viewer || physical.type === 'football')) {
      choices.push({ kind: 'physical', at: anchorSquare(physical, at), piece: physical });
    }
    // Overlay cells and glyphs are already projected for this viewer. Never
    // correlate them with a physical origin or a hidden engine identity.
    for (const overlay of records(publicState.overlays)) {
      if (overlay.kind !== 'quantum' || !Array.isArray(overlay.cells)
        || !overlay.cells.some((cell) => sameSquare(square(cell), at))) continue;
      const piece = record(overlay.piece);
      const anchor = square(overlay.cells[0]);
      if (piece.color === viewer && anchor) choices.push({ kind: 'quantum', at: anchor, piece });
    }
    return choices;
  }
  function selectOrigin(choice: VisibleOrigin) {
    clearSelection();
    selectedSquare = { ...choice.at };
    selectedGhost = choice.kind === 'quantum' ? choice.piece : null;
    activity = choice.kind === 'quantum'
      ? `공개 양자 잔상 ${squareName(choice.at)} 선택 · 목적지를 직접 지정하세요.`
      : `${pieceName(choice.piece)} ${squareName(choice.at)} 선택`;
  }
  function clickSquare(at: Square) {
    if (!canAct || mode !== 'play' || selectionPhase.kind) return;
    if (selectedCard) {
      if (!slot || slot.kind === 'choice') return;
      const chosen = { ...at };
      if (slot.kind === 'squares') {
        const previous = Array.isArray(targetValues[slot.key]) ? targetValues[slot.key] as Square[] : [];
        if (previous.length >= (form?.maxSelections ?? 64)) {
          diagnostic(new Error(`이 입력은 최대 ${form?.maxSelections ?? 64}개 좌표까지 지정할 수 있습니다. 마지막 선택을 취소한 뒤 수정하세요.`), 'card-selection');
          return;
        }
        targetValues = { ...targetValues, [slot.key]: [...previous, chosen] };
      } else {
        targetValues = { ...targetValues, [slot.key]: chosen };
        if (activeSlot + 1 < (form?.slots.length ?? 0)) activeSlot++;
      }
      return;
    }
    if (wizardSpell && selectedSquare) {
      try { void apply(buildPieceIntent('wizardSpell', actor, selectedSquare, { spellId: wizardSpell, target: at })); }
      catch (error) { diagnostic(error, 'piece-selection'); }
      return;
    }
    if (selectedSquare && (selectedGhost !== null || movementMode !== 'normal' || highlights.some((hint) => sameSquare(hint, at)))) {
      try { void apply(buildMoveIntent(actor, selectedSquare, at, movementMode)); }
      catch (error) { diagnostic(error, 'move-selection'); }
      return;
    }
    const choices = visibleOrigins(at);
    if (choices.length > 1) {
      clearSelection();
      originChoices = choices;
      activity = `${squareName(at)}에 겹친 공개 표현을 선택하세요.`;
    } else if (choices.length === 1) {
      if (choices[0].kind === 'physical' && selectedGhost === null && sameSquare(selectedSquare, choices[0].at)) clearSelection();
      else selectOrigin(choices[0]);
    } else clearSelection();
  }
  function selectCard(card: CardView) {
    clearSelection();
    selectedCard = card;
    try {
      const description = getCardSelectionForm(card.id, { cards });
      targetValues = Object.fromEntries(description.slots.filter((field) => field.kind === 'choice' && field.choices?.length === 1)
        .map((field) => [field.key, field.choices![0].value]));
      const nextSlot = description.slots.findIndex((field) => targetValues[field.key] === undefined);
      activeSlot = Math.max(0, nextSlot);
    } catch (error) { diagnostic(error, 'card-selection'); }
  }
  function useCard() {
    if (!selectedCard) return;
    try { void apply(buildCardIntent(selectedCard, actor, targetValues)); }
    catch (error) { diagnostic(error, 'card-selection'); }
  }
  function selectChoice(key: string, encodedValue: string, index: number) {
    const option = form?.slots[index]?.choices?.find((choice) => JSON.stringify(choice.value) === encodedValue);
    if (!option) return;
    targetValues = { ...targetValues, [key]: option.value };
    if (index + 1 < (form?.slots.length ?? 0)) activeSlot = index + 1;
  }
  function specialPieceAction(kind: 'shotgunReload' | 'promotion' | 'fileSurgeSkip') {
    if (!selectedSquare) return;
    try { void apply(buildPieceIntent(kind, actor, selectedSquare)); }
    catch (error) { diagnostic(error, 'piece-selection'); }
  }
  function pickDraft(card: CardView, index: number) {
    if (draft.kind === 'chaos') {
      const bundleIndex = Math.floor(index / 2);
      const bundle = offers.slice(bundleIndex * 2, bundleIndex * 2 + 2);
      void apply({ type: 'draftBundlePick', color: actor, bundleIndex, cardInstanceIds: bundle.map((choice) => choice.instanceId) });
    } else void apply({ type: 'draftPick', color: actor, cardInstanceId: card.instanceId });
  }
  function downloadReport() {
    if (!snapshot || !info) return;
    try { exportPublicReport(snapshot, info, [...diagnostics, ...snapshot.diagnostics]); }
    catch (error) { diagnostic(error, 'public-report'); }
  }
  onMount(() => {
    let active = true;
    client.initialize().then((ready) => {
      if (!active) return;
      if (ready.rulesVersion !== catalogProvenance.rulesVersion || ready.catalogVersion !== catalogProvenance.catalogVersion) {
        throw fault('presentation_contract_mismatch',
          `화면과 WASM의 공개 계약 버전이 다릅니다. rulesVersion expected=${catalogProvenance.rulesVersion}, actual=${ready.rulesVersion}; catalogVersion expected=${catalogProvenance.catalogVersion}, actual=${ready.catalogVersion}`,
          'initialize');
      }
      info = ready;
      diagnostics = [...diagnostics, ...(ready.diagnostics ?? [])].slice(-100);
      engineStatus = 'ready';
      activity = '엔진 준비 완료';
    }).catch((error) => {
      if (!active) return;
      engineStatus = 'error';
      activity = '엔진 초기화 실패';
      diagnostic(error, 'initialize');
    });
    return () => { active = false; ++requestEpoch; client.dispose(); };
  });
</script>

<div class="app-shell">
  <header class="masthead">
    <a class="brand" href="./" aria-label="Augment Chess 엔진 시험 예제 홈"><span class="brand-symbol" aria-hidden="true">A<span>+</span></span><span>Augment Chess<small>ENGINE LAB</small></span></a>
    <div class="header-meta"><span class="pill" class:failure={engineStatus === 'error'}><span class="status-dot" class:ready={engineStatus === 'ready'}></span>{engineStatus === 'ready' ? 'Rust 엔진 준비' : engineStatus === 'loading' ? '엔진 로딩' : '엔진 오류'}</span><span class="pill muted">브라우저 실행</span></div>
  </header>

  <section class="intro"><div><span class="eyebrow">STATIC SPACE · PUBLIC ENGINE TEST</span><h1>체스에 새로운 한 수를.</h1><p>카드를 선택하고 보드를 움직여 동결 v7 엔진의 대국 흐름을 확인하세요.</p></div><span class="version-tag">v7 <span>FROZEN RULES</span></span></section>

  <section class="setup panel" aria-label="새 대국 설정">
    <label>게임 스타일<select bind:value={gameStyle} disabled={busy}><option value="normal">일반 · Normal</option><option value="chaos">혼돈 · Chaos</option><option value="grand">그랜드 · Grand</option></select></label>
    <label>대국 모드<select bind:value={playMode} disabled={busy}><option value="manual">수동 시험 · 양쪽 조작</option><option value="human-ai" disabled={bot.status !== 'ready'}>사람 대 AI · {bot.status === 'ready' ? '준비' : '준비 대기'}</option></select></label>
    <label>시작 관점<select bind:value={humanColor} disabled={busy}><option value="white">백</option><option value="black">흑</option></select></label>
    <label class="seed-field">시드<input type="number" min="0" max="4294967295" step="1" bind:value={seed} disabled={busy} /></label>
    <button class="primary" type="button" onclick={startGame} disabled={engineStatus !== 'ready' || busy || (playMode === 'human-ai' && bot.status !== 'ready')}>새 게임 <span aria-hidden="true">↗</span></button>
  </section>
  <div class="ai-readiness"><span class="pill tiny">AI {bot.status}</span><span class="pill tiny">모델 model-missing</span><span>{bot.message}</span></div>

  {#if lastError}<div class="error-banner" role="alert"><strong>오류</strong><span>{lastError}</span><button class="text-button" onclick={() => lastError = ''} aria-label="오류 알림 닫기">×</button></div>{/if}

  <main class="game-layout">
    <section class="game-column" aria-label="보드와 기보">
      <div class="player-strip panel"><span class="player-avatar black" aria-hidden="true">♟</span><div><strong>{colorName(viewer === 'white' ? 'black' : 'white')} · 상대</strong><small>공개 획득 카드 {acquiredOpponent.length}장</small></div><span class="score">★ {numeric(publicState.opponentStarTotal)}</span></div>
      <div class="board-heading"><span class="pill turn">{snapshot?.result ? `${snapshot.result === 'draw' ? '무승부' : `${colorName(snapshot.result)} 승리`}` : snapshot ? `${colorName(actor)} 차례` : '대국 준비'}</span><span class="activity" aria-live="polite">{activity}</span>{#if busy}<button class="text-button" onclick={cancel}>취소</button>{:else if snapshot && !snapshot.result && playMode === 'manual' && viewer !== actor}<button class="text-button" onclick={refreshPerspective}>다음 관점 불러오기</button>{/if}</div>
      <Board {observation} {viewer} selected={selectedSquare} {highlights} targets={chosenTargets} disabled={!canAct} onSquare={clickSquare} />
      <div class="player-strip panel own-player"><span class="player-avatar" aria-hidden="true">♙</span><div><strong>{colorName(viewer)} · 현재 관점</strong><small>{playMode === 'manual' ? '수동 시험: 다음 결정자의 공개 관점으로 전환' : '사람의 공개 관점 고정'}</small></div><span class="score">★ {numeric(publicState.ownStarTotal)}</span></div>

      {#if originChoices.length}
        <section class="piece-controls panel" role="group" aria-label="겹친 공개 기물 표현 선택">
          <div><strong>어느 표현을 선택할까요?</strong><p class="small-note">같은 칸에 보이는 기물과 양자 잔상을 구분해 선택하세요.</p></div>
          {#each originChoices as choice, index}
            <button class="secondary small" disabled={!canAct} onclick={() => selectOrigin(choice)}>
              {choice.kind === 'quantum' ? '양자 잔상' : '보드 기물'} · {pieceName(choice.piece)} · {squareName(choice.at)}{originChoices.filter((option) => option.kind === choice.kind).length > 1 ? ` · ${index + 1}번 표현` : ''}
            </button>
          {/each}
          <button class="text-button" onclick={clearSelection}>선택 취소</button>
        </section>
      {/if}
      {#if selectedGhost}
        <p class="small-note">공개 양자 잔상 {selectedSquare ? squareName(selectedSquare) : ''} 선택: 목적지 칸을 직접 클릭하세요. 이동 가능 여부는 엔진이 확인합니다.</p>
      {/if}

      {#if selectedPiece}<section class="piece-controls panel"><div><strong>{pieceName(selectedPiece)} · {selectedSquare ? squareName(selectedSquare) : ''}</strong><p class="small-note">{pieceStatus(selectedPiece).join(' · ') || '표시된 상태 효과 없음'}</p></div><label>이동 방식<select bind:value={movementMode}>{#each getPublicMoveModes() as option}<option value={option.value}>{option.label}</option>{/each}</select></label>{#if selectedPiece.type === 'shotgunKing'}<button class="secondary small" disabled={!canAct} onclick={() => specialPieceAction('shotgunReload')}>재장전 요청</button>{/if}{#if selectedPiece.type === 'wizard'}<label>마법 선택<select bind:value={wizardSpell}><option value="">이동</option>{#each wizardSpellChoices as option}<option value={option.value}>{option.label}</option>{/each}</select></label>{/if}{#if selectedPiece.type === 'pawn'}<button class="secondary small" disabled={!canAct} onclick={() => specialPieceAction('promotion')}>특수 승급 요청</button>{/if}<button class="secondary small" disabled={!canAct} onclick={() => specialPieceAction('fileSurgeSkip')}>추가 이동 건너뛰기 요청</button><button class="text-button" onclick={clearSelection}>선택 해제</button><p class="small-note">메뉴는 공개 입력 방식입니다. 현재 위치에서 가능한지는 엔진이 확인합니다.</p></section>{/if}

      <section class="history-panel panel"><div class="section-heading"><h2>공개 기보</h2><span class="count">{history.length}</span><button class="text-button" onclick={downloadReport} disabled={!snapshot}>조사 자료 ↓</button></div>{#if history.length}<ol class="history-list">{#each history as event, index}<li><span class="move-number">{index + 1}</span><span>{historySummary(event)}</span></li>{/each}</ol>{:else}<p class="empty-note">확정된 공개 전이가 여기에 기록됩니다.</p>{/if}</section>
    </section>

    <aside class="cards-column" aria-label="카드와 선택">
      <section class="panel action-panel"><div class="section-heading"><h2>{mode === 'draft' ? '카드 드래프트' : selectionPhase.kind ? '진행 중인 선택' : '내 카드'}</h2><span class="count">{text(publicState.phase, 'OPENING')}</span></div>
        {#if selectionPhase.kind === 'promotion'}<p class="small-note">승급할 기물을 선택하세요.</p><div class="choice-grid">{#each (Array.isArray(selectionPhase.choices) ? selectionPhase.choices : []) as choice}<button class="secondary" disabled={!canAct} onclick={() => apply({ type: 'promotionChoice', color: actor, promotionType: text(choice) })}>{pieceName({ type: choice })}</button>{/each}</div>
        {:else if selectionPhase.kind === 'trolley'}<p class="small-note">선택한 쪽의 기물이 제거됩니다. 공개된 선택을 확인하세요.</p><div class="choice-grid">{#each (Array.isArray(selectionPhase.choices) ? selectionPhase.choices : []) as choice, index}<button class="secondary trolley-choice" disabled={!canAct} onclick={() => apply({ type: 'trolleyChoice', color: actor, doomedIndex: index })}><strong>{index + 1}번 선택</strong><span>{records(choice).map((piece) => `${colorName(piece.color)} ${pieceName(piece)}`).join(', ') || '기물 없음'}</span></button>{/each}</div>
        {:else if mode === 'draft'}<p class="small-note">{draft.kind === 'chaos' ? '인접한 두 카드가 한 묶음으로 선택됩니다.' : `${colorName(actor)}이 카드를 고릅니다.`}</p>{#if offers.length}<div class="card-grid draft-grid">{#each offers as card, index}<Card {card} disabled={!canAct} onSelect={() => pickDraft(card, index)} />{/each}</div>{:else}<p class="empty-note">이 관점에는 공개된 드래프트 제안이 없습니다.</p>{/if}
        {:else if cards.length}<div class="card-grid">{#each cards as card}<Card {card} selected={selectedCard?.instanceId === card.instanceId} disabled={!canAct || card.used || card.recovering} onSelect={() => selectCard(card)} />{/each}</div>
        {:else}<div class="empty-state"><span aria-hidden="true">✦</span><strong>카드 선택을 기다립니다</strong><p>새 대국을 시작하면 공개 제안과 획득 카드가 표시됩니다.</p></div>{/if}

        {#if selectedCard}
          <div class="selection-builder">
            <h3>{selectedCard.name}</h3><p>{selectedCard.help || selectedCard.text}</p>
            {#if form}
              <p class="small-note">{form.label}</p>
              {#each form.slots as field, index}
                <div class="target-slot" class:active={activeSlot === index}>
                  <button type="button" class="slot-title" onclick={() => activeSlot = index}><span>{index + 1}</span>{field.label}</button>
                  {#if field.kind === 'choice'}
                    <select aria-label={field.label} value={JSON.stringify(targetValues[field.key]) ?? ''} onchange={(event) => selectChoice(field.key, event.currentTarget.value, index)}>
                      <option value="">선택하세요</option>
                      {#each field.choices ?? [] as choice}<option value={JSON.stringify(choice.value)}>{choice.label}</option>{/each}
                    </select>
                  {:else}
                    <p class="selected-coordinates">{Array.isArray(targetValues[field.key]) ? (targetValues[field.key] as Square[]).map(squareName).join(' → ') : square(targetValues[field.key]) ? squareName(square(targetValues[field.key])!) : '보드에서 선택'}</p>
                    {#if field.kind === 'squares'}
                      <p class="small-note">{form.minSelections ?? 1}~{form.maxSelections ?? 64}개를 순서대로 지정</p>
                      {#if Array.isArray(targetValues[field.key])}<button class="text-button" onclick={() => targetValues = { ...targetValues, [field.key]: (targetValues[field.key] as Square[]).slice(0, -1) }}>마지막 선택 취소</button>{/if}
                    {/if}
                  {/if}
                </div>
              {/each}
            {/if}
            <div class="button-row"><button class="primary small" disabled={!canAct || !completeTargets} onclick={useCard}>선택 적용 요청</button><button class="secondary small" onclick={clearSelection}>카드 선택 취소</button></div>
            <p class="small-note">처음 선택할 대상은 공개 힌트로 표시합니다. 이후 대상과 순서는 직접 지정하며, 완료된 선택은 엔진이 검증한 뒤 적용합니다.</p>
          </div>
        {/if}
      </section>

      <section class="panel opponent-library"><div class="section-heading"><h2>상대 공개 카드</h2><span class="count">{acquiredOpponent.length}</span></div>{#if acquiredOpponent.length}<div class="card-grid">{#each acquiredOpponent as card}<Card {card} compact />{/each}</div>{:else}<p class="empty-note">상대가 획득한 공개 카드가 여기에 표시됩니다.</p>{/if}</section>
      {#if Array.isArray(publicState.ruleCardIds) && publicState.ruleCardIds.length}<section class="panel"><h2>적용 규칙</h2><div class="rule-chips">{#each publicState.ruleCardIds as rule}<span class="pill tiny">{text(rule)}</span>{/each}</div></section>{/if}
      <p class="source-note">원본 게임 <a href="https://augmentchess.org/" target="_blank" rel="noreferrer">Augment Chess ↗</a>의 동결 규칙과 공개 카탈로그를 사용합니다. 그림과 화면은 예제를 위해 새로 제작했습니다.</p>
    </aside>
  </main>

  <details class="diagnostics panel"><summary><span>개발 진단</span><span class="count">{diagnostics.length + (snapshot?.diagnostics?.length ?? 0)}</span><span class="small-note">버전 · 공개 관측 · 경고와 오류</span></summary><div class="diagnostic-content"><div class="diagnostic-versions"><h3>버전 정보</h3><pre>{JSON.stringify({ engine: info, presentation: catalogProvenance, ai: bot }, null, 2)}</pre></div><div><h3>경고와 오류</h3>{#if diagnostics.length || snapshot?.diagnostics?.length}<pre>{JSON.stringify([...diagnostics, ...(snapshot?.diagnostics ?? [])], null, 2)}</pre>{:else}<p class="small-note">현재 기록된 경고나 오류가 없습니다.</p>{/if}</div><details><summary>현재 관점의 공개 관측 JSON</summary><pre>{JSON.stringify(observation, null, 2)}</pre></details><p class="small-note">내보내기는 공개 화면과 조작 흐름의 조사 자료입니다. 비공개 전체 상태의 완전 재현 파일을 제공하지 않습니다.</p><button class="secondary small" onclick={downloadReport} disabled={!snapshot || !info}>공개 조사 자료 내려받기</button></div></details>
  <footer><span>Accelerate · Browser Engine Demo</span><span>게임 {snapshot?.gameId ?? '—'} · revision {snapshot?.revision ?? '—'}</span></footer>
</div>
