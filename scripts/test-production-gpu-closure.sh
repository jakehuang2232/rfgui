#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

cargo test -q --lib native_production_ -- \
  --ignored \
  --nocapture \
  --test-threads=1 \
  --skip native_production_transform_surface_reuses_real_pool_on_second_frame \
  --skip native_production_retained_surface_tree_reuses_real_pool_on_second_frame \
  --skip native_production_isolation_reuses_real_pool_on_opacity_only_frame
