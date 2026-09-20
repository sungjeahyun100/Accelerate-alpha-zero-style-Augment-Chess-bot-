const path=require("path"),fs=require("fs");
const cur=require(path.join(__dirname,"..","..","engine-merged.js"));
const lines=fs.readFileSync(path.join(__dirname,"..","..","data","experiments","selfplay-data.merged-engine-16cards-local-2026-09-15.jsonl"),"utf8").split("\n").filter(Boolean);
const step=Math.floor(lines.length/600);
for(let i=0;i<lines.length;i+=step){const rec=JSON.parse(lines[i]);
 const s=cur.cloneState({});s.board=rec.board.map(r=>r.map(p=>p?{type:p.t,color:p.c,moved:true}:null));s.mode="play";
 s.deckSlots={white:rec.deckSlots?.white||[],black:rec.deckSlots?.black||[]};s.captures={white:[],black:[]};s.aiSearchNoCards=true;cur.setWorkerBoardDimensions(s);
 cur.evaluateStateComponents(s,"white");cur.evaluateStateComponents(s,"black");}
