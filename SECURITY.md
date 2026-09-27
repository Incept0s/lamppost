# Security policy

## What LAMPPost is for

LAMPPost is a **local development stack**. Like XAMPP, it is set up for
convenience on your own computer, not for the internet. If you put it on a
server or expose it to your network, you are on your own.

Known and intentional trade-offs:

* The MariaDB user `root` has no password, and phpMyAdmin logs in
  automatically. Anyone who can run programs on the machine can read and
  change your databases.
* PHP displays errors in the browser and the dashboard offers `phpinfo()`.
* Every module listens on `127.0.0.1` only.
* Installing sets `net.ipv4.ip_unprivileged_port_start = 21` system-wide, so
  programs of any user can bind ports from 21 up without root. This is what
  lets Apache run on port 80 as your user. `./uninstall.sh` reverts it.

## Design decisions that protect you

* No part of LAMPPost runs as root. There is no setuid helper and no polkit
  rule that grants passwordless root.
* `/opt/lamppost` belongs to your user. Root never writes configuration there,
  so a writable config file cannot turn into a root exploit.
* LAMPPost asks for a password only for the initial setup, for installing
  updates and for installing a module's missing packages - always through
  `pkexec` running the distribution's package manager. For the last one, the
  privileged step receives module names only and works out the package list
  itself, so no package names can be injected.

## Reporting a vulnerability

Please report security problems **privately**: open a
[GitHub security advisory](../../security/advisories/new) rather than a public
issue.

Include what you did, what happened, and what you expected. A proof of concept
helps. Expect a first reply within a week; this is a spare-time project, so
please be patient.

Out of scope: the intentional trade-offs listed above, and anything that
requires an attacker to already have a shell on your machine as your user.
