#!/usr/bin/env bash
# Work around https://github.com/tauri-apps/tauri/issues/15976 until the
# bundler supports excluding host display libraries. Run before upload/signing.
set -euo pipefail

if (( $# < 2 )); then
  echo "Usage: $0 APPIMAGETOOL APPIMAGE [APPIMAGE ...]" >&2
  exit 1
fi
appimagetool=$(realpath "$1")
shift
test -x "$appimagetool"

display_libraries() {
  local appdir=$1
  shift
  # Include SONAME symlinks and fully versioned files, also in multiarch dirs.
  find "$appdir/usr/lib" \( -type f -o -type l \) \( \
    -name 'libwayland-client.so*' -o -name 'libwayland-cursor.so*' -o \
    -name 'libwayland-egl.so*' -o -name 'libwayland-server.so*' -o \
    -name 'libxkbcommon.so*' -o -name 'libxcb-randr.so*' -o \
    -name 'libxcb-render.so*' -o -name 'libxcb-shm.so*' -o \
    -name 'libXau.so*' -o -name 'libXdmcp.so*' \
  \) "$@"
}

for image in "$@"; do
  (
    test -f "$image" || { echo "AppImage not found: $image" >&2; exit 1; }
    image=$(realpath "$image")
    chmod +x "$image"
    work_dir=$(mktemp -d)
    trap 'rm -rf "$work_dir"' EXIT
    cd "$work_dir"

    "$image" --appimage-extract > /dev/null
    test -d squashfs-root/usr/lib
    test -x squashfs-root/AppRun
    echo "Removing bundled display libraries from $(basename "$image")"
    display_libraries squashfs-root -print -delete

    # Reuse Tauri's runtime, avoiding an unpinned runtime download by appimagetool.
    offset=$("$image" --appimage-offset)
    [[ "$offset" =~ ^[1-9][0-9]*$ ]]
    head -c "$offset" "$image" > runtime
    # Extraction mode works on CI hosts without FUSE.
    ARCH=x86_64 "$appimagetool" --appimage-extract-and-run \
      --runtime-file "$work_dir/runtime" --comp zstd \
      squashfs-root "$work_dir/repacked.AppImage"
    test -s repacked.AppImage
    chmod +x repacked.AppImage

    # Verify the packaged result before replacing the original release artifact.
    mkdir verify
    cd verify
    ../repacked.AppImage --appimage-extract > /dev/null
    test -d squashfs-root/usr/lib
    test -x squashfs-root/AppRun
    remaining=$(display_libraries squashfs-root -print -quit)
    if [[ -n "$remaining" ]]; then
      echo "Conflicting library remains in repacked AppImage: $remaining" >&2
      exit 1
    fi
    mv "$work_dir/repacked.AppImage" "$image"
  )
done
