# Review calibration (groundwork for win-probability based move ratings)

`fit-winprob.js <data.jsonl>` fits how the engine's search score maps to the real chance of winning
(win = 1, draw = 0.5), from self-play positions and their game outcomes.

## Findings (2026-09-19)

| data | positions | fitted k (larger = flatter) | notes |
|---|---|---|---|
| round 2 (mostly depth 1) | 106,210 | >= 4000 (grid ceiling) | the score barely predicts the outcome |
| round 3 snapshot (depth 2-6) | 15,974 | 3450 overall; **2350 at depth 4, 2150 at depth 5** | deeper search = clearly more predictive (MSE 26% below baseline at depth 4 vs 10% at depth 2) |

- Self-play outcomes are noisy (half the games are draws, both sides play imperfectly), so the curve is very flat:
  between -1500 and +1500 the win chance only moves from 0.44 to 0.53.
- A fixed win-probability loss therefore maps to roughly constant score gaps (k = 3450): 2% = 280, 5% = 690, 10% = 1400, 20% = 2900.
  The review's current thresholds (excellent 150, good 350, inaccuracy 700, mistake 1500) already sit at about 1% / 2.5% / 5% / 10%,
  i.e. they are reasonable; switching to win-probability alone would change little with outcome-based calibration.
- Consequence: threshold VALUES are not the weak point. What would help is (a) context (already-won positions), (b) a confidence
  tag from the depth reached, and (c) calibrating against a stronger oracle (a deeper search or Stockfish on plain positions)
  instead of noisy self-play outcomes.
