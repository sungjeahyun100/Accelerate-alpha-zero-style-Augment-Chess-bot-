// Experiment-factory driver around match.yml (needs the gh CLI, logged in).
//   node tools/lab/lab.js dispatch <candidates.json>   start one match per candidate
//   node tools/lab/lab.js collect [runIds...]          download match-verdict artifacts of finished match runs
//                                                       and append them to docs/results/results.jsonl
//   node tools/lab/lab.js status                       list recent match runs
//
// candidates.json: { "baseline": "handcoded", "defaults": {"pairs_per_shard":"16","ms":"300","handicap":"1"},
//                    "candidates": [ {"name":"resid300","model":"data:models/resid300-r3b.json@hybrid300", "inputs":{...}} ] }
// A candidate plays as model_a against the baseline (model_b). Every dispatch is one run of match.yml
// (4 shards), so keep the batch small enough for the 20-concurrent-job limit (<= 3 candidates at once).
const { execFileSync } = require("child_process");
const fs = require("fs");
const path = require("path");
const GH = process.env.GH || (process.platform === "win32" ? "C:/Program Files/GitHub CLI/gh.exe" : "gh");
const gh = (args, input) => execFileSync(GH, args, { encoding: "utf8", input, maxBuffer: 64 * 1024 * 1024 });
const RESULTS = path.join(__dirname, "..", "..", "docs", "results", "results.jsonl");

const [cmd, ...rest] = process.argv.slice(2);
if (cmd === "dispatch") {
  const spec = JSON.parse(fs.readFileSync(rest[0], "utf8"));
  for (const c of spec.candidates) {
    const inputs = { model_a: c.model, model_b: spec.baseline, ...(spec.defaults || {}), ...(c.inputs || {}) };
    for (const k of Object.keys(inputs)) inputs[k] = String(inputs[k]);
    console.log(gh(["workflow", "run", "match.yml", "--json"], JSON.stringify(inputs)).trim(), "<-", c.name);
  }
} else if (cmd === "status") {
  for (const r of JSON.parse(gh(["run", "list", "--workflow", "match.yml", "--limit", "10", "--json", "databaseId,status,conclusion,createdAt"]))) console.log(r.databaseId, r.status, r.conclusion, r.createdAt);
} else if (cmd === "collect") {
  let ids = rest;
  if (!ids.length) ids = JSON.parse(gh(["run", "list", "--workflow", "match.yml", "--limit", "20", "--json", "databaseId,status"])).filter((r) => r.status === "completed").map((r) => String(r.databaseId));
  fs.mkdirSync(path.dirname(RESULTS), { recursive: true });
  const seen = new Set(fs.existsSync(RESULTS) ? fs.readFileSync(RESULTS, "utf8").split("\n").filter(Boolean).map((l) => String(JSON.parse(l).run)) : []);
  const tmp = fs.mkdtempSync(path.join(require("os").tmpdir(), "lab-"));
  for (const id of ids) {
    if (seen.has(String(id))) continue;
    try {
      const dir = path.join(tmp, String(id));
      gh(["run", "download", String(id), "-n", "match-verdict", "-D", dir]);
      const v = JSON.parse(fs.readFileSync(path.join(dir, "verdict.json"), "utf8"));
      fs.appendFileSync(RESULTS, JSON.stringify(v) + "\n");
      console.log(id, v.verdict, `${(v.aWinRate * 100).toFixed(1)}% of ${v.decisive} decisive`, v.modelA, "vs", v.modelB);
    } catch (e) {
      console.log(id, "no verdict artifact (older run or failed)");
    }
  }
} else {
  console.log("usage: node tools/lab/lab.js dispatch <candidates.json> | collect [runIds] | status");
  process.exit(1);
}
