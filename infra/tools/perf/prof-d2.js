// node --cpu-prof --cpu-prof-dir=tools/perf/prof tools/perf/prof-d2.js [N=8] [depth=2]
const fs=require("fs"),path=require("path");const root=path.join(__dirname,"..","..");
const e=require(path.join(root,"engine-merged.js"));const N=+process.argv[2]||8,D=+process.argv[3]||2;
const lines=fs.readFileSync(path.join(root,"data","experiments","selfplay-data.merged-engine-16cards-local-2026-09-15.jsonl"),"utf8").split("\n").filter(Boolean);
const step=Math.floor(lines.length/N);
for(let i=0,n=0;n<N;i+=step,n++){const rec=JSON.parse(lines[i]);const s=e.cloneState({});s.board=rec.board.map(r=>r.map(p=>p?{type:p.t,color:p.c,moved:true}:null));s.mode="play";s.turn=rec.turn;s.deckSlots={white:rec.deckSlots?.white||[],black:rec.deckSlots?.black||[]};s.captures={white:[],black:[]};s.aiSearchNoCards=true;s.turnsTaken={white:10,black:10};s.actionsRemaining=1;s.moveCount=20;s.castlingCanceled={white:true,black:true};e.setWorkerBoardDimensions(s);
 const a=e.generateActions(s,rec.turn);e.searchBestAction(s,a,rec.turn,D,60000,{limits:{depth:D,movetimeMs:60000}});}
