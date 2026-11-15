#!/usr/bin/env bash
# Builds AfroBIM.app and a drag-to-Applications .dmg. Runs on macOS only (lipo, codesign, hdiutil).
#
# Usage: build-dmg.sh --version X.Y.Z --out DIR [--bundle-id ID] [--min-macos 11.0] [--no-fonts]
#                     --bin PATH [--bin PATH]
# One --bin makes a single-architecture image; two (arm64 and x86_64) are merged with lipo into
# a universal binary. With --launcher the real binary is installed as Contents/MacOS/afrobim-bin and
# Contents/MacOS/afrobim becomes a small shell script that starts it with Contents/Resources as the
# working directory. Use it ONLY until the app's resource lookup knows about Contents/Resources
# (see docs/installing.md); the app finds packs/ and fonts/ from its working directory today.
# The result is AfroBIM-VERSION-macos-<arm64|x86_64|universal>.dmg in DIR.
#
# The bundle is signed AD HOC (codesign -s -). That is not a Developer ID signature and does not
# notarise anything: Gatekeeper will still warn on first launch. Apple silicon is generally
# documented to refuse arm64 code that carries no signature at all, which is why the ad hoc
# signature is applied (this was not confirmed against an Apple page in the session that wrote this).
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
version="" out="" bundle_id="com.afrobim.AfroBIM" min_macos="11.0" bins=() extra=() launcher=0
while [ $# -gt 0 ]; do
  case "$1" in
    --version) version="$2"; shift 2 ;;
    --out) out="$2"; shift 2 ;;
    --bundle-id) bundle_id="$2"; shift 2 ;;
    --min-macos) min_macos="$2"; shift 2 ;;
    --bin) bins+=("$2"); shift 2 ;;
    --launcher) launcher=1; shift ;;
    --no-fonts) extra+=(--no-fonts); shift ;;
    *) echo "build-dmg.sh: unknown argument $1" >&2; exit 2 ;;
  esac
done
[ -n "$version" ] && [ -n "$out" ] && [ "${#bins[@]}" -ge 1 ] && [ "${#bins[@]}" -le 2 ] || {
  echo "usage: build-dmg.sh --version X.Y.Z --out DIR --bin PATH [--bin PATH] [--bundle-id ID] [--min-macos V] [--launcher] [--no-fonts]" >&2; exit 2; }
[ "$(uname -s)" = Darwin ] || { echo "build-dmg.sh: must run on macOS" >&2; exit 1; }

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
mkdir -p "$out"

# 1. The executable: single architecture, or a universal binary.
exe="$work/afrobim"
if [ "${#bins[@]}" = 2 ]; then
  lipo -create "${bins[0]}" "${bins[1]}" -output "$exe"
  arch=universal
else
  cp "${bins[0]}" "$exe"
  arch="$(lipo -archs "$exe")"
  [ "$arch" = arm64 ] || [ "$arch" = x86_64 ] || { echo "unexpected architecture: $arch" >&2; exit 1; }
fi
lipo -info "$exe"

# 2. The bundle.
app="$work/dmg/AfroBIM.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
"$here/../stage.sh" --bin "$exe" --dest "$app/Contents/MacOS" --resources "$app/Contents/Resources" ${extra[@]+"${extra[@]}"}
if [ "$launcher" = 1 ]; then
  mv "$app/Contents/MacOS/afrobim" "$app/Contents/MacOS/afrobim-bin"
  cat > "$app/Contents/MacOS/afrobim" <<'LAUNCH'
#!/bin/sh
# Interim launcher: the app looks for packs/ and fonts/ in its working directory.
here="$(cd "$(dirname "$0")" && pwd)"
cd "$here/../Resources" || exit 1
exec "$here/afrobim-bin" "$@"
LAUNCH
  chmod 755 "$app/Contents/MacOS/afrobim"
fi
cp "$here/../icons/afrobim.icns" "$app/Contents/Resources/AfroBIM.icns"
sed -e "s/@VERSION@/$version/g" -e "s/@BUNDLE_ID@/$bundle_id/g" -e "s/@MIN_MACOS@/$min_macos/g" \
  "$here/Info.plist.in" > "$app/Contents/Info.plist"
plutil -lint "$app/Contents/Info.plist"

# 3. Ad hoc signature (identity "-"). --deep is deprecated for distribution signing but is fine for
# a bundle with one executable and plain data files.
codesign --force --deep --sign - "$app"
codesign --verify --deep --strict --verbose=2 "$app"

# 4. The disk image: the app and an Applications link, for drag-to-install.
ln -s /Applications "$work/dmg/Applications"
dmg="$out/AfroBIM-$version-macos-$arch.dmg"
rm -f "$dmg"
hdiutil create -volname "AfroBIM $version" -srcfolder "$work/dmg" -fs HFS+ -format UDZO -ov "$dmg"

# 5. Check what was built: mount, look inside, unmount.
mnt="$work/mnt"
mkdir -p "$mnt"
hdiutil attach "$dmg" -mountpoint "$mnt" -nobrowse -readonly -quiet
ok=1
for p in AfroBIM.app/Contents/MacOS/afrobim AfroBIM.app/Contents/Info.plist AfroBIM.app/Contents/Resources/AfroBIM.icns \
         AfroBIM.app/Contents/Resources/packs/UK-TSM-2019.json AfroBIM.app/Contents/Resources/packs/UG-TSM-2004/pack.xml; do
  [ -e "$mnt/$p" ] || { echo "missing in image: $p" >&2; ok=0; }
done
[ -L "$mnt/Applications" ] || { echo "missing Applications link" >&2; ok=0; }
codesign --verify --deep --strict "$mnt/AfroBIM.app" || ok=0
hdiutil detach "$mnt" -quiet
[ "$ok" = 1 ] || exit 1
echo "wrote $dmg"
