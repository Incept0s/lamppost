# Contributing

Thanks for taking a look. This is a small spare-time project, so the rules are
short.

## Reporting a bug

Open an issue and include:

* your Fedora version (`cat /etc/os-release`) and desktop (GNOME, KDE, …)
* what you did and what happened instead
* the panel's log (the bottom half of the window) and, if it is about a
  module, the relevant file from `/opt/lamppost/*/logs/`

If the control panel confused you or a message was unclear, that is worth an
issue too. Bad wording is a bug.

## Sending a patch

1. `make lint` must pass (it runs `bash -n`, `shellcheck`, compiles the Python
   servers and builds the Rust control panel).
2. Test on a real Fedora machine and say in the pull request what you tested.
3. Match the surrounding style: plain Python, no new dependencies, comments
   that explain *why* rather than what.
4. One change per pull request.

## Things that are welcome

* Support for more distributions: add the paths to `src/distro.sh` and the
  package names to `install.sh` and `panel/src/packages.rs`.
* Translations of the control panel.
* Testing on RHEL-family and Ubuntu systems.

## Things that are out of scope

* Bundling Apache, MariaDB or PHP. LAMPPost deliberately uses the
  distribution's packages.
* Anything that needs a permanent root helper or a passwordless polkit rule.
* Windows and macOS support.

## Development

```bash
git clone https://github.com/Incept0s/lamppost.git
cd lamppost
./install.sh            # installs into /opt/lamppost
make lint               # before every commit
make rpm                # build an RPM locally
make deb                # build a .deb (uses a Debian container when needed)
make appimage           # build an AppImage
```

The control panel can be run straight from the source tree, against the
installed instance:

```bash
cd panel && cargo run --release
```

Rust 1.88 or newer is needed. Distribution packages of the servers are never
bundled - if you need a different Apache or PHP, install it with your package
manager and LAMPPost will use it.
