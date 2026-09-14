import json,re,statistics,math
from pathlib import Path
root=Path(__file__).parent
result={}
for stage in ['assets','baseline','slots','hover','projection']:
 base=root.parent/'build-graph-diagnosis-2026-09-15' if stage=='assets' else root
 for mode in ['artifact','legacy']:
  rows=[]
  for path in sorted(base.glob(f'{stage}-[0-9]-{mode}.log')):
   samples=[]
   for l in path.read_text().splitlines():
    if l.startswith('diag-frame '):
     d=dict(re.findall(r'(\w+)=([^ ]+)',l.split(' cpu_ms=')[0]));d['cpu']=json.loads(l.split(' cpu_ms=')[1]); samples.append(d)
   if samples: assert len(samples)==150,(path,len(samples))
   rows+=samples
  if not rows:continue
  def stats(v):
   v=sorted(v);return {'n':len(v),'median':statistics.median(v),'p95':v[math.ceil(len(v)*.95)-1]}
  phases={}
  for phase in ['idle','hover','drag']:
   ds=[d for d in rows if d['phase']==phase]
   phases[phase]={'build':stats([d['cpu'][5] for d in ds]),'total':stats([d['cpu'][0] for d in ds]),'new_keys':stats([int(d['new_keys']) for d in ds]),'event_mutations':stats([int(d['event_mutations']) for d in ds])}
  result[f'{stage}-{mode}']={'frames':len(rows),'artifact_frames':sum(d['artifact']=='true' for d in rows),'phases':phases}
  print(stage,mode,[(p,round(v['build']['median'],3),v['new_keys']['median'],v['event_mutations']['median']) for p,v in phases.items()])
(root/'statistics.json').write_text(json.dumps(result,indent=2))
