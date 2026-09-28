"use strict";

const { FrozenClientSource, cacheRoot, verify } = require("./frozen-client-source");
const { OracleRuntime, createHeadlessProfile } = require("./game-adapter");

class ActionCursor {
  #runtime;
  #stream;
  #disposed = false;

  constructor(settings, position, options = {}) {
    this.#runtime = new OracleRuntime(settings);
    this.#stream = this.#runtime.actionStream(position, options);
  }

  nextPage(limit = 256, options = {}) {
    if (this.#disposed) throw new Error("Action cursor is disposed.");
    try {
      return this.#stream.nextPage(limit, options);
    } catch (error) {
      // A source exception may have left a paused generator or VM inconsistent.
      this.dispose();
      throw error;
    }
  }

  dispose() {
    this.#disposed = true;
    this.#stream = null;
    this.#runtime = null;
  }
}

class GameAdapter {
  #settings;
  #runtime;
  #disposed = false;

  constructor({ source, contract, maxCandidates = 100000, maxMicrotasks = 256 } = {}) {
    this.#settings = { source, contract, maxCandidates, maxMicrotasks };
    this.#runtime = new OracleRuntime(this.#settings);
  }

  #call(method, ...args) {
    if (this.#disposed) throw new Error("Game adapter is disposed.");
    try {
      const answer = this.#runtime[method](...args);
      // A rejected source transition can still mutate transient VM state.
      if (method === "apply" && !answer.ok) this.#runtime = new OracleRuntime(this.#settings);
      return answer;
    } catch (error) {
      try {
        this.#runtime = new OracleRuntime(this.#settings);
      } catch (recoveryError) {
        this.dispose();
        throw new AggregateError([error, recoveryError], "Game adapter failed and its runtime could not be rebuilt.");
      }
      throw error;
    }
  }

  newGame(config = {}, seed = 0, tape = []) { return this.#call("newGame", config, seed, tape); }
  observe(position, viewer) { return this.#call("observe", position, viewer); }
  publicHints(position, viewer) { return this.#call("publicHints", position, viewer); }
  actions(position) { return this.#call("actions", position); }
  apply(position, action, options = {}) { return this.#call("apply", position, action, options); }
  result(position) { return this.#call("result", position); }

  actionStream(position, options = {}) {
    if (this.#disposed) throw new Error("Game adapter is disposed.");
    return new ActionCursor(this.#settings, position, options);
  }

  dispose() {
    this.#disposed = true;
    this.#runtime = null;
  }
}

module.exports = { FrozenClientSource, GameAdapter, ActionCursor, cacheRoot, verify, createHeadlessProfile };
