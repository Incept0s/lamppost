# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[semantic versioning](https://semver.org/).

## [1.0] - 2026-09-26

First public release.

### Added
* Control panel for Apache, MariaDB, an FTP server, a mail catcher and Tomcat,
  with the XAMPP folder layout in `/opt/lamppost`.
* Everything runs as your own user — no root, no password prompts for starting
  and stopping.
* The panel is a single Rust binary that needs no Qt, GTK or Python runtime,
  and draws its own window frame so the title bar, its buttons and the contents
  share one theme.
* Tray icon in its own small process (about 9 MB, no graphics stack): closing
  the window really closes it and LAMPPost stays in the top bar, which is the
  only way that works on Wayland — a window there cannot hide itself.
* Two looks, switchable while running: **Modern** (rounded cards, Nunito,
  light/dark following the desktop or pinned) and **Classic** (the plain grey look of
  old Windows control panels). All text colours meet the WCAG AA contrast ratio.
* **Versions and Updates**: shows the installed version of every component and
  installs newer ones through `dnf` or `apt` with a single password prompt,
  stopping and restarting affected modules.
* Fedora and Debian support. Where the distribution keeps Apache, PHP-FPM,
  MariaDB, Tomcat and phpMyAdmin is detected (`src/distro.sh`) instead of
  assumed, and the Apache module list is generated for the Apache that is
  installed.
* Missing packages are installed on demand: starting a module that is not
  installed opens a dialog listing the required packages, which are installed
  with one password prompt (`lamppost missing` / `lamppost install` on the
  command line). The privileged step only accepts module names and derives
  the package list itself.
* Netstat shows every listening socket with its scope (local or reachable
  from the network), the full program name and which LAMPPost module it
  belongs to - or blocks.
* Dashboard at `http://localhost/` showing module status and versions.
* RPM, DEB and AppImage packages, built by CI for every release. Each ships
  `THIRD-PARTY-LICENSES.txt` with the licence notices of the 342 Rust crates
  compiled into the control panel.

### Notes
* This project was called LIXAMPP while it was a private experiment. The name
  was changed before publication because "XAMPP" is a registered trademark of
  BitRock, and the modules were renamed to what they actually are: the FTP
  server is pyftpdlib, the mail catcher is aiosmtpd, the database is MariaDB.
  `install.sh` takes over an existing LIXAMPP installation, keeping databases
  and `htdocs`.
