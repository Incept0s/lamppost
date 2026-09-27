#!/usr/bin/env bash
#
# Build LAMPPost-x86_64.AppImage - the control panel as one portable file.
#
#     packaging/build-appimage.sh [version]
#
# The AppImage contains the control panel and the setup files. The servers
# themselves still come from your distribution, so the first run needs:
#
#     ./LAMPPost-x86_64.AppImage --setup
set -euo pipefail

VERSION=${1:-1.0}
ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
OUT=$ROOT/dist
APPDIR=$ROOT/build/LAMPPost.AppDir
ARCH=$(uname -m)

PANEL=$ROOT/panel/target/release/lamppost-control
if [ ! -x "$PANEL" ]; then
    command -v cargo >/dev/null || { echo "no control panel binary and no cargo to build one" >&2; exit 1; }
    echo "==> building the control panel"
    (cd "$ROOT/panel" && cargo build --release)
fi

echo "==> assembling the AppDir"
rm -rf "$APPDIR"
install -d "$APPDIR/usr/bin" "$APPDIR/usr/share/lamppost" \
           "$APPDIR/usr/share/icons/hicolor/scalable/apps"
install -m755 "$PANEL" "$APPDIR/usr/bin/lamppost-control"
# "--setup" runs install.sh from usr/share/lamppost, which installs the panel
# from panel/ - the same place the RPM and DEB put it (squashfs stores it once)
install -Dm755 "$PANEL" "$APPDIR/usr/share/lamppost/panel/lamppost-control"
cp -a "$ROOT"/{src,templates,htdocs,assets,install.sh,uninstall.sh,README.md,LICENSE,THIRD-PARTY.md} \
      "$APPDIR/usr/share/lamppost/"
install -m644 "$ROOT/assets/lamppost.svg" "$APPDIR/lamppost.svg"
# licence notices of the crates compiled into the panel
install -d "$APPDIR/usr/share/doc/lamppost"
python3 "$ROOT/packaging/third-party-licenses.py" "$APPDIR/usr/share/doc/lamppost/THIRD-PARTY-LICENSES.txt"
install -m644 "$ROOT/assets/lamppost.svg" "$APPDIR/usr/share/icons/hicolor/scalable/apps/lamppost.svg"

cat > "$APPDIR/lamppost-control.desktop" <<'EOF'
[Desktop Entry]
Type=Application
Name=LAMPPost Control Panel
Comment=Start and stop Apache, MariaDB, FTP, Mail and Tomcat
Exec=lamppost-control
Icon=lamppost
Categories=Development;
Keywords=apache;mariadb;mysql;php;phpmyadmin;tomcat;ftp;webserver;
Terminal=false
StartupWMClass=lamppost-control
EOF

cat > "$APPDIR/AppRun" <<'EOF'
#!/bin/sh
# LAMPPost AppImage entry point.
HERE=$(dirname "$(readlink -f "$0")")
case "${1:-}" in
    --setup)
        # create /opt/lamppost and install the missing packages of this distro
        shift
        exec "$HERE/usr/share/lamppost/install.sh" "$@"
        ;;
    --uninstall)
        shift
        exec "$HERE/usr/share/lamppost/uninstall.sh" "$@"
        ;;
esac
if [ ! -x /opt/lamppost/lamppost ]; then
    echo "LAMPPost is not set up on this system yet. Run:"
    echo
    echo "    $APPIMAGE --setup"
    echo
    echo "That installs your distribution's Apache, MariaDB and PHP packages"
    echo "and creates /opt/lamppost. It asks for your password once."
fi
exec "$HERE/usr/bin/lamppost-control" "$@"
EOF
chmod 755 "$APPDIR/AppRun"

TOOL=${APPIMAGETOOL:-$ROOT/build/appimagetool}
if [ ! -x "$TOOL" ]; then
    echo "==> downloading appimagetool"
    mkdir -p "$(dirname "$TOOL")"
    curl -fsSL -o "$TOOL" \
        "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-${ARCH}.AppImage"
    chmod 755 "$TOOL"
fi

echo "==> building the AppImage"
mkdir -p "$OUT"
# no FUSE in containers and on some systems, so unpack the tool instead
export APPIMAGE_EXTRACT_AND_RUN=1
export ARCH
"$TOOL" "$APPDIR" "$OUT/LAMPPost-${VERSION}-${ARCH}.AppImage" >/dev/null
echo "==> $OUT/LAMPPost-${VERSION}-${ARCH}.AppImage"
ls -lh "$OUT/LAMPPost-${VERSION}-${ARCH}.AppImage" | awk '{print $5, $9}'
