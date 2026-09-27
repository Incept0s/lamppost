%global appname lamppost
# the Rust binary is stripped at build time; no -debuginfo/-debugsource packages
%global debug_package %{nil}

Name:           lamppost
Version:        1.0
Release:        1%{?dist}
Summary:        An XAMPP-style local web stack built on Fedora's own packages

License:        MIT
URL:            https://github.com/Incept0s/lamppost
Source0:        %{name}-%{version}.tar.gz

# the control panel is a Rust program, so the package is architecture specific
BuildRequires:  cargo
BuildRequires:  rust
BuildRequires:  make
BuildRequires:  python3

# the stack LAMPPost starts for you
Requires:       httpd
Requires:       mod_ssl
Requires:       php-fpm
Requires:       php-cli
Requires:       php-mysqlnd
Requires:       php-gd
Requires:       php-mbstring
Requires:       php-intl
Requires:       php-xml
Requires:       php-pecl-zip
Requires:       php-sodium
Requires:       mariadb-server
Requires:       mariadb
Requires:       phpMyAdmin
Requires:       python3-pyftpdlib
Requires:       python3-aiosmtpd
Requires:       tomcat
Requires:       tomcat-webapps
Requires:       tomcat-admin-webapps
# the setup script
Requires:       polkit
Requires:       iproute
Requires:       openssl
Requires:       bash

%description
LAMPPost gives you an XAMPP-style workflow on Fedora: one window with Start and
Stop buttons for Apache, MariaDB, an FTP server, a mail catcher and Tomcat,
plus an htdocs folder for your PHP files and the familiar XAMPP folder layout.

Unlike XAMPP it ships no servers of its own. It starts Fedora's packages with
its own configuration in /opt/lamppost, so your local server is always the
up-to-date, security-patched software from your distribution. Every module
runs as your own user, so starting and stopping never needs a password.

This package installs the program files. Run "lamppost-setup" once as your
normal user to create your personal instance in /opt/lamppost.

%prep
%autosetup -n %{name}-%{version}

%build
# the control panel (fonts and icons are baked into the binary)
cd panel
cargo build --release
cd ..
# licence notices of the crates compiled into the panel
python3 packaging/third-party-licenses.py THIRD-PARTY-LICENSES.txt

%install
install -d %{buildroot}%{_datadir}/%{appname}
cp -a src templates htdocs assets install.sh uninstall.sh README.md LICENSE THIRD-PARTY.md \
      %{buildroot}%{_datadir}/%{appname}/
install -Dpm0755 panel/target/release/lamppost-control \
      %{buildroot}%{_datadir}/%{appname}/panel/lamppost-control
install -Dpm0755 packaging/lamppost-setup %{buildroot}%{_bindir}/lamppost-setup
install -Dpm0644 packaging/50-lamppost.conf %{buildroot}%{_sysconfdir}/sysctl.d/50-lamppost.conf
install -Dpm0644 assets/lamppost.svg \
      %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/%{appname}.svg

%post
# ports from 21 upwards for normal programs (see the file for details)
/usr/lib/systemd/systemd-sysctl %{_sysconfdir}/sysctl.d/50-lamppost.conf >/dev/null 2>&1 || :
cat <<'EOM'

LAMPPost is installed. Run this once as your normal user (not with sudo):

    lamppost-setup

It creates your instance in /opt/lamppost and adds LAMPPost to the app grid.

EOM

%postun
if [ $1 -eq 0 ]; then
    sysctl -q -w net.ipv4.ip_unprivileged_port_start=1024 >/dev/null 2>&1 || :
fi

%files
%license LICENSE THIRD-PARTY-LICENSES.txt
%doc README.md THIRD-PARTY.md
%{_datadir}/%{appname}/
%{_bindir}/lamppost-setup
%config(noreplace) %{_sysconfdir}/sysctl.d/50-lamppost.conf
%{_datadir}/icons/hicolor/scalable/apps/%{appname}.svg

%changelog
* Sat Sep 26 2026 Incept0s <noreply@users.noreply.github.com> - 1.0-1
- First public release
