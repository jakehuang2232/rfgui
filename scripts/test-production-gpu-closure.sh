#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

acceptance_dir="${RFGUI_NATIVE_ACCEPTANCE_OUTPUT:-$repo_root/target/native-renderer-acceptance}"
mkdir -p "$acceptance_dir"
rustc -Vv > "$acceptance_dir/toolchain.log"
# Run the full CPU suite and every opt-in native gate. Name-prefix filters and
# historical skips omit modern single-Viewport, budget and recovery coverage.
cargo test --locked -p rfgui --lib -- --test-threads=1 2>&1 | tee "$acceptance_dir/cpu.log"
cargo test --locked -p rfgui --lib -- --ignored --nocapture --test-threads=1 2>&1 | tee "$acceptance_dir/native.log"
# A successful command with zero selected tests is not hardware evidence.
python3 - "$acceptance_dir" <<'PY'
import pathlib,re,sys
directory=pathlib.Path(sys.argv[1])
for name in ('cpu','native'):
    log=(directory/f'{name}.log').read_text()
    result=re.search(r'test result: ok\. (\d+) passed; 0 failed;',log)
    assert result and int(result[1])>0, f'{name}: no successful executed tests'
    if name=='native':
        assert 'portable renderer acceptance passed:' in log, 'missing portable hardware corpus'
PY
