#!/usr/bin/env bash
# scripts/conpty.sh [folder...]: Microsoft's console host for OneTerm (specs/oneterm, "The console
# host"). Fetches the pinned NuGet package into .cache/conpty at the root of the workspace
# once, checks its SHA-256, and copies conpty.dll and OpenConsole.exe into each folder given,
# or beside the binaries cargo builds: target/debug, target/release, target/dist and
# target/dist/examples. CARGO names another cargo than scripts/cargo.sh.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$("${CARGO:-$here/cargo.sh}" locate-project --workspace --message-format plain --manifest-path "$here/../crates/app/Cargo.toml")"
root="$(dirname "$(cygpath -u "$root" 2>/dev/null || echo "$root")")"
version=1.25.260930003
sha256=02b07b349af66d801159bdf9e440d4a1ce78bb951f37fc8609731665afdae7ee
cache="$root/.cache/conpty"
package="$cache/conpty.$version.nupkg"
unpacked="$cache/$version"
if [ ! -f "$unpacked/runtimes/win-x64/native/conpty.dll" ]; then
	mkdir -p "$cache"
	if [ ! -f "$package" ]; then
		url="https://api.nuget.org/v3-flatcontainer/microsoft.windows.console.conpty/$version/microsoft.windows.console.conpty.$version.nupkg"
		curl -sSfL --retry 3 -o "$package.part" "$url"
		mv "$package.part" "$package"
	fi
	echo "$sha256 *$package" | sha256sum -c --quiet - || { echo "conpty: $package is not the pinned package" >&2; rm -f "$package"; exit 1; }
	rm -rf "$unpacked"
	unzip -q -o "$package" -d "$unpacked"
fi
folders=("$@")
target="${CARGO_TARGET_DIR:-$root/target}"
[ ${#folders[@]} -gt 0 ] || folders=("$target/debug" "$target/release" "$target/dist" "$target/dist/examples")
for folder in "${folders[@]}"; do
	mkdir -p "$folder"
	cp "$unpacked/runtimes/win-x64/native/conpty.dll" "$unpacked/build/native/runtimes/x64/OpenConsole.exe" "$folder/"
done
echo "conpty: $version in ${folders[*]}"
