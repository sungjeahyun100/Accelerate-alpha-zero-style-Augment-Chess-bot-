#!/usr/bin/env node
"use strict";
const { OracleRuntime, PRESENTATION_HOOKS, createHeadlessProfile, decisionActor } = require("../../game-adapter/src/game-adapter");
const { FrozenClientSource, ActionCursor, cacheRoot } = require("../../game-adapter/src");
const defaultContract = require("../../../contracts/tools/runtime-contract");
const readline = require("node:readline");

// Keep the old site-parity API and executable path while the implementation
// belongs to the independent game-adapter project.
class OfflineOracle extends OracleRuntime {
  constructor(root = cacheRoot(), { source, contract = defaultContract, maxCandidates = 100000, maxMicrotasks = 256 } = {}) {
    super({ source: source || new FrozenClientSource(root), contract, maxCandidates, maxMicrotasks });
  }
  _spawn() {
    return new OfflineOracle(this.root, {
      source: this.source, contract: this.contract,
      maxCandidates: this.maxCandidates, maxMicrotasks: this.maxMicrotasks,
    });
  }
  actionStream(position, options = {}) {
    return new ActionCursor({
      source: this.source, contract: this.contract,
      maxCandidates: this.maxCandidates, maxMicrotasks: this.maxMicrotasks,
    }, position, options);
  }
}
const HEADLESS_PROFILE = createHeadlessProfile(defaultContract);
module.exports = { OfflineOracle, PRESENTATION_HOOKS, HEADLESS_PROFILE, decisionActor };

if (require.main === module) {
  const root = process.argv[2] || cacheRoot();
  let oracle;
  try { oracle = new OfflineOracle(root); }
  catch (error) { console.error(error.stack); process.exit(2); }
  const input = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });
  input.on("line", line => {
    let request;
    try {
      request = JSON.parse(line);
      const handlers = {
        new_game: () => oracle.newGame(request.config, request.seed, request.tape),
        get_legal_actions: () => oracle.actions(request.position),
        get_public_hints: () => oracle.publicHints(request.position, request.viewer),
        apply_action: () => oracle.apply(request.position, request.action),
        get_result: () => oracle.result(request.position),
        observe: () => oracle.observe(request.position, request.viewer),
      };
      if (!handlers[request.command]) throw new TypeError("Unknown oracle command.");
      process.stdout.write(JSON.stringify({ id: request.id, value: handlers[request.command]() }) + "\n");
    } catch (error) {
      process.stdout.write(JSON.stringify({ id: request?.id ?? null, error: {
        code: error.code || "ORACLE_ERROR", name: error.name, message: error.message,
      } }) + "\n");
    }
  });
}
