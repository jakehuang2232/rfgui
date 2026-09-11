#!/usr/bin/env bash
set -euo pipefail
repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
acceptance_dir="${RFGUI_ACCEPTANCE_OUTPUT:-$repo_root/target/browser-renderer-acceptance}"
mkdir -p "$acceptance_dir"
# Use the CLI matching Cargo.lock; a version mismatch must fail, not silently
# regenerate a different lockfile or fetch an unrelated compiler.
bindgen_version="$(python3 -c 'import tomllib; print(next(p["version"] for p in tomllib.load(open("Cargo.lock","rb"))["package"] if p["name"]=="wasm-bindgen"))')"
if [[ -n "${WASM_BINDGEN:-}" ]]; then
  bindgen_cli="$WASM_BINDGEN"
elif command -v wasm-bindgen >/dev/null; then
  bindgen_cli="$(command -v wasm-bindgen)"
else
  bindgen_cli="$HOME/Library/Caches/dev.trunkrs.trunk/wasm-bindgen-$bindgen_version/wasm-bindgen"
fi
[[ -x "$bindgen_cli" ]] || { echo "Set WASM_BINDGEN to wasm-bindgen $bindgen_version" >&2; exit 1; }
[[ "$("$bindgen_cli" --version)" == "wasm-bindgen $bindgen_version" ]]
# Build test-only exports as a library: the standard Rust test executable
# starts libtest, whose command-line/thread runtime cannot run in a browser.
cargo rustc --locked -p rfgui --target wasm32-unknown-unknown --lib --crate-type cdylib --message-format=json -- --cfg test \
  > "$acceptance_dir/build.jsonl" 2> "$acceptance_dir/build.log"
wasm_file="$(python3 - "$acceptance_dir/build.jsonl" <<'PY'
import sys,json
artifacts=[f for line in open(sys.argv[1]) if (j:=json.loads(line)).get('reason')=='compiler-artifact' and j['target']['name']=='rfgui' for f in j['filenames'] if f.endswith('.wasm')]
assert len(artifacts)==1,artifacts
print(artifacts[0])
PY
)"
"$bindgen_cli" "$wasm_file" --target web --out-dir "$acceptance_dir" --out-name rfgui
node scripts/test-browser-renderer-acceptance.mjs "$acceptance_dir" | tee "$acceptance_dir/run.log"
