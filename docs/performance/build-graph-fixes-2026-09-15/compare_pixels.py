from pathlib import Path
import numpy as np,json,gzip
root=Path(__file__).parent
out=[]
def exists(p):return p.exists() or p.with_suffix('.rgba.gz').exists()
def read(p):return p.read_bytes() if p.exists() else gzip.decompress(p.with_suffix('.rgba.gz').read_bytes())
frames=[59,79,89,109,129,149]
def compare(a,b,label):
 aa=np.frombuffer(read(a),dtype=np.uint8).reshape(1600,2560,4)
 bb=np.frombuffer(read(b),dtype=np.uint8).reshape(1600,2560,4)
 d=np.abs(aa.astype(np.int16)-bb.astype(np.int16));mask=np.any(d!=0,axis=2);y,x=np.where(mask)
 out.append({'comparison':label,'frame':int(a.stem.split('-')[-1]),'changed_pixels':int(mask.sum()),'total_pixels':int(mask.size),'max_channel_delta':int(d.max()),'mean_channel_delta':float(d.mean()),'bbox':None if not len(x) else [int(x.min()),int(y.min()),int(x.max()+1),int(y.max()+1)]})
for stage in ['baseline','projection']:
 for frame in frames:
  a=root/f'{stage}-pixels/artifact-{frame}.rgba';b=root/f'{stage}-pixels/legacy-{frame}.rgba'
  if exists(a) and exists(b):compare(a,b,f'{stage}: artifact vs legacy')
for mode in ['artifact','legacy']:
 for frame in frames:
  a=root/f'baseline-pixels/{mode}-{frame}.rgba';b=root/f'projection-pixels/{mode}-{frame}.rgba'
  if exists(a) and exists(b):compare(a,b,f'{mode}: baseline vs fixed')
(root/'pixel-comparison.json').write_text(json.dumps(out,indent=2))
for r in out:print(r)
