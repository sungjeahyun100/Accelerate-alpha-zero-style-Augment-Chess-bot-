#!/usr/bin/env node
// Differential test runner: feeds oracle fixtures to a CANDIDATE implementation (e.g. the Rust
// engine) and compares its answers with the oracle's expected answers.
//
//   node run-differential.js --fixtures=oracle-v1.jsonl.gz --candidate="<command>"
//        [--source=card:] [--limit=200] [--stop-on-fail] [--report=report.json]
//
// --candidate is a shell command that speaks the line protocol on stdin/stdout:
//   in  (one JSON per line) {"id","color","state":{...},"actions":[raw action, ...]}
//   out (one JSON per line) {"id","legalActions":[normalised action keys, sorted],
//                            "applied":[{"ok":true|false,"signature":{...}} ...]}   (same order as "actions")
// The oracle server itself is a valid candidate (use it to check the harness and fixtures):
//   --candidate="node tools/fixtures/generate-fixtures.js --serve"
//
// Normalised action key = the action as JSON with the random ids ("id", "instanceId", "pieceId")
// removed at every depth, keys in the order the oracle produced them. Signature = the object
// documented in docs/PORTING-GUIDE.md. Exit code 0 = every fixture matched, 1 = mismatches.
const fs = require("fs");
const zlib = require("zlib");
const { spawn } = require("child_process");
const readline = require("readline");

const arg = (name, def) => {
  const a = process.argv.slice(2).find((x) => x === "--" + name || x.startsWith("--" + name + "="));
  if (!a) return def;
  return a.includes("=") ? a.slice(a.indexOf("=") + 1) : true;
};
const file = arg("fixtures", null), cmd = arg("candidate", null);
if (!file || !cmd) { console.error('usage: node run-differential.js --fixtures=<file> --candidate="<command>" [--source=prefix] [--limit=N] [--stop-on-fail] [--report=file]'); process.exit(2); }
const sourceFilter = arg("source", ""), limit = Number(arg("limit", 0)), stopOnFail = !!arg("stop-on-fail", false), reportFile = arg("report", null);

const raw = fs.readFileSync(file);
const text = (file.endsWith(".gz") ? zlib.gunzipSync(raw) : raw).toString("utf8");
let fixtures = text.split("\n").filter(Boolean).map((l) => JSON.parse(l)).filter((f) => f.source.startsWith(sourceFilter));
if (limit) fixtures = fixtures.slice(0, limit);

const child = spawn(cmd, { shell: true, stdio: ["pipe", "pipe", "inherit"] });
const rl = readline.createInterface({ input: child.stdout });
const waiting = [];
rl.on("line", (line) => { const w = waiting.shift(); if (w) w(line); });
child.on("exit", (code) => { while (waiting.length) waiting.shift()(null); });
const ask = (req) => new Promise((resolve) => { waiting.push(resolve); child.stdin.write(JSON.stringify(req) + "\n"); });

(async () => {
  const kinds = {};
  const bad = [];
  let done = 0;
  const t0 = Date.now();
  for (const f of fixtures) {
    const line = await ask({ id: f.id, color: f.color, state: f.state, actions: f.expected.applied.map((a) => a.action) });
    done++;
    let problem = null;
    let res = null;
    try { res = line === null ? null : JSON.parse(line); } catch (e) { problem = "candidate answer is not JSON"; }
    if (line === null) problem = "candidate exited";
    else if (res && res.error) problem = "candidate error: " + res.error;
    else if (res) {
      const exp = f.expected;
      const got = res.legalActions || [];
      if (got.length !== exp.legalActions.length || got.some((k, i) => k !== exp.legalActions[i])) {
        const eset = new Set(exp.legalActions), gset = new Set(got);
        const missing = exp.legalActions.filter((k) => !gset.has(k)), extra = got.filter((k) => !eset.has(k));
        problem = `legal actions differ: expected ${exp.legalActions.length}, got ${got.length}; missing ${missing.length}${missing[0] ? " e.g. " + missing[0].slice(0, 120) : ""}; extra ${extra.length}${extra[0] ? " e.g. " + extra[0].slice(0, 120) : ""}`;
      } else {
        for (let i = 0; i < exp.applied.length; i++) {
          const e = exp.applied[i], g = (res.applied || [])[i];
          if (!g) { problem = `applied[${i}] missing`; break; }
          if (!!g.ok !== e.ok) { problem = `applied[${i}] ok differs: expected ${e.ok}, got ${g.ok} (action ${e.key.slice(0, 100)})`; break; }
          if (e.ok && JSON.stringify(g.signature) !== JSON.stringify(e.signature)) {
            const ks = Object.keys(e.signature).filter((k) => JSON.stringify(e.signature[k]) !== JSON.stringify(g.signature && g.signature[k]));
            problem = `applied[${i}] result differs in [${ks.join(",")}] (action ${e.key.slice(0, 100)})`;
            break;
          }
        }
      }
    }
    if (problem) {
      const kind = f.source.split(":").slice(0, 2).join(":");
      kinds[kind] = (kinds[kind] || 0) + 1;
      bad.push({ id: f.id, source: f.source, problem });
      if (bad.length <= 10) console.log(`FAIL ${f.id} [${f.source}] ${problem}`);
      if (stopOnFail) break;
    }
  }
  child.stdin.end();
  const secs = ((Date.now() - t0) / 1000).toFixed(1);
  console.log(`\n${done} fixtures run in ${secs}s: ${done - bad.length} matched, ${bad.length} mismatched`);
  const worst = Object.entries(kinds).sort((a, b) => b[1] - a[1]).slice(0, 12);
  if (worst.length) console.log("most mismatches by source: " + worst.map(([k, v]) => `${k}=${v}`).join("  "));
  if (reportFile) fs.writeFileSync(reportFile, JSON.stringify({ fixtures: done, matched: done - bad.length, mismatched: bad.length, bySource: kinds, failures: bad }, null, 2));
  process.exit(bad.length ? 1 : 0);
})();
