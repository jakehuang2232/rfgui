from pathlib import Path
import subprocess,sys,json,hashlib
source=Path('/Users/jakehuang/.codex/worktrees/e0c3/rfgui')
dest=Path('/private/tmp/rfgui-build-graph-diagnosis')
ref=sys.argv[1]
paths=subprocess.check_output(['git','diff','--name-only','0c422b7','b138789'],cwd=source,text=True).splitlines()
hashes={}
for name in paths:
 if '/tests' in name or name.endswith('_tests.rs'):continue
 data=subprocess.check_output(['git','show',ref+':'+name],cwd=source)
 (dest/name).write_bytes(data)
 hashes[name]=hashlib.sha256(data).hexdigest()
(Path(__file__).parent/f'{ref}-source.json').write_text(json.dumps(hashes,indent=2))
print('prepared',ref,len(hashes),'production files')
