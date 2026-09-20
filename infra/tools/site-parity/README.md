# site-parity: differential tests against the site's real AI worker

```
node fetch-real-worker.js [--force]          # download live aiWorker.js -> .cache/real-worker.js (exports generateActions/applyAction/...)
node parity-actions.js  [engine|-] [N=400] [seed=12345]          # generateActions parity on random sparse boards (want: differing=0)
node parity-apply.js    [engine|-] [N=300] [seed=777]            # same random action applied in both, resulting state compared
node parity-playout.js  [engine|-] [GAMES=30] [seed=4242] [PLIES=40]   # multi-ply playouts, stops at first divergence per game
node check-site-update.js [--save]           # CHANGED/UNCHANGED vs last-seen.json (bundle name + worker SHA-256); exit 0, or 2 on network error
```

`engine` defaults to `../../engine-merged.js` (`-` = default). Actions are matched between engines by normalized content
(random ids stripped), never by list index. Run `fetch-real-worker.js --force` first whenever `check-site-update.js` says CHANGED.

Automation: `.github/workflows/site-watch.yml` runs daily (and on demand). It runs `check-site-update.js`; on CHANGED it runs
`fetch-real-worker.js --force` and the three parity scripts (200/150/15), writing a report to the step summary and a 30-day artifact
`site-watch-report`. Read-only: it never commits, opens issues, or updates `last-seen.json` (do `check-site-update.js --save` yourself after
reviewing). Network errors only produce a warning.

Caveats when reading multi-ply diffs:
- The site's worker does NOT model some cards. Example: `locustSwarm` is an OPENING card; the worker only honours it via its
  opening-setup path, so playing it as an in-game card has no effect there, while our engine applies it. Diffs after such cards are expected.
- Card effects that pick random targets (brutus rook, freeze, ...) are not RNG-aligned between the two code bases; `Math.random` is
  not seeded by these scripts. Treat isolated one-piece differences after such cards as noise.
- The worker only tracks parrot memory (`state.parrotMovement`) if the state already carries it; see TRIAGE.md.
- Test against a clean copy when the engine is mid-edit: `git show HEAD:engine-merged.js > .cache/engine-head.js`.

See TRIAGE.md for the 2026-09-19 classification of every divergence found.
