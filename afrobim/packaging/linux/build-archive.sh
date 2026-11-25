#!/usr/bin/env bash
# Builds the Linux portable archive from a release binary.
# Usage: build-archive.sh BINARY VERSION OUT_DIR [--no-fonts]
# Result: OUT_DIR/afrobim-VERSION-linux-x86_64.tar.gz containing afrobim-VERSION/{afrobim,packs,fonts,LICENSE,EULA.md,NOTICE,THIRD-PARTY-NOTICES.md,README.txt}
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
bin="$1" version="$2" out="$3"
shift 3
name="afrobim-$version"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

"$here/../stage.sh" --bin "$bin" --dest "$work/$name" "$@"
cat > "$work/$name/README.txt" <<TXT
AfroBIM $version, portable Linux build (unsigned, untested on any distribution but the one it was built on).

Run ./afrobim from this folder. The packs/ and fonts/ folders beside it are found automatically.
LICENSE, EULA.md, NOTICE and THIRD-PARTY-NOTICES.md in this folder give the licence terms and the
third-party notices. Using AfroBIM means accepting EULA.md.
Needs a desktop session with OpenGL (X11 or Wayland). See docs/installing.md in the repository.
TXT

mkdir -p "$out"
tar -C "$work" --owner=0 --group=0 --sort=name -czf "$out/$name-linux-x86_64.tar.gz" "$name"
echo "wrote $out/$name-linux-x86_64.tar.gz"
