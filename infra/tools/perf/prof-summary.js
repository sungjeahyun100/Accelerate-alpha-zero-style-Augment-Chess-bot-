// node --cpu-prof --cpu-prof-dir=tools/perf/prof tools/perf/prof-run.js ; node tools/perf/prof-summary.js <file.cpuprofile>
const p=JSON.parse(require("fs").readFileSync(process.argv[2]));
const byId=new Map(p.nodes.map(n=>[n.id,n]));
const self=new Map();const dt=p.timeDeltas;
p.samples.forEach((id,i)=>self.set(id,(self.get(id)||0)+dt[i]));
const parent=new Map();p.nodes.forEach(n=>(n.children||[]).forEach(c=>parent.set(c,n.id)));
const tot={},slf={};let all=0;
for(const [id,t] of self){all+=t;const n=byId.get(id);const nm=n.callFrame.functionName||"(anon)";slf[nm]=(slf[nm]||0)+t;
 const seen=new Set();let c=id;while(c!==undefined){const nn=byId.get(c).callFrame.functionName||"(anon)";if(!seen.has(nn)){seen.add(nn);tot[nn]=(tot[nn]||0)+t;}c=parent.get(c);}}
const show=(o,k)=>Object.entries(o).sort((a,b)=>b[1]-a[1]).slice(0,k).map(([n,t])=>(t/all*100).toFixed(1)+"%  "+n).join("\n");
console.log("== inclusive\n"+show(tot,30)+"\n== self\n"+show(slf,20));
