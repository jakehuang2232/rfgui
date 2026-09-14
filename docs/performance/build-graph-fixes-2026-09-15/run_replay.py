from pathlib import Path
import os,subprocess,sys
root=Path(__file__).parent
stage=sys.argv[1]
binary=os.environ.get('RFGUI_REPLAY_BINARY','/private/tmp/rfgui-build-graph-diagnosis/target/debug/deps/01_window-72fd5e24881af1e1')
for n,mode in enumerate(['artifact','legacy','legacy','artifact','artifact','legacy']):
 env=os.environ|{'DIAG_MODE':mode,'DIAG_DPR':'2','RFGUI_PROFILE_PAINT':'0','DIAG_SKIP_SAME_HOVER':'0'}
 with (root/f'{stage}-{n}-{mode}.log').open('w') as log:
  subprocess.run([binary,'native_build_graph_diagnosis','--ignored','--nocapture','--test-threads=1'],env=env,stdout=log,stderr=subprocess.STDOUT,check=True)
 print(stage,n,mode,'passed',flush=True)

if len(sys.argv)>2 and sys.argv[2]=='pixels':
 for mode in ['artifact','legacy']:
  env=os.environ|{'DIAG_MODE':mode,'DIAG_DPR':'2','RFGUI_PROFILE_PAINT':'0','DIAG_SKIP_SAME_HOVER':'0','DIAG_PIXEL_DIR':str(root.resolve()/f'{stage}-pixels')}
  with (root/f'{stage}-pixels-{mode}.log').open('w') as log:
   subprocess.run([binary,'native_build_graph_diagnosis','--ignored','--nocapture','--test-threads=1'],env=env,stdout=log,stderr=subprocess.STDOUT,check=True)
  print(stage,mode,'pixels passed',flush=True)
