"use strict";

// Read-only accounting of encoded source state. Mirror canonical() charges
// without its limit. The report contains sizes and field names, no state values.
function measure(root) {
  const size = { nodes: 0, bytes: 0 };
  const stack = [root];
  while (stack.length) {
    const value = stack.pop();
    size.nodes++;
    size.bytes += 8;
    if (typeof value === "string") {
      size.nodes++;
      size.bytes += 8 + Buffer.byteLength(value, "utf8");
    } else if (Array.isArray(value)) {
      for (let index = value.length - 1; index >= 0; index--) stack.push(value[index]);
    } else if (value && typeof value === "object") {
      for (const [key, child] of Object.entries(value)) stack.push(child, key);
    }
  }
  return size;
}

function replayGrowth(state, context) {
  const events = state.replayEvents || [];
  if (!Array.isArray(events)) throw new TypeError("replayEvents must be an array");
  const replay = measure(events);
  const deltas = events.map(event => measure(event.delta ?? null));
  const last = events.at(-1);
  const largest = deltas.reduce((result, size, index) =>
    size.nodes > result.nodes ? { index, nodes: size.nodes, bytes: size.bytes } : result,
  { index: null, nodes: 0, bytes: 0 });
  return {
    ...context,
    replayEvents: { count: events.length, ...replay,
      latestEvent: last ? measure(last) : null,
      latestDelta: deltas.at(-1) || null,
      largestDelta: largest,
      deltaNodes: deltas.reduce((sum, size) => sum + size.nodes, 0),
      boardCellChanges: events.reduce((sum, event) => sum + (event.delta?.board?.cells?.length || 0), 0),
      fieldChanges: events.reduce((sum, event) => sum + (event.delta?.fields?.length || 0), 0),
      latestFieldKeys: (last?.delta?.fields || []).map(field => field.key).filter(key => typeof key === "string"),
    },
    boardHistory: measure(state.boardHistory || []),
    notationTimeline: measure(state.notationTimeline || []),
    replayTailFrame: measure(state.replayTailFrame ?? null),
    moveReplay: measure(state.moveReplay ?? null),
  };
}

module.exports = { measure, replayGrowth };
