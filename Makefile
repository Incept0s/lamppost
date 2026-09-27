# LAMPPost - development helpers.
# Users do not need make: ./install.sh does everything.

VERSION := 1.0
NAME    := lamppost
TARBALL := $(NAME)-$(VERSION).tar.gz
TOPDIR  := $(CURDIR)/rpmbuild

.PHONY: help install uninstall lint panel tarball rpm deb appimage dist clean

help:
	@echo "make install    - install LAMPPost from this source tree (./install.sh)"
	@echo "make uninstall  - remove LAMPPost (keeps /opt/lamppost)"
	@echo "make lint       - check the shell scripts and compile the Python"
	@echo "make panel      - build the control panel (cargo build --release)"
	@echo "make rpm        - build an RPM into rpmbuild/RPMS/"
	@echo "make deb        - build a .deb into dist/ (uses a Debian container when needed)"
	@echo "make appimage   - build an AppImage into dist/"
	@echo "make dist       - build all three packages"
	@echo "make clean      - remove build output"

install:
	./install.sh

uninstall:
	./uninstall.sh

lint:
	bash -n install.sh uninstall.sh src/lamppost src/distro.sh
	cd panel && cargo build --release --quiet
	python3 -m py_compile src/lamppost-ftpd src/mail-catcher
	@command -v shellcheck >/dev/null && shellcheck -S warning install.sh uninstall.sh src/lamppost src/distro.sh || \
	        echo "shellcheck not installed - skipped"
	@command -v php >/dev/null && php -l htdocs/index.php > /dev/null && \
	        php -l htdocs/dashboard/index.php > /dev/null && echo "PHP syntax ok" || \
	        echo "php not installed - skipped"
	@echo "lint ok"

tarball:
	@mkdir -p $(TOPDIR)/SOURCES
	git archive --format=tar.gz --prefix=$(NAME)-$(VERSION)/ \
	    -o $(TOPDIR)/SOURCES/$(TARBALL) HEAD 2>/dev/null || \
	  tar --exclude-vcs --exclude=rpmbuild --transform 's,^\.,$(NAME)-$(VERSION),' \
	      -czf $(TOPDIR)/SOURCES/$(TARBALL) .
	@echo "$(TOPDIR)/SOURCES/$(TARBALL)"

rpm: tarball
	rpmbuild --define "_topdir $(TOPDIR)" -bb packaging/$(NAME).spec
	@find $(TOPDIR)/RPMS -name "*.rpm"

panel:
	cd panel && cargo build --release

deb: panel
	@if command -v dpkg-deb >/dev/null; then \
	    packaging/build-deb.sh $(VERSION); \
	else \
	    echo "not a Debian system - building in a container"; \
	    podman run --rm -v $(CURDIR):/src:Z -w /src docker.io/library/debian:13 sh -c \
	      'apt-get update -qq && apt-get install -y -qq --no-install-recommends build-essential dpkg-dev curl ca-certificates python3 >/dev/null && \
	       curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable >/dev/null 2>&1 && \
	       . $$HOME/.cargo/env && CARGO_TARGET_DIR=/tmp/target packaging/build-deb.sh $(VERSION)'; \
	fi

appimage: panel
	packaging/build-appimage.sh $(VERSION)

dist: rpm deb appimage
	@ls -lh dist/ rpmbuild/RPMS/*/*.rpm 2>/dev/null

clean:
	rm -rf $(TOPDIR) dist build panel/target src/__pycache__
