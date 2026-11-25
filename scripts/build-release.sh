#!/usr/bin/env bash
# scripts/build-release.sh: OneTerm's release asset (specs/oneterm, "Updates" and "The
# repository"), the one script a developer and the release workflow both run.
#
# It builds the binary in the dist profile with scripts/app.manifest embedded by the linker,
# and stages in dist/, at the root of the workspace:
#   oneterm-<version>-<target>/             oneterm.exe, the console host, LICENSE, NOTICE,
#                                           THIRD-PARTY-NOTICES.txt
#   oneterm-<version>-<target>.zip          that folder: the name the updater asks a release for
#   oneterm-<version>-<target>.zip.sha256   "<hex>  <name>.zip", as `sha256sum -c` reads it
# <version> is the app's (crates/app/Cargo.toml), <target> the machine's. It publishes nothing.
#
# CARGO names another cargo than scripts/cargo.sh.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
cd "$here/.."
cargo="${CARGO:-$here/cargo.sh}"
root="$("$cargo" locate-project --workspace --message-format plain)"
root="$(dirname "$(cygpath -u "$root" 2>/dev/null || echo "$root")")"
target="${CARGO_TARGET_DIR:-$root/target}"
triple="$(rustc -vV | sed -n 's/^host: //p')"
version="$(sed -n 's/^version = "\(.*\)"/\1/p' crates/app/Cargo.toml | head -1)"
name="oneterm-$version-$triple"

# Only the final crate is linked again with these arguments; its dependencies are built once.
manifest="$(cygpath -w "$here/app.manifest")"
"$cargo" rustc --profile dist --quiet -p oneterm-app --bin oneterm -- \
	-C link-arg=/MANIFEST:EMBED -C "link-arg=/MANIFESTINPUT:$manifest"

out="$root/dist"
rm -rf "$out/$name" "$out/$name.zip" "$out/$name.zip.sha256"
mkdir -p "$out/$name"
cp "$target/dist/oneterm.exe" "$out/$name/"
grep -q PerMonitorV2 "$out/$name/oneterm.exe" || { echo "release: the manifest is not in oneterm.exe" >&2; exit 1; }
python "$here/exe_icon.py" "$out/$name/oneterm.exe" > /dev/null
CARGO="$cargo" bash "$here/conpty.sh" "$out/$name" > /dev/null
cp LICENSE NOTICE "$out/$name/"
# What the binary holds of others: its crates, the kit's icons and faces, the console host.
"$cargo" metadata --format-version 1 --filter-platform "$triple" |
	PYTHONUTF8=1 python "$here/notices.py" oneterm-app "$out/$name/THIRD-PARTY-NOTICES.txt"

cd "$out"
python -c "import shutil, sys; shutil.make_archive(sys.argv[1], 'zip', '.', sys.argv[1])" "$name"
sum="$(sha256sum "$name.zip" | cut -d' ' -f1)"
printf '%s  %s\n' "$sum" "$name.zip" > "$name.zip.sha256"
echo "release: dist/$name.zip ($(du -k "$name.zip" | cut -f1) KB)"
echo "sha256: $sum"
