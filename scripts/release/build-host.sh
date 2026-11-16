#!/usr/bin/env bash
# The static sheprd-remote-control Sheprd copies onto SSH and cloud machines
# of another platform to pair a phone with them (host-manifest.json, schema
# 4), and the static sheprd-acp its chats run (schema 5). musl links
# everything in, so they start whatever glibc the machine has.
set -euo pipefail
source "$(dirname -- "$0")/common.sh"
[[ $# == 3 ]] || fail 'Usage: bash build-host.sh VERSION TARGET OUTPUT_DIR'
version_check "$1"
case $2 in x86_64-unknown-linux-musl|aarch64-unknown-linux-musl) ;; *) fail 'Unsupported host target';; esac
[[ -d $3 ]] || fail 'Existing output directory required'
arch=${2%%-*}
[[ $(uname -m) == "$arch" ]] || fail "Build $2 on a $arch machine"
dir=$(cd -- "$3" && pwd)
out=$dir/sheprd-remote-control-$1-$2
# Chats there run this static sheprd-acp; the sign job packs both targets'
# with their signed manifest (update-manifest.py pack-host-chats).
chat=$dir/sheprd-acp-$1-$2
new_output "$out"
new_output "$chat"
if ! command -v musl-gcc >/dev/null; then
    sudo apt-get install -y --no-install-recommends musl-tools
fi
rustup target add "$2"
export "CC_${2//-/_}=musl-gcc"
cd "$release_root"
cargo build --locked --release -p sheprd-remote-control -p sheprd-acp --target "$2"
target/$2/release/sheprd-remote-control --version
target/$2/release/sheprd-acp --help >/dev/null
for program in sheprd-remote-control sheprd-acp; do
    binary=target/$2/release/$program
    # No interpreter to look for on the machine it lands on.
    if file -L "$binary" | grep -q 'dynamically linked'; then
        fail "The host build of $program must be statically linked"
    fi
done
cp target/$2/release/sheprd-remote-control "$out"
cp target/$2/release/sheprd-acp "$chat"
strip "$out" "$chat"
chmod 755 "$out" "$chat"
printf '%s\n' "$out" "$chat"
