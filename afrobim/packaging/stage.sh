#!/usr/bin/env bash
# Stages an AfroBIM install tree: the binary plus the run-time files it loads from disk.
#
# Every platform package is built from the same tree, so a layout fault shows up on Linux first:
#   Linux portable archive   <dest>/afrobim, <dest>/packs, <dest>/fonts
#   Windows .msi             the same tree, installed under Program Files\AfroBIM
#   macOS AfroBIM.app        binary in Contents/MacOS, packs and fonts in Contents/Resources
#
# Usage: stage.sh --bin FILE --dest DIR [--resources DIR] [--no-fonts]
#   --bin        the built executable (afrobim or afrobim.exe)
#   --dest       directory that receives the executable
#   --resources  directory that receives packs/ and fonts/ (default: --dest)
#   --no-fonts   leave the Transport fonts out (see docs/installing.md: licence decision)
#
# What the app loads from disk is listed in afrobim/docs/installing.md ("What the app needs").
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=layout.env
. "$here/layout.env"
repo="$(cd "$here/$REPO_ROOT_REL" && pwd)"
packs="$repo/$PACKS_REL"
fontsdir="$repo/$FONTS_REL"

bin="" dest="" res="" fonts=1
while [ $# -gt 0 ]; do
  case "$1" in
    --bin) bin="$2"; shift 2 ;;
    --dest) dest="$2"; shift 2 ;;
    --resources) res="$2"; shift 2 ;;
    --no-fonts) fonts=0; shift ;;
    *) echo "stage.sh: unknown argument $1" >&2; exit 2 ;;
  esac
done
[ -n "$bin" ] && [ -n "$dest" ] || { echo "usage: stage.sh --bin FILE --dest DIR [--resources DIR] [--no-fonts]" >&2; exit 2; }
[ -f "$bin" ] || { echo "stage.sh: binary not found: $bin" >&2; exit 1; }
res="${res:-$dest}"

mkdir -p "$dest" "$res/packs/UG-TSM-2004"
install -m 0755 "$bin" "$dest/$(basename "$bin")"

# Rule packs: the UK pack is one JSON file, the Uganda pack a folder of XML files. The XSD files in
# $PACKS_REL/Schema are only for authoring; the app does not read them.
install -m 0644 "$packs/UK-TSM-2019.json" "$res/packs/"
install -m 0644 "$packs"/UG-TSM-2004/*.xml "$res/packs/UG-TSM-2004/"

if [ "$fonts" = 1 ]; then
  mkdir -p "$res/fonts"
  install -m 0644 "$fontsdir/tranh___.TTF" "$fontsdir/tranm___.TTF" "$res/fonts/"
else
  echo "stage.sh: fonts left out; the app will report 'Traffic signs unavailable' until AFROBIM_FONTS points at them" >&2
fi

echo "staged $(basename "$bin") in $dest, resources in $res"
