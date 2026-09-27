#!/usr/bin/env bash
#
# Build a .deb. Run it on Debian or Ubuntu (the control panel is compiled
# here, so it matches that system's C library):
#
#     packaging/build-deb.sh [version]
#
# "make deb" runs this for you, in a Debian container if you are not on one.
set -euo pipefail

VERSION=${1:-1.0}
ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
OUT=$ROOT/dist
STAGE=$(mktemp -d)
trap 'rm -rf "$STAGE"' EXIT

command -v dpkg-deb >/dev/null || { echo "dpkg-deb is missing - run this on Debian/Ubuntu" >&2; exit 1; }
ARCH=$(dpkg --print-architecture)

PANEL=${CARGO_TARGET_DIR:-$ROOT/panel/target}/release/lamppost-control
if [ ! -x "$PANEL" ]; then
    command -v cargo >/dev/null || { echo "no control panel binary and no cargo to build one" >&2; exit 1; }
    # Debian's own cargo can be older than the dependencies need; rustup works
    have=$(rustc --version | awk '{print $2}')
    need=1.88.0
    if [ "$(printf '%s\n%s\n' "$need" "$have" | sort -V | head -n1)" != "$need" ]; then
        echo "rustc $have is too old (need $need or newer)." >&2
        echo "Install a current toolchain: https://rustup.rs" >&2
        exit 1
    fi
    echo "==> building the control panel"
    (cd "$ROOT/panel" && cargo build --release)
fi

echo "==> assembling the package"
install -d "$STAGE/DEBIAN" "$STAGE/usr/share/lamppost" "$STAGE/usr/bin" \
           "$STAGE/etc/sysctl.d" "$STAGE/usr/share/icons/hicolor/scalable/apps" \
           "$STAGE/usr/share/doc/lamppost"
cp -a "$ROOT"/{src,templates,htdocs,assets,install.sh,uninstall.sh,README.md,LICENSE,THIRD-PARTY.md} \
      "$STAGE/usr/share/lamppost/"
install -Dm755 "$PANEL" "$STAGE/usr/share/lamppost/panel/lamppost-control"
install -Dm755 "$ROOT/packaging/lamppost-setup" "$STAGE/usr/bin/lamppost-setup"
install -Dm644 "$ROOT/packaging/50-lamppost.conf" "$STAGE/etc/sysctl.d/50-lamppost.conf"
install -Dm644 "$ROOT/assets/lamppost.svg" "$STAGE/usr/share/icons/hicolor/scalable/apps/lamppost.svg"
install -Dm644 "$ROOT/LICENSE" "$STAGE/usr/share/doc/lamppost/copyright"
# licence notices of the crates compiled into the panel
python3 "$ROOT/packaging/third-party-licenses.py" "$STAGE/usr/share/doc/lamppost/THIRD-PARTY-LICENSES.txt"

SIZE=$(du -ks "$STAGE" | cut -f1)
cat > "$STAGE/DEBIAN/control" <<EOF
Package: lamppost
Version: $VERSION
Section: web
Priority: optional
Architecture: $ARCH
Maintainer: Incept0s <noreply@users.noreply.github.com>
Installed-Size: $SIZE
Depends: apache2, php-fpm, php-cli, php-mysql, php-gd, php-mbstring, php-intl,
 php-xml, php-zip, mariadb-server, mariadb-client, python3-pyftpdlib,
 python3-aiosmtpd, openssl, iproute2, pkexec | policykit-1, libc6
Recommends: phpmyadmin, tomcat10, tomcat10-admin
Homepage: https://github.com/Incept0s/lamppost
Description: XAMPP-style local web stack built on your distribution's packages
 LAMPPost gives you an XAMPP-style workflow on Linux: one window with Start and
 Stop buttons for Apache, MariaDB, an FTP server, a mail catcher and Tomcat,
 plus an htdocs folder for your PHP files and the familiar XAMPP layout.
 .
 Unlike XAMPP it ships no servers of its own. It starts your distribution's
 packages with its own configuration in /opt/lamppost, so your local server is
 always the up-to-date, security-patched software from your distribution.
 Every module runs as your own user, so starting and stopping needs no
 password.
 .
 This package installs the program files. Run "lamppost-setup" once as your
 normal user to create your personal instance in /opt/lamppost.
EOF

cat > "$STAGE/DEBIAN/postinst" <<'EOF'
#!/bin/sh
set -e
# ports from 21 upwards for normal programs (see the file for details)
[ -x /sbin/sysctl ] || [ -x /usr/sbin/sysctl ] && sysctl -q -p /etc/sysctl.d/50-lamppost.conf 2>/dev/null || true
cat <<'EOM'

LAMPPost is installed. Run this once as your normal user (not with sudo):

    lamppost-setup

It creates your instance in /opt/lamppost and adds LAMPPost to the app grid.

EOM
exit 0
EOF

cat > "$STAGE/DEBIAN/postrm" <<'EOF'
#!/bin/sh
set -e
if [ "$1" = purge ]; then
    sysctl -q -w net.ipv4.ip_unprivileged_port_start=1024 2>/dev/null || true
fi
exit 0
EOF
chmod 755 "$STAGE/DEBIAN/postinst" "$STAGE/DEBIAN/postrm"

mkdir -p "$OUT"
PACKAGE="$OUT/lamppost_${VERSION}_${ARCH}.deb"
dpkg-deb --build --root-owner-group "$STAGE" "$PACKAGE" >/dev/null
echo "==> $PACKAGE"
dpkg-deb --info "$PACKAGE" | sed -n '2,8p'
