const fs=require("fs"),path=require("path");const root=path.join(__dirname,"..","..");
const e=require(path.join(root,"engine-merged.js"));
const lines=fs.readFileSync(path.join(root,"data","experiments","selfplay-data.merged-engine-16cards-local-2026-09-15.jsonl"),"utf8").split("\n").filter(Boolean);
const step=Math.floor(lines.length/5);
for(let i=0;i<5*step;i+=step){const rec=JSON.parse(lines[i]);const s=e.cloneState({});s.board=rec.board.map(r=>r.map(p=>p?{type:p.t,color:p.c,moved:true}:null));s.mode="play";s.turn=rec.turn;
 s.deckSlots={white:rec.deckSlots?.white||[],black:rec.deckSlots?.black||[]};s.captures={white:[],black:[]};s.aiSearchNoCards=true;s.turnsTaken={white:10,black:10};s.actionsRemaining=1;s.moveCount=20;s.castlingCanceled={white:true,black:true};e.setWorkerBoardDimensions(s);
 const a=e.generateActions(s,rec.turn);const r=e.searchBestAction(s,a,rec.turn,1,1e9);console.log(a.length,r.nodes);}
