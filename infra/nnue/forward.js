// Pure-JS forward pass for the trained net -- no tfjs dependency at runtime,
// just plain arrays, so this is what the actual engine/extension would use
// (bundling all of tfjs into a content script for a 768x32x32x1 network would
// be absurd overkill).
const fs = require("fs");
const path = require("path");

function loadWeights(file) {
  const raw = JSON.parse(fs.readFileSync(file, "utf8"));
  // "Wide & deep" architecture (2026-09-08, see train.js's buildModel):
  // model.getWeights() order follows layer-creation order --
  // [deep1_k, deep1_b, deep2_k, deep2_b, deepOut_k, wideOut_k]. deepOut and
  // wideOut both use useBias:false (matches evaluateState()/tune-eval.js
  // having no additive constant term), so there are only 6 tensors total,
  // not the old 3-Dense Sequential's 6-as-3-kernel-bias-pairs shape -- the
  // last two are BOTH kernels, not a kernel+bias pair, which is why this
  // can't reuse the old raw[4]/raw[5] layout.
  return {
    k1: raw[0], b1: raw[1],
    k2: raw[2], b2: raw[3],
    kDeepOut: raw[4],
    kWideOut: raw[5]
  };
}

function relu(x) {
  return x > 0 ? x : 0;
}
function tanh(x) {
  return Math.tanh(x);
}

// x: Float32Array/array of length inShape, kernel: {shape:[inDim,outDim], data:[...]}, bias: {shape:[outDim], data:[...]} or null for no bias
function denseLayer(x, kernel, bias, activation) {
  const [inDim, outDim] = kernel.shape;
  const out = new Array(outDim).fill(0);
  for (let o = 0; o < outDim; o++) {
    let sum = bias ? bias.data[o] : 0;
    for (let i = 0; i < inDim; i++) {
      sum += x[i] * kernel.data[i * outDim + o];
    }
    out[o] = activation ? activation(sum) : sum;
  }
  return out;
}

function forward(weights, input) {
  const h1 = denseLayer(input, weights.k1, weights.b1, relu);
  const h2 = denseLayer(h1, weights.k2, weights.b2, relu);
  const deepOut = denseLayer(h2, weights.kDeepOut, null, null)[0];
  const wideOut = denseLayer(input, weights.kWideOut, null, null)[0];
  return tanh(deepOut + wideOut);
}

module.exports = { loadWeights, forward };

if (require.main === module) {
  const { encodeBoard } = require("./encode.js");
  const weights = loadWeights(path.join(__dirname, "model", "weights.json"));
  const startBoard = JSON.parse(fs.readFileSync(path.join(__dirname, "..", "selfplay-data.jsonl"), "utf8").split("\n")[0]).board;
  const input = encodeBoard(startBoard, "white");
  console.log("pure-JS forward pass prediction:", forward(weights, input));
  console.log("(should match whatever number train.js just printed as the game-1 starting-position prediction)");
}
