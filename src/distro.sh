#!/bin/bash
# Where this distribution keeps the programs LAMPPost starts.
#
# Sourced by the "lamppost" command line tool and by install.sh. Fedora and
# Debian put the same software in different places, so everything that differs
# is detected here instead of being hard-coded.

first_file() { for c in "$@"; do [ -x "$c" ] && { printf '%s\n' "$c"; return 0; }; done; return 1; }
first_dir()  { for c in "$@"; do [ -d "$c" ] && { printf '%s\n' "$c"; return 0; }; done; return 1; }

# package family: rpm (Fedora, RHEL) or deb (Debian, Ubuntu)
if command -v rpm >/dev/null 2>&1; then
    LAMPPOST_FAMILY=rpm
elif command -v dpkg-query >/dev/null 2>&1; then
    LAMPPOST_FAMILY=deb
else
    LAMPPOST_FAMILY=unknown
fi

# Apache: "httpd" on Fedora, "apache2" on Debian
APACHE_BIN=$(first_file /usr/bin/httpd /usr/sbin/httpd /usr/sbin/apache2 || true)
APACHE_MODDIR=$(first_dir /usr/lib64/httpd/modules /usr/lib/httpd/modules /usr/lib/apache2/modules || true)

# PHP-FPM: Debian names the binary after the PHP version (php-fpm8.3)
PHPFPM_BIN=$(first_file /usr/bin/php-fpm /usr/sbin/php-fpm || true)
if [ -z "${PHPFPM_BIN:-}" ]; then
    PHPFPM_BIN=$(ls -1 /usr/sbin/php-fpm* /usr/bin/php-fpm* 2>/dev/null | sort -V | tail -n1 || true)
fi
PHP_BIN=$(first_file /usr/bin/php || true)

# MariaDB
MARIADBD_BIN=$(first_file /usr/bin/mariadbd /usr/sbin/mariadbd /usr/sbin/mysqld || true)
MARIADB_INSTALL_DB=$(first_file /usr/bin/mariadb-install-db /usr/bin/mysql_install_db || true)
MARIADB_CLIENT=$(first_file /usr/bin/mariadb /usr/bin/mysql || true)

# phpMyAdmin (Fedora spells it with capitals, Debian does not)
PMA_DIR=$(first_dir /usr/share/phpMyAdmin /usr/share/phpmyadmin || true)
PMA_CONF=""
for candidate in /etc/phpMyAdmin/config.inc.php /etc/phpmyadmin/config.inc.php; do
    [ -f "$candidate" ] && PMA_CONF=$candidate && break
done

# Tomcat
TOMCAT_HOME=$(first_dir /usr/share/tomcat /usr/share/tomcat10 /usr/share/tomcat9 || true)
TOMCAT_CONF=$(first_dir /etc/tomcat /etc/tomcat10 /etc/tomcat9 || true)
TOMCAT_WEBAPPS=$(first_dir /var/lib/tomcat/webapps /var/lib/tomcat10/webapps /var/lib/tomcat9/webapps || true)

JAVA_BIN=$(first_file /usr/bin/java || true)
PYTHON_BIN=$(first_file /usr/bin/python3 || true)

# The group that may read phpMyAdmin's configuration
if getent group apache >/dev/null 2>&1; then
    WEB_GROUP=apache
elif getent group www-data >/dev/null 2>&1; then
    WEB_GROUP=www-data
else
    WEB_GROUP=""
fi

export LAMPPOST_FAMILY APACHE_BIN APACHE_MODDIR PHPFPM_BIN PHP_BIN MARIADBD_BIN \
       MARIADB_INSTALL_DB MARIADB_CLIENT PMA_DIR PMA_CONF TOMCAT_HOME TOMCAT_CONF \
       TOMCAT_WEBAPPS JAVA_BIN PYTHON_BIN WEB_GROUP
