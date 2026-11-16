#!/usr/bin/env bash
# The Turnip (Mesa) Vulkan driver and libadrenotools, into the Android
# APK's lib/arm64-v8a (crates/rustudio-android/libs/arm64-v8a/, which
# manifest.yaml lists as runtime_libs). The app swaps the driver in at
# start on Snapdragon devices (src/android/turnip.rs; docs/ANDROID.md
# "Turnip driver"). Run before `x build` — the Android workflow does; a
# build without these files is a normal APK that keeps the device's driver.
#
# Sources (both prebuilt, neither is built here; the pinned downloads):
#   * libadrenotools-native: TouseefX's fork of bylaws/libadrenotools
#     (BSD-2-Clause; libshadowhook / libbytehook inside are ByteDance's,
#     MIT) — libadrenotools.so and its hook libraries:
#     https://github.com/TouseefX/libadrenotools-native/releases/download/V0.7/libadrenotools-arm64-v8a.zip
#   * mesa-turnip-android-driver: nihui's Mesa builds for Android
#     (Mesa: MIT) — libvulkan_freedreno.so:
#     https://github.com/nihui/mesa-turnip-android-driver/releases/download/26.3.0/mesa-turnip-android-26.3.0.zip
#
# Overrides, for a newer build or one of your own:
#   ADRENOTOOLS_ZIP=<url or local .zip>   TURNIP_ZIP=<url or local .zip>
#   ADRENOTOOLS_SHA256= / TURNIP_SHA256=   (checked when given; the
#                                          defaults are pinned below)
#   FORCE=1                                replace files already there
# Files already in libs/arm64-v8a/ are kept unless FORCE=1, so a build of
# your own can be dropped in by hand (git ignores the .so files; `git add
# -f` to commit them).
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
dest="$root/crates/rustudio-android/libs/arm64-v8a"

ADRENOTOOLS_ZIP="${ADRENOTOOLS_ZIP:-https://github.com/TouseefX/libadrenotools-native/releases/download/V0.7/libadrenotools-arm64-v8a.zip}"
ADRENOTOOLS_SHA256="${ADRENOTOOLS_SHA256-73eb6a8178d9280d0ed25ef1bee1852a8225dcfb4a8c82e11d0a5146424871b3}"
TURNIP_ZIP="${TURNIP_ZIP:-https://github.com/nihui/mesa-turnip-android-driver/releases/download/26.3.0/mesa-turnip-android-26.3.0.zip}"
TURNIP_SHA256="${TURNIP_SHA256-6a9bef1c83aa6280e2389d33a992f2d80199d1d13be629c7ff12d5f9f17b5803}"
# An override without a checksum of its own isn't held to the default's.
if [ "${ADRENOTOOLS_ZIP}" != "https://github.com/TouseefX/libadrenotools-native/releases/download/V0.7/libadrenotools-arm64-v8a.zip" ] && [ -z "${ADRENOTOOLS_SHA256+x}" ]; then ADRENOTOOLS_SHA256=""; fi
if [ "${TURNIP_ZIP}" != "https://github.com/nihui/mesa-turnip-android-driver/releases/download/26.3.0/mesa-turnip-android-26.3.0.zip" ] && [ -z "${TURNIP_SHA256+x}" ]; then TURNIP_SHA256=""; fi

mkdir -p "$dest"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# fetch <name> <url-or-path> <sha256-or-empty> <marker file in dest> <files...>
fetch() {
  local name="$1" src="$2" sum="$3" marker="$4"
  if [ -f "$dest/$marker" ] && [ "${FORCE:-0}" != "1" ]; then
    echo "$name: $dest/$marker present, keeping it (FORCE=1 replaces)"
    return
  fi
  local zip="$work/$name.zip"
  case "$src" in
    http://*|https://*)
      echo "$name: downloading $src"
      curl -fsSL --retry 3 -o "$zip" "$src"
      ;;
    *)
      echo "$name: using $src"
      cp "$src" "$zip"
      ;;
  esac
  if [ -n "$sum" ]; then
    echo "$sum  $zip" | sha256sum -c - >/dev/null || { echo "$name: checksum mismatch for $src" >&2; exit 1; }
  fi
  local out="$work/$name"
  mkdir -p "$out"
  unzip -q -o "$zip" -d "$out"
  local n=0
  while IFS= read -r -d '' f; do
    cp "$f" "$dest/"
    chmod 644 "$dest/$(basename "$f")"
    n=$((n + 1))
  done < <(find "$out" -type f -name '*.so' -print0)
  [ "$n" -gt 0 ] || { echo "$name: no .so files in $src" >&2; exit 1; }
  [ -f "$dest/$marker" ] || { echo "$name: $marker not in $src" >&2; exit 1; }
  echo "$name: $n file(s) into $dest"
}

fetch adrenotools "$ADRENOTOOLS_ZIP" "$ADRENOTOOLS_SHA256" libadrenotools.so
fetch turnip "$TURNIP_ZIP" "$TURNIP_SHA256" libvulkan_freedreno.so

# What the app looks for (src/android/turnip.rs): the loader shim, the
# hook libraries it loads from the APK's native library folder, and the
# driver itself under the name the shim opens.
for f in libadrenotools.so libhook_impl.so libmain_hook.so libvulkan_freedreno.so; do
  [ -f "$dest/$f" ] || { echo "missing $dest/$f" >&2; exit 1; }
done
ls -l "$dest"
