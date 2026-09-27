#!/usr/bin/env bash
#
# LAMPPost installer - run as your normal user:   ./install.sh
#
# Safe to run again (e.g. after changing the control panel): existing
# configuration files, databases and your files in htdocs are never
# overwritten. Use --force-config to re-create the configuration files
# (the old ones are kept as *.bak).
#
# Asks for your password once (system setup). After that, starting and
# stopping LAMPPost never needs a password: every module runs as your user.
#
set -euo pipefail

SRC="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
PREFIX=/opt/lamppost
SYSCTL_CONF=/etc/sysctl.d/50-lamppost.conf

# where this distribution keeps Apache, PHP, MariaDB, Tomcat and phpMyAdmin
# shellcheck source=src/distro.sh
. "$SRC/src/distro.sh"
PMA_CONF=${PMA_CONF:-/etc/phpMyAdmin/config.inc.php}
WEB_GROUP=${WEB_GROUP:-apache}
# Fedora/RHEL follow the system-wide crypto policy, Debian does not know it
if [ "$LAMPPOST_FAMILY" = rpm ]; then
    SSL_CIPHERS="PROFILE=SYSTEM"
else
    SSL_CIPHERS="HIGH:!aNULL:!MD5:!3DES"
fi
if [ "$LAMPPOST_FAMILY" = deb ]; then
    PACKAGES=(apache2 php-fpm php-cli php-mysql php-gd php-mbstring php-intl php-xml
              php-zip mariadb-server mariadb-client phpmyadmin python3-pyftpdlib
              python3-aiosmtpd tomcat10 tomcat10-admin openssl iproute2)
else
    PACKAGES=(httpd mod_ssl php-fpm php-cli php-mysqlnd php-gd php-mbstring php-intl
              php-xml php-pecl-zip php-sodium mariadb-server mariadb phpMyAdmin
              python3-pyftpdlib python3-aiosmtpd tomcat tomcat-webapps tomcat-admin-webapps
              openssl iproute)
fi

say()  { printf '\033[1;33m==>\033[0m %s\n' "$*"; }
die()  { printf '\033[1;31mError:\033[0m %s\n' "$*" >&2; exit 1; }

# ---------------------------------------------------------------------------
# root part (runs through pkexec, called by the user part below)
# ---------------------------------------------------------------------------
if [ "${1:-}" = "--root-part" ]; then
    owner=$2
    [ "$(id -u)" -eq 0 ] || die "--root-part must run as root"
    id "$owner" >/dev/null 2>&1 || die "unknown user $owner"
    [ "$owner" != root ] || die "LAMPPost must belong to a normal user"

    # Normal programs may use ports from 21 up (FTP 21, mail 25, web 80/443),
    # so every LAMPPost module can run as the user - no root, no password.
    cat > "$SYSCTL_CONF" <<'EOF'
# LAMPPost: allow normal programs to use ports from 21 upwards (FTP 21,
# SMTP 25, HTTP 80, HTTPS 443), so LAMPPost runs without root.
# Removed by LAMPPost's uninstall.sh (default is 1024).
net.ipv4.ip_unprivileged_port_start = 21
EOF
    # in a container or a locked-down kernel this can fail; that is only fatal
    # when ports from 21 upwards are not allowed already
    if ! sysctl -q -p "$SYSCTL_CONF" 2>/dev/null; then
        current=$(cat /proc/sys/net/ipv4/ip_unprivileged_port_start 2>/dev/null || echo 1024)
        [ "$current" -le 21 ] || die "could not allow ports from 21 upwards (sysctl failed)"
        echo "note: the port setting was already in place"
    fi

    # the distribution's phpMyAdmin config is readable for its web group only
    [ -n "$WEB_GROUP" ] && usermod -aG "$WEB_GROUP" "$owner" || true

    # LIXAMPP (the old name of this project) - take the installation over
    if [ -d /opt/lixampp ] && [ ! -e "$PREFIX" ]; then
        mv /opt/lixampp "$PREFIX"
    fi
    rm -f /etc/sysctl.d/50-lixampp.conf
    [ -f "$PMA_CONF" ] && sed -i -E '/^\/\* LIXAMPP:/,/^\}$/d' "$PMA_CONF" || true

    install -d -m 755 -o "$owner" -g "$owner" "$PREFIX"

    # Tomcat's configuration is root-only on some distributions, so copy it here
    if [ -n "${TOMCAT_CONF:-}" ] && [ -d "$TOMCAT_CONF" ]; then
        install -d -m 755 "$PREFIX/tomcat/conf" "$PREFIX/tomcat/webapps"
        for f in server.xml web.xml context.xml catalina.properties catalina.policy \
                 logging.properties jaspic-providers.xml; do
            [ -e "$PREFIX/tomcat/conf/$f" ] || cp "$TOMCAT_CONF/$f" "$PREFIX/tomcat/conf/$f" 2>/dev/null || true
        done
        for app in ROOT manager host-manager; do
            [ -e "$PREFIX/tomcat/webapps/$app" ] || \
                cp -r "${TOMCAT_WEBAPPS:-/var/lib/tomcat/webapps}/$app" "$PREFIX/tomcat/webapps/$app" 2>/dev/null || true
        done
    fi
    # Files created by root in earlier LAMPPost versions (logs, pid files)
    chown -R "$owner:$owner" "$PREFIX"

    # Earlier LAMPPost versions used a root helper + Polkit rule - not needed anymore
    rm -rf /usr/local/libexec/lamppost
    rm -f /usr/share/polkit-1/actions/org.lamppost.helper.policy

    # Let Fedora's phpMyAdmin load LAMPPost's settings - only when it runs
    # under LAMPPost's PHP-FPM (env LAMPPOST=1), never for the system Apache.
    if [ -n "$PMA_CONF" ] && [ -f "$PMA_CONF" ] && ! grep -q "LAMPPOST" "$PMA_CONF"; then
        cat >> "$PMA_CONF" <<'EOF'

/* LAMPPost: use LAMPPost's settings when running under LAMPPost (added by install.sh) */
if (getenv('LAMPPOST') === '1' && is_readable('/opt/lamppost/phpMyAdmin/config.inc.php')) {
    include '/opt/lamppost/phpMyAdmin/config.inc.php';
}
EOF
    fi
    exit 0
fi

# ---------------------------------------------------------------------------
# user part
# ---------------------------------------------------------------------------
[ "$(id -u)" -ne 0 ] || die "run install.sh as your normal user, not as root"
FORCE_CONFIG=0
SKIP_PACKAGES=0
for arg in "$@"; do
    case "$arg" in
        --force-config)  FORCE_CONFIG=1 ;;
        # used by lamppost-setup from the RPM: dnf already installed them
        --skip-packages) SKIP_PACKAGES=1 ;;
        *) die "unknown option: $arg (use --force-config, --skip-packages)" ;;
    esac
done
ME=$(id -un)
TZ_NAME=$(timedatectl show -p Timezone --value 2>/dev/null || echo UTC)

if [ -x "$PREFIX/lamppost" ] && "$PREFIX/lamppost" status 2>/dev/null | grep -q running; then
    die "LAMPPost modules are running - stop them first:  $PREFIX/lamppost stop"
fi

if [ "$SKIP_PACKAGES" -eq 0 ]; then
    say "Checking packages"
    missing=()
    for p in "${PACKAGES[@]}"; do
        if [ "$LAMPPOST_FAMILY" = deb ]; then
            dpkg-query -W -f='${Status}' "$p" 2>/dev/null | grep -q "ok installed" || missing+=("$p")
        else
            rpm -q "$p" >/dev/null 2>&1 || missing+=("$p")
        fi
    done
    if [ ${#missing[@]} -gt 0 ]; then
        say "Installing missing packages: ${missing[*]} - password required"
        if [ "$LAMPPOST_FAMILY" = deb ]; then
            pkexec sh -c "apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y $(printf '%s ' "${missing[@]}")"
        else
            pkexec dnf install -y "${missing[@]}"
        fi
    fi
    # the paths may only exist now that the packages are installed
    . "$SRC/src/distro.sh"
fi

say "System setup (ports, group, phpMyAdmin hook) - password required"
pkexec "$SRC/install.sh" --root-part "$ME"

# Folders of the old LIXAMPP layout ("FileZilla"/"Mercury" were names of
# programs LAMPPost does not ship; the database is MariaDB, not MySQL).
MIGRATED=0
for old_new in "FileZillaFTP:ftp" "MercuryMail:mail" "mysql:mariadb"; do
    old=$PREFIX/${old_new%%:*}; new=$PREFIX/${old_new#*:}
    if [ -d "$old" ] && [ ! -d "$new" ]; then mv "$old" "$new"; MIGRATED=1; fi
done
if [ -f "$PREFIX/ftp/lixampp-ftp.ini" ]; then
    # keep the FTP password from the old installation
    FTP_PASSWORD=$(sed -n 's/^password *= *//p' "$PREFIX/ftp/lixampp-ftp.ini" | head -n1)
    mv "$PREFIX/ftp/lixampp-ftp.ini" "$PREFIX/ftp/lamppost-ftp.ini"
fi
[ -f "$PREFIX/lixampp-control.ini" ] && mv "$PREFIX/lixampp-control.ini" "$PREFIX/lamppost-control.ini" || true
[ -f "$PREFIX/mail/mercury.pid" ] && rm -f "$PREFIX/mail/mercury.pid" || true
rm -f "$PREFIX/lixampp" "$PREFIX/lixampp-control" "$PREFIX/icons/lixampp.svg" \
      "$PREFIX/mail/mercury-catcher" "$PREFIX/ftp/lixampp-ftpd" \
      "$PREFIX/panel/lixampp_theme.py" "$PREFIX/panel/lixampp_updates.py"
rm -rf "$PREFIX/panel/__pycache__"
if [ "$MIGRATED" -eq 1 ]; then
    # the old configuration files still point at /opt/lixampp
    say "Taking over the old LIXAMPP installation (your databases and htdocs are kept)"
    say "Configuration files are rewritten for the new paths; the old ones stay as *.bak"
    FORCE_CONFIG=1
fi

render() {  # <template> <destination> - never overwrites unless --force-config
    local tpl=$1 dst=$2
    if [ -e "$dst" ]; then
        [ "$FORCE_CONFIG" -eq 1 ] || return 0
        cp -p "$dst" "$dst.bak"
    fi
    mkdir -p "$(dirname "$dst")"
    sed -e "s|@PREFIX@|$PREFIX|g" -e "s|@USER@|$ME|g" -e "s|@TZ@|$TZ_NAME|g" \
        -e "s|@MODDIR@|${APACHE_MODDIR:-/usr/lib64/httpd/modules}|g" \
        -e "s|@PMA_DIR@|${PMA_DIR:-/usr/share/phpMyAdmin}|g" \
        -e "s|@SSL_CIPHERS@|$SSL_CIPHERS|g" \
        -e "s|@FTP_PASSWORD@|${FTP_PASSWORD:-}|g" "$tpl" > "$dst"
}

say "Creating the LAMPPost folder structure in $PREFIX"
mkdir -p "$PREFIX"/{apache/conf/extra,apache/conf/ssl.crt,apache/conf/ssl.key,apache/logs} \
         "$PREFIX"/{htdocs,mariadb/data,mariadb/bin,php/logs,phpMyAdmin} \
         "$PREFIX"/{tmp/sessions,tmp/phpmyadmin,ftp/logs,mail,mailoutput,mailtodisk} \
         "$PREFIX"/{tomcat/conf,tomcat/webapps,tomcat/logs,tomcat/temp,tomcat/work,tomcat/lib,icons}
chmod 700 "$PREFIX/apache/conf/ssl.key" "$PREFIX/tmp/sessions" "$PREFIX/tmp/phpmyadmin"

say "Detecting the Apache modules of this system"
APACHE_MODULES="mpm_event unixd authz_core authz_host authz_user authn_core authn_file
                auth_basic access_compat log_config mime dir autoindex alias rewrite
                headers env setenvif expires filter deflate status vhost_alias proxy
                proxy_fcgi socache_shmcb ssl http2"
builtin=$("${APACHE_BIN:-/usr/bin/httpd}" -l 2>/dev/null | tr -d ' ' | sed -n 's/^mod_\(.*\)\.c$/\1/p')
{
    echo "# Apache modules for $APACHE_BIN - generated by install.sh."
    echo "# Built-in modules are skipped; they cannot be loaded again."
    echo
    for m in $APACHE_MODULES; do
        if printf '%s\n' "$builtin" | grep -qx "$m"; then
            echo "# $m is built into this Apache"
        elif [ -f "${APACHE_MODDIR:-/usr/lib64/httpd/modules}/mod_$m.so" ]; then
            printf 'LoadModule %s_module %s/mod_%s.so\n' "$m" "${APACHE_MODDIR}" "$m"
        else
            echo "# mod_$m.so was not found in ${APACHE_MODDIR}"
        fi
    done
} > "$PREFIX/apache/conf/extra/httpd-modules.conf"

say "Writing configuration files (existing ones are kept)"
render "$SRC/templates/apache/conf/httpd.conf"              "$PREFIX/apache/conf/httpd.conf"
render "$SRC/templates/apache/conf/extra/httpd-ssl.conf"    "$PREFIX/apache/conf/extra/httpd-ssl.conf"
render "$SRC/templates/apache/conf/extra/httpd-vhosts.conf" "$PREFIX/apache/conf/extra/httpd-vhosts.conf"
render "$SRC/templates/php/php.ini"                         "$PREFIX/php/php.ini"
render "$SRC/templates/php/php-fpm.conf"                    "$PREFIX/php/php-fpm.conf"
render "$SRC/templates/mariadb/my.cnf"                        "$PREFIX/mariadb/my.cnf"
render "$SRC/templates/phpMyAdmin/config.inc.php"           "$PREFIX/phpMyAdmin/config.inc.php"
render "$SRC/templates/tomcat/conf/tomcat-users.xml"        "$PREFIX/tomcat/conf/tomcat-users.xml"
if [ ! -e "$PREFIX/ftp/lamppost-ftp.ini" ] || [ "$FORCE_CONFIG" -eq 1 ]; then
    FTP_PASSWORD=$(openssl rand -base64 12 | tr -d '/+=' | cut -c1-12)
    render "$SRC/templates/ftp/lamppost-ftp.ini"    "$PREFIX/ftp/lamppost-ftp.ini"
    chmod 600 "$PREFIX/ftp/lamppost-ftp.ini"
    say "FTP login: user \"lamppost\", password \"$FTP_PASSWORD\" (in ftp/lamppost-ftp.ini)"
fi

# Migrate configs from the earlier root-based LAMPPost (only our own lines)
sed -i -E '/^# Apache starts as root \(for port 80\/443\)/,/^Group apache$/d' "$PREFIX/apache/conf/httpd.conf"
sed -i -E '/^(user = |group = apache|listen\.owner = |listen\.group = apache)/d; s/^listen\.mode = 0660$/listen.mode = 0600/' \
    "$PREFIX/php/php-fpm.conf"
rm -f "$PREFIX/ftp/proftpd."{conf,pid,scoreboard,delay} \
      "$PREFIX/ftp/logs/proftpd.log" "$PREFIX/ftp/logs/xferlog"

if [ ! -f "$PREFIX/apache/conf/ssl.crt/server.crt" ]; then
    say "Creating a self-signed HTTPS certificate for localhost"
    openssl req -x509 -newkey rsa:2048 -nodes -days 3650 -sha256 \
        -subj "/CN=localhost/O=LAMPPost" \
        -addext "subjectAltName=DNS:localhost,IP:127.0.0.1,IP:::1" \
        -addext "basicConstraints=critical,CA:FALSE" \
        -keyout "$PREFIX/apache/conf/ssl.key/server.key" \
        -out "$PREFIX/apache/conf/ssl.crt/server.crt" 2>/dev/null
    chmod 600 "$PREFIX/apache/conf/ssl.key/server.key"
fi

say "Tomcat base directory"
for f in server.xml web.xml context.xml catalina.properties catalina.policy logging.properties jaspic-providers.xml; do
    [ -e "$PREFIX/tomcat/conf/$f" ] || cp "${TOMCAT_CONF:-/etc/tomcat}/$f" "$PREFIX/tomcat/conf/$f" 2>/dev/null || true
done
if [ -f "$PREFIX/tomcat/conf/server.xml" ]; then
    # only reachable from this computer
    if ! grep -q 'address="127.0.0.1"' "$PREFIX/tomcat/conf/server.xml"; then
        sed -i -E '0,/<Connector port="8080"/s//<Connector port="8080" address="127.0.0.1"/' \
            "$PREFIX/tomcat/conf/server.xml"
    fi
    for app in ROOT manager host-manager; do
        [ -e "$PREFIX/tomcat/webapps/$app" ] || \
            cp -r "${TOMCAT_WEBAPPS:-/var/lib/tomcat/webapps}/$app" "$PREFIX/tomcat/webapps/$app" 2>/dev/null || true
    done
else
    say "Note: no Tomcat configuration found - the Tomcat module will not start."
    say "      Everything else works; install the tomcat package to get it."
fi

say "Installing web root and dashboard"
if ! ls "$PREFIX/htdocs"/index.* >/dev/null 2>&1; then
    cp "$SRC/htdocs/index.php" "$PREFIX/htdocs/index.php"
fi
mkdir -p "$PREFIX/htdocs/dashboard"
cp "$SRC/htdocs/dashboard/"*.php "$PREFIX/htdocs/dashboard/"
cp "$SRC/assets/lamppost.svg" "$PREFIX/htdocs/dashboard/lamppost.svg"

say "Installing control panel, servers, command line tool and shortcuts"
# The control panel is a single Rust binary: prebuilt in a package, or built
# here from panel/ (fonts and icons are baked into it).
PANEL_BIN=""
for candidate in "$SRC/panel/lamppost-control" "$SRC/panel/target/release/lamppost-control"; do
    [ -x "$candidate" ] && PANEL_BIN=$candidate && break
done
if [ -z "$PANEL_BIN" ] && command -v cargo >/dev/null 2>&1; then
    say "Building the control panel (cargo build --release)"
    if (cd "$SRC/panel" && cargo build --release); then
        PANEL_BIN=$SRC/panel/target/release/lamppost-control
    fi
fi
if [ -n "$PANEL_BIN" ]; then
    install -m 755 "$PANEL_BIN" "$PREFIX/lamppost-control"
else
    say "Note: no control panel binary and no Rust toolchain to build one."
    say "      Everything works from the terminal ($PREFIX/lamppost start|stop|status)."
    say "      For the window, install Rust (cargo) and run this again, or use a package."
fi
# leftovers of the old Python control panel
rm -rf "$PREFIX/panel" "$PREFIX/fonts"
install -m 755 "$SRC/src/lamppost" "$PREFIX/lamppost"
install -m 644 "$SRC/src/distro.sh" "$PREFIX/distro.sh"
install -m 755 "$SRC/src/lamppost-ftpd" "$PREFIX/ftp/lamppost-ftpd"
install -m 755 "$SRC/src/mail-catcher" "$PREFIX/mail/mail-catcher"
sed "s|@PREFIX@|$PREFIX|g" "$SRC/src/mailtodisk" > "$PREFIX/mailtodisk/mailtodisk"
chmod 755 "$PREFIX/mailtodisk/mailtodisk"
install -m 644 "$SRC/assets/lamppost.svg" "$PREFIX/icons/lamppost.svg"
for tool in mysql:mariadb mariadb:mariadb mysqldump:mariadb-dump mysqladmin:mariadb-admin; do
    name=${tool%%:*}; real=${tool#*:}
    [ -x "/usr/bin/$real" ] || real=${real/mariadb/mysql}
    printf '#!/bin/sh\n# LAMPPost: %s for the LAMPPost database\nexec /usr/bin/%s --defaults-file=%s/mariadb/my.cnf "$@"\n' \
        "$name" "$real" "$PREFIX" > "$PREFIX/mariadb/bin/$name"
    chmod 755 "$PREFIX/mariadb/bin/$name"
done
printf '#!/bin/sh\n# LAMPPost: PHP command line with LAMPPost'"'"'s php.ini\nexec %s -c %s/php/php.ini "$@"\n' "${PHP_BIN:-/usr/bin/php}" "$PREFIX" > "$PREFIX/php/php"
chmod 755 "$PREFIX/php/php"
cp "$SRC/README.md" "$PREFIX/readme_en.txt"

if [ ! -d "$PREFIX/mariadb/data/mysql" ]; then
    say "Initializing the MariaDB database (user root, no password - like XAMPP)"
    "$MARIADB_INSTALL_DB" --defaults-file="$PREFIX/mariadb/my.cnf" \
        --auth-root-authentication-method=normal --skip-test-db \
        > "$PREFIX/mariadb/install-db.log" 2>&1 \
        || die "mariadb-install-db failed - see $PREFIX/mariadb/install-db.log"
fi

say "Adding LAMPPost to the app grid"
mkdir -p ~/.local/share/applications ~/.local/share/icons/hicolor/scalable/apps
install -m 644 "$SRC/assets/lamppost.svg" ~/.local/share/icons/hicolor/scalable/apps/lamppost.svg
sed "s|@PREFIX@|$PREFIX|g" "$SRC/src/lamppost-control.desktop" > ~/.local/share/applications/lamppost-control.desktop
rm -f ~/.local/share/applications/lixampp-control.desktop \
      ~/.local/share/icons/hicolor/scalable/apps/lixampp.svg
if [ -f ~/.config/autostart/lixampp-control.desktop ]; then
    rm -f ~/.config/autostart/lixampp-control.desktop
    sed "s|@PREFIX@|$PREFIX|g" "$SRC/src/lamppost-control.desktop" \
        > ~/.config/autostart/lamppost-control.desktop
    sed -i 's|^Exec=.*|Exec='"$PREFIX"'/lamppost-control --tray|' \
        ~/.config/autostart/lamppost-control.desktop
fi
update-desktop-database -q ~/.local/share/applications 2>/dev/null || true
gtk-update-icon-cache -q -f -t ~/.local/share/icons/hicolor 2>/dev/null || true

if [ -n "$WEB_GROUP" ] && ! id -nG | tr ' ' '\n' | grep -qx "$WEB_GROUP"; then
    say "Log out and back in once, so phpMyAdmin can read its configuration (new group '$WEB_GROUP')"
fi
say "Done! Open \"LAMPPost Control Panel\" from the app grid, or run: $PREFIX/lamppost start"
