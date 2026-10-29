#!/usr/bin/env bash
# The single-large-attachment shape of spike A (handoff 003 T9; ledger PK-20, issue IS-12): a small wiki
# (`inklkunn-corpus wiki --pages 50 --total-bytes 204800 --depth 2 --blocks 1 --seed 5`) plus ONE random file of
# SIZE_MIB MiB at `pages/attachment.bin`, committed and sealed in its own block, with a throwaway key pair
# (never a real key directory).
#
# usage: make-attachment-shape.sh SIZE_MIB OUT_DIR [KEYS_DIR]   (KEYS_DIR keeps the throwaway key pair for
#        spike-a-write.py; without it the pair is discarded. Needs prikk on PATH and target/debug/inklkunn-corpus built:
#        cargo build -p inklkunn-corpus)
set -euo pipefail
size="${1:?SIZE_MIB}"
out="${2:?OUT_DIR}"
root="$(git rev-parse --show-toplevel)"
keys="${3:-}"
if [ -z "$keys" ]; then
  keys="$(mktemp -d)"
  trap 'rm -rf "$keys"' EXIT
fi
"$root/target/debug/inklkunn-corpus" wiki --out "$out" --keys "$keys" --pages 50 --total-bytes 204800 --depth 2 --blocks 1 --seed 5
head -c "$((size * 1024 * 1024))" /dev/urandom > "$out/pages/attachment.bin"
(
  export HOME="$keys"
  cd "$out"
  prikk commit -m "Add an attachment"
  prikk seal --allow-no-audit
)
echo "built $out: $(du -sm "$out/.prikk" | cut -f1) MiB in .prikk"
