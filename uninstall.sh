#!/usr/bin/env bash
#
# LAMPPost uninstaller - run as your normal user:   ./uninstall.sh
#
# Stops all modules and undoes the system changes (port setting, group
# "apache", phpMyAdmin hook) and the app grid entry. Your data in
# /opt/lamppost (htdocs, databases, configs) is KEPT unless you pass --purge.
# Fedora's packages (httpd, mariadb, php, ...) stay installed.
#
set -euo pipefail
PREFIX=/opt/lamppost
SYSCTL_CONF=/etc/sysctl.d/50-lamppost.conf
PMA_CONF=/etc/phpMyAdmin/config.inc.php

say() { printf '\033[1;33m==>\033[0m %s\n' "$*"; }

if [ "${1:-}" = "--root-part" ]; then
    [ "$(id -u)" -eq 0 ] || exit 1
    owner=$2 purge=$3
    rm -f "$SYSCTL_CONF"
    sysctl -q -w net.ipv4.ip_unprivileged_port_start=1024
    gpasswd -d "$owner" apache >/dev/null 2>&1 || true
    if [ -f "$PMA_CONF" ]; then
        sed -i '/^\/\* LAMPPost: use LAMPPost/,/^}$/d' "$PMA_CONF"
    fi
    # leftovers of LIXAMPP, the old name of this project
    rm -f /etc/sysctl.d/50-lixampp.conf
    [ -f "$PMA_CONF" ] && sed -i -E '/^\/\* LIXAMPP:/,/^\}$/d' "$PMA_CONF" || true
    [ "$purge" = 1 ] && rm -rf /opt/lixampp || true
    # leftovers of earlier root-based versions
    rm -rf /usr/local/libexec/lamppost
    rm -f /usr/share/polkit-1/actions/org.lamppost.helper.policy
    [ "$purge" = 1 ] && rm -rf "$PREFIX" || true
    exit 0
fi

[ "$(id -u)" -ne 0 ] || { echo "Run as your normal user" >&2; exit 1; }
PURGE=0
[ "${1:-}" = "--purge" ] && PURGE=1 || true

if [ -x "$PREFIX/lamppost" ]; then
    say "Stopping all LAMPPost modules"
    "$PREFIX/lamppost" stop all || true
fi
# the control panel window and its tray icon
pkill -f "^$PREFIX/lamppost-control" 2>/dev/null || true

say "Removing app grid entry"
rm -f ~/.local/share/applications/lixampp-control.desktop \
      ~/.local/share/icons/hicolor/scalable/apps/lixampp.svg \
      ~/.config/autostart/lixampp-control.desktop
rm -f ~/.local/share/applications/lamppost-control.desktop \
      ~/.local/share/icons/hicolor/scalable/apps/lamppost.svg \
      ~/.config/autostart/lamppost-control.desktop
update-desktop-database -q ~/.local/share/applications 2>/dev/null || true

say "Undoing system changes - password required"
pkexec "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/uninstall.sh" --root-part "$(id -un)" "$PURGE"

if [ "$PURGE" -eq 1 ]; then
    say "Removed $PREFIX including all websites and databases"
else
    say "Your data is still in $PREFIX (delete it with: ./uninstall.sh --purge)"
fi
