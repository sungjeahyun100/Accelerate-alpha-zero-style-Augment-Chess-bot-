// same as ab-time.js but cards ENABLED in search (card-heavy equivalence of the speed changes); compares against tools/perf/engine-head.js (the pre-speed-up engine, git show 5e8c861:engine-merged.js)
const fs=require("fs"),path=require("path");const root=path.join(__dirname,"..","..");
const N=+process.argv[2]||8,D=+process.argv[3]||2;
const A=require("./engine-head.js"),B=require(path.join(root,"engine-merged.js"));
const lines=fs.readFileSync(path.join(root,"data","experiments","selfplay-data.merged-engine-16cards-local-2026-09-15.jsonl"),"utf8").split("\n").filter(Boolean);
const step=Math.floor(lines.length/N);
function mk(e,rec){const s=e.cloneState({});s.board=rec.board.map(r=>r.map(p=>p?{type:p.t,color:p.c,moved:true}:null));s.mode="play";s.turn=rec.turn;s.deckSlots={white:rec.deckSlots?.white||[],black:rec.deckSlots?.black||[]};s.captures={white:[],black:[]};s.aiSearchNoCards=false;s.turnsTaken={white:10,black:10};s.actionsRemaining=1;s.moveCount=20;s.castlingCanceled={white:true,black:true};e.setWorkerBoardDimensions(s);return s;}
let tA=0,tB=0,same=0,n=0;
for(let i=0;n<N;i+=step){const rec=JSON.parse(lines[i]);n++;const out=[];
 for(const e of [A,B]){const s=mk(e,rec);const a=e.generateActions(s,rec.turn);const t=Date.now();const r=e.searchBestAction(s,a,rec.turn,D,60000,{limits:{depth:D,movetimeMs:60000}});out.push([Date.now()-t,JSON.stringify([r.action,r.score,r.nodes,r.cutoffs])]);}
 tA+=out[0][0];tB+=out[1][0];if(out[0][1]===out[1][1])same++;else console.log("DIFF pos",i," head",out[0][1].slice(0,260)," new ",out[1][1].slice(0,260));}
console.log(`HEAD ${tA}ms new ${tB}ms speedup ${(tA/tB).toFixed(2)}x identical ${same}/${n}`);
