# Build and install the Focus engine (the omarchy-focus daemon).
#
# Builds go to ~/.cache, never inside this folder: the Omarchy shell watches
# its plugins directory and reloads the plugin on every file written there.

PREFIX   ?= $(HOME)/.local
BINDIR   ?= $(PREFIX)/bin
UNITDIR  ?= $(HOME)/.config/systemd/user
CARGO    ?= cargo
export CARGO_TARGET_DIR ?= $(HOME)/.cache/omarchy-focus/target

MANIFEST := daemon/Cargo.toml
BIN      := $(CARGO_TARGET_DIR)/release/omarchy-focus
UNIT     := omarchy-focus.service

.PHONY: all build test install uninstall restart logs

all: build

build:
	$(CARGO) build --release --locked --manifest-path $(MANIFEST)

test:
	$(CARGO) test --locked --manifest-path $(MANIFEST)

install: build
	install -Dm755 $(BIN) $(BINDIR)/omarchy-focus
	install -Dm644 systemd/$(UNIT) $(UNITDIR)/$(UNIT)
	systemctl --user daemon-reload
	systemctl --user enable $(UNIT)
	systemctl --user restart $(UNIT)
	@for i in $$(seq 1 50); do $(BINDIR)/omarchy-focus status >/dev/null 2>&1 && break; sleep 0.1; done
	@$(BINDIR)/omarchy-focus status >/dev/null && echo "Focus engine installed and running. Reload the bar with: omarchy restart shell"

uninstall:
	-systemctl --user disable --now $(UNIT)
	rm -f $(UNITDIR)/$(UNIT) $(BINDIR)/omarchy-focus
	systemctl --user daemon-reload

restart:
	systemctl --user restart $(UNIT)

logs:
	journalctl --user -u $(UNIT) -f
