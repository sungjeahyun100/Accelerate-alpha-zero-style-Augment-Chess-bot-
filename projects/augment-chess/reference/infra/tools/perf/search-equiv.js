// node tools/perf/search-equiv.js [N=100] [depth=2] [--noorig] : compare searchBestAction old vs new (fixed depth, big time limit)
const fs=require("fs"),path=require("path");
const root=path.join(__dirname,"..","..");
const N=+process.argv[2]||100,D=+process.argv[3]||2;
const orig=require("./engine-orig.js"),cur=require(path.join(root,"engine-merged.js"));
const lines=fs.readFileSync(path.join(root,"data","experiments","selfplay-data.merged-engine-16cards-local-2026-09-15.jsonl"),"utf8").split("\n").filter(Boolean);
const step=Math.floor(lines.length/N);
function mk(e,rec){const s=e.cloneState({});s.board=rec.board.map(r=>r.map(p=>p?{type:p.t,color:p.c,moved:true}:null));s.mode="play";s.turn=rec.turn;
 s.deckSlots={white:rec.deckSlots?.white||[],black:rec.deckSlots?.black||[]};s.captures={white:[],black:[]};s.aiSearchNoCards=true;s.turnsTaken={white:10,black:10};s.actionsRemaining=1;s.moveCount=20;s.castlingCanceled={white:true,black:true};e.setWorkerBoardDimensions(s);return s;}
const LIM=6000;let slow=0,nondet=0,diffs=0,n=0,tO=0,tC=0;
for(let i=0;i<lines.length&&n<N;i+=step){const rec=JSON.parse(lines[i]);n++;console.error("pos",i);const col=rec.turn;
 let dtO=0,dtC=0;const res=[];for(const [e,k] of [[orig,0],[cur,1]]){const s=mk(e,rec);const t=process.hrtime.bigint();let r;
  try{const a=e.generateActions(s,col);r=e.searchBestAction(s,a,col,D,LIM);r=JSON.stringify({a:r.action,s:r.score,d:r.completedDepth,n:r.nodes,c:r.cutoffs});}catch(x){r="ERR "+x.message}
  const dt=Number(process.hrtime.bigint()-t);if(k){tC+=dt;dtC=dt/1e6}else{tO+=dt;dtO=dt/1e6}res.push(r);}
 if(dtO>LIM*0.8||dtC>LIM*0.8){slow++;continue;}
 if(res[0]!==res[1]){const run=(e)=>{try{const s2=mk(e,rec);const a2=e.generateActions(s2,col);const r2=e.searchBestAction(s2,a2,col,D,LIM);return JSON.stringify({a:r2.action,s:r2.score,d:r2.completedDepth,n:r2.nodes,c:r2.cutoffs});}catch(x){return 'ERR'}};
 const os=new Set([res[0]]),ns=new Set([res[1]]);for(let q=0;q<6;q++){os.add(run(orig));ns.add(run(cur));}
 if([...ns].every(x=>os.has(x))&&os.size>1){nondet++;continue;}diffs++;if(diffs<4)console.log("DIFF",i,res[0].slice(0,300),"\n  ",res[1].slice(0,300));}}
console.log(`positions ${n} depth ${D} diffs ${diffs} (orig-nondeterministic skipped: ${nondet}, slow-skipped ${slow}) orig ${(tO/1e6/n).toFixed(1)}ms new ${(tC/1e6/n).toFixed(1)}ms speedup ${(tO/tC).toFixed(2)}x`);
