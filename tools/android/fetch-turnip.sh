#!/usr/bin/env bash
# The Mesa Vulkan drivers — Turnip for Adreno, PanVK for Mali — and
# libadrenotools, into the Android APK's lib/arm64-v8a
# (crates/rustudio-android/libs/arm64-v8a/, which manifest.yaml lists as
# runtime_libs). The app swaps the driver in at start on Snapdragon
# devices (Turnip) and on Mali devices with ARM's kbase kernel driver
# (PanVK) (src/android/turnip.rs; docs/ANDROID.md "Turnip driver"). Run
# before `x build` — the Android workflow does; a build without these
# files is a normal APK that keeps the device's driver.
#
# Sources (all prebuilt, none is built here; the pinned downloads):
#   * libadrenotools-native: TouseefX's fork of bylaws/libadrenotools
#     (BSD-2-Clause; libshadowhook / libbytehook inside are ByteDance's,
#     MIT) — libadrenotools.so and its hook libraries. The pinned build is
#     the fork's CI artifact of commit 4d4d482 (2026-11-17 12:42, "driver:
#     fix initial": the driver.cpp rewrite — the Vulkan loader looked for
#     under the vendor paths too, which lets HyperOS devices try Turnip;
#     its own initialisation switchable off with ADRENOTOOLS_DISABLE=1,
#     which the app sets, and hooking only vkGetInstanceProcAddr /
#     vkGetDeviceProcAddr with the system's own callers bypassed when it
#     isn't; a working PanVK path for Mali; a mailbox present-mode default
#     for Mesa's own WSI), served by nightly.link:
#     https://nightly.link/TouseefX/libadrenotools-native/actions/runs/44085827920/libadrenotools-arm64-v8a.zip
#     GitHub keeps workflow artifacts for 90 days: when it is gone, point
#     ADRENOTOOLS_ZIP at a release of the fork (V0.7 is the last one:
#     https://github.com/TouseefX/libadrenotools-native/releases/download/V0.7/libadrenotools-arm64-v8a.zip,
#     SHA-256 73eb6a8178d9280d0ed25ef1bee1852a8225dcfb4a8c82e11d0a5146424871b3;
#     it keeps HyperOS on the device's driver, and devices whose device
#     tree names a Mali, and its hooks of the system loader are run,
#     unused, since it has no ADRENOTOOLS_DISABLE).
#   * mesa-turnip-android-driver: nihui's Mesa builds for Android
#     (Mesa: MIT) — libvulkan_freedreno.so:
#     https://github.com/nihui/mesa-turnip-android-driver/releases/download/26.3.0/mesa-turnip-android-26.3.0.zip
#   * panvk-kbase-android: zenithblue-oss's Mesa PanVK built for Android
#     on ARM's kbase kernel driver (Mesa: MIT) — one bare .so, 35 MB,
#     put in as libvulkan_panfrost.so (the name libadrenotools-native's
#     own PanVK path opens, src/driver.cpp init_panvk_driver):
#     https://github.com/zenithblue-oss/panvk-kbase-android/releases/download/panvk-kbase-v26.3.1/libvulkan_panfrost-android-aarch64.so
#     PANVK=0 leaves it out (an APK 35 MB smaller that keeps Mali
#     devices on their own driver).
#
# Overrides, for a newer build or one of your own:
#   ADRENOTOOLS_ZIP=<url or local .zip>   TURNIP_ZIP=<url or local .zip>
#   PANVK_SO=<url or local .so>
#   ADRENOTOOLS_SHA256= / TURNIP_SHA256= / PANVK_SHA256=
#                                          (checked when given; the
#                                          defaults are pinned below)
#   PANVK=0                                no PanVK driver
#   FORCE=1                                replace files already there
# Files already in libs/arm64-v8a/ are kept unless FORCE=1, so a build of
# your own can be dropped in by hand (git ignores the .so files; `git add
# -f` to commit them).
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
dest="$root/crates/rustudio-android/libs/arm64-v8a"

ADRENOTOOLS_ZIP="${ADRENOTOOLS_ZIP:-https://nightly.link/TouseefX/libadrenotools-native/actions/runs/44085827920/libadrenotools-arm64-v8a.zip}"
ADRENOTOOLS_SHA256="${ADRENOTOOLS_SHA256-2b3bf3174d7e4de785d1f02a308ed91bec8ca6da3347f8c5ba575fb0d0bc090d}"
TURNIP_ZIP="${TURNIP_ZIP:-https://github.com/nihui/mesa-turnip-android-driver/releases/download/26.3.0/mesa-turnip-android-26.3.0.zip}"
TURNIP_SHA256="${TURNIP_SHA256-6a9bef1c83aa6280e2389d33a992f2d80199d1d13be629c7ff12d5f9f17b5803}"
PANVK_SO="${PANVK_SO:-https://github.com/zenithblue-oss/panvk-kbase-android/releases/download/panvk-kbase-v26.3.1/libvulkan_panfrost-android-aarch64.so}"
PANVK_SHA256="${PANVK_SHA256-62c2b3f98ca987d5ab50b6f01e2d2c1c5b3ee995fe93666aa5da9415f65c3348}"
# An override without a checksum of its own isn't held to the default's.
if [ "${ADRENOTOOLS_ZIP}" != "https://nightly.link/TouseefX/libadrenotools-native/actions/runs/44085827920/libadrenotools-arm64-v8a.zip" ] && [ -z "${ADRENOTOOLS_SHA256+x}" ]; then ADRENOTOOLS_SHA256=""; fi
if [ "${TURNIP_ZIP}" != "https://github.com/nihui/mesa-turnip-android-driver/releases/download/26.3.0/mesa-turnip-android-26.3.0.zip" ] && [ -z "${TURNIP_SHA256+x}" ]; then TURNIP_SHA256=""; fi
if [ "${PANVK_SO}" != "https://github.com/zenithblue-oss/panvk-kbase-android/releases/download/panvk-kbase-v26.3.1/libvulkan_panfrost-android-aarch64.so" ] && [ -z "${PANVK_SHA256+x}" ]; then PANVK_SHA256=""; fi

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

# fetch_file <name> <url-or-path> <sha256-or-empty> <file name in dest>
fetch_file() {
  local name="$1" src="$2" sum="$3" file="$4"
  if [ -f "$dest/$file" ] && [ "${FORCE:-0}" != "1" ]; then
    echo "$name: $dest/$file present, keeping it (FORCE=1 replaces)"
    return
  fi
  local out="$work/$file"
  case "$src" in
    http://*|https://*)
      echo "$name: downloading $src"
      curl -fsSL --retry 3 -o "$out" "$src"
      ;;
    *)
      echo "$name: using $src"
      cp "$src" "$out"
      ;;
  esac
  if [ -n "$sum" ]; then
    echo "$sum  $out" | sha256sum -c - >/dev/null || { echo "$name: checksum mismatch for $src" >&2; exit 1; }
  fi
  cp "$out" "$dest/$file"
  chmod 644 "$dest/$file"
  echo "$name: $file into $dest"
}

fetch adrenotools "$ADRENOTOOLS_ZIP" "$ADRENOTOOLS_SHA256" libadrenotools.so
fetch turnip "$TURNIP_ZIP" "$TURNIP_SHA256" libvulkan_freedreno.so
if [ "${PANVK:-1}" != "0" ]; then
  fetch_file panvk "$PANVK_SO" "$PANVK_SHA256" libvulkan_panfrost.so
else
  echo "panvk: PANVK=0, not fetched (Mali devices keep their driver)"
fi

# What the app looks for (src/android/turnip.rs): the loader shim, the
# hook libraries it loads from the APK's native library folder, and the
# drivers under the names the shim opens (PanVK's optional).
for f in libadrenotools.so libhook_impl.so libmain_hook.so libvulkan_freedreno.so; do
  [ -f "$dest/$f" ] || { echo "missing $dest/$f" >&2; exit 1; }
done
ls -l "$dest"
