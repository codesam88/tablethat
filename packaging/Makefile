# tablethat tarball install — no dependencies beyond coreutils `install(1)`.
# Installs to user-local paths by default (no sudo needed):
#
#   tar xzf tablethat-<target>.tar.gz && cd tablethat
#   make install                         # -> ~/.local
#   make install PREFIX=/usr/local       # system-wide (needs sudo)
#   make install DESTDIR=/tmp/stage      # staged install (packagers)
#   make uninstall                       # removes exactly what install added
#
# System-wide themes land in <prefix>/share/tablethat/themes; per-user
# config in ~/.config/{plan,gloss}/ still takes precedence.

PREFIX ?= $(HOME)/.local
BINDIR = $(DESTDIR)$(PREFIX)/bin
THEMEDIR = $(DESTDIR)$(PREFIX)/share/tablethat/themes
DOCDIR = $(DESTDIR)$(PREFIX)/share/doc/tablethat
DOCS = README.md plan.toml.example LICENSE-MIT LICENSE-APACHE

.PHONY: install uninstall

install:
	install -D -m755 plan $(BINDIR)/plan
	install -D -m755 gloss $(BINDIR)/gloss
	install -D -m644 -t $(THEMEDIR)/ themes/*.toml
	install -D -m644 -t $(DOCDIR)/ $(DOCS)

uninstall:
	rm -f $(BINDIR)/plan $(BINDIR)/gloss
	rm -f $(THEMEDIR)/*.toml
	rm -f $(addprefix $(DOCDIR)/,$(DOCS))
	-rmdir -p --ignore-fail-on-non-empty $(BINDIR) $(THEMEDIR) $(DOCDIR) 2>/dev/null; true
