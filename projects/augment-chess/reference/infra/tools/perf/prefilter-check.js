// Can a reply win (remove a royal piece / end the game) without moving onto a royal square? Counts violations of that prefilter.
const fs=require("fs"),path=require("path");const root=path.join(__dirname,"..","..");
const e=require(path.join(root,"engine-merged.js"));
const lines=fs.readFileSync(path.join(root,"data","experiments","selfplay-data.merged-engine-16cards-local-2026-09-15.jsonl"),"utf8").split("\n").filter(Boolean);
const N=+process.argv[2]||40;const step=Math.floor(lines.length/N);
function mk(rec){const s=e.cloneState({});s.board=rec.board.map(r=>r.map(p=>p?{type:p.t,color:p.c,moved:true}:null));s.mode="play";s.turn=rec.turn;s.deckSlots={white:rec.deckSlots?.white||[],black:rec.deckSlots?.black||[]};s.captures={white:[],black:[]};s.aiSearchNoCards=true;s.turnsTaken={white:10,black:10};s.actionsRemaining=1;s.moveCount=20;s.castlingCanceled={white:true,black:true};e.setWorkerBoardDimensions(s);return s;}
const kings=(s,c)=>{let n=0;for(const r of s.board)for(const p of r)if(p&&p.color===c&&p.type==="king")n++;return n;};
let replies=0,wins=0,viol=0,types={},shown=0;
for(let i=0,k=0;k<N;i+=step,k++){const rec=JSON.parse(lines[i]);const ai=rec.turn,en=ai==="white"?"black":"white";const s=mk(rec);
 for(const a of e.generateActions(s,ai).slice(0,12)){const s1=e.cloneState(s);if(!e.applyAction(s1,JSON.parse(JSON.stringify(a)),ai).ok||s1.mode==="gameover"||s1.turn!==en)continue;
  const kb=kings(s1,ai);
  for(const r of e.generateActions(s1,en)){const s2=e.cloneState(s1);if(!e.applyAction(s2,JSON.parse(JSON.stringify(r)),en).ok)continue;replies++;
   const win=s2.mode==="gameover"&&s2.winner===en||kings(s2,ai)<kb;
   if(win){wins++;const t=r.type;types[t]=(types[t]||0)+1;
    const mp=s1.board[r.from?.row]?.[r.from?.col];const dest=r.move&&s1.board[r.move.row]?.[r.move.col];const BASIC=["pawn","knight","bishop","rook","queen","king"];const plain=r.type==="move"&&mp&&BASIC.includes(mp.type)&&Object.keys(r.move).every(k=>k==="row"||k==="col");const maybeRoyal=dest&&dest.color===ai&&!["pawn","knight","bishop","rook","queen"].includes(dest.type);
    if(plain&&!maybeRoyal){viol++;console.log("VIOLATION",JSON.stringify(r).slice(0,200));}}}}}
console.log({replies,wins,viol,types});
