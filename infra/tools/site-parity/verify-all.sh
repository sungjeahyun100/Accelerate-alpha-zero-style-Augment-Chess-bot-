#!/bin/bash
# quick regression bundle: parity + smoke + perf equivalence
cd "$(dirname "$0")/../.." || exit 1
node tools/site-parity/parity-actions.js - 400 | head -1
node tools/site-parity/parity-apply.js - 300 | head -1
node tools/site-parity/parity-playout.js - 60 4242 40 | head -1 | cut -c1-400
node smoke-merged.js 2>&1 | tail -1
NOSAVE=1 node tools/perf/eval-equiv.js 600 2>&1 | grep checked
node tools/perf/eq-exotic.js 2>&1 | tail -1
