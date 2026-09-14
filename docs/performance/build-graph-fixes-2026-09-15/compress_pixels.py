from pathlib import Path
import gzip,hashlib,json,struct,zlib
r=Path(__file__).parent;manifest={}
def png(data):
 def chunk(t,d):return struct.pack('>I',len(d))+t+d+struct.pack('>I',zlib.crc32(t+d))
 scan=b''.join(b'\x00'+data[y*10240:(y+1)*10240] for y in range(1600))
 return b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',2560,1600,8,6,0,0,0))+chunk(b'IDAT',zlib.compress(scan))+chunk(b'IEND',b'')
for p in sorted(r.glob('*-pixels/*.rgba')):
 data=p.read_bytes();manifest[str(p.relative_to(r))]=hashlib.sha256(data).hexdigest()
 if p.parent.name=='projection-pixels' and p.stem.endswith('-109'):(r/f'fixed-{p.stem}.png').write_bytes(png(data))
 target=p.with_suffix('.rgba.gz');target.write_bytes(gzip.compress(data,compresslevel=6,mtime=0))
 assert gzip.decompress(target.read_bytes())==data
 p.unlink()
(r/'pixel-sha256.json').write_text(json.dumps(manifest,indent=2))
print('losslessly compressed',len(manifest),'pixel frames')
