CARGO_FLAGS ?= --release
CARGO_FEATURES ?= --features=capi

MACHINE := $(shell getconf LONG_BIT)
ifeq ($(MACHINE), 64)
LIB ?= lib64
else
LIB ?= lib
endif

export PREFIX=/usr
export PKG_CONFIG_PATH+=:$(PWD)/inst/$(PREFIX)/$(LIB)/pkgconfig
export GI_TYPELIB_PATH+=:$(PWD)/inst/$(PREFIX)/$(LIB)/girepository-1.0
export LD_LIBRARY_PATH=$(PWD)/inst/$(PREFIX)/$(LIB)

HEADER = inst/$(PREFIX)/include/spice-gtk-usb-portal/spice-usb-portal.h
GIR = inst/$(PREFIX)/share/gir-1.0/SpiceUsbPortal-0.1.gir
TYPELIB = inst/$(PREFIX)/$(LIB)/girepository-1.0/SpiceUsbPortal-0.1.typelib
VAPI = inst/$(PREFIX)/share/vala/vapi/spice-gtk-usb-portal.vapi
DOC_DIR = inst/$(PREFIX)/share/doc/spice-gtk-usb-portal
DOC_INDEX = $(DOC_DIR)/index.html
DOC_CONFIG = docs/SpiceUsbPortal.toml

# Extra include paths for gi-docgen, derived from XDG_DATA_DIRS so it can find
# Gtk-4.0.gir / SpiceClientGLib-2.0.gir / GObject-2.0.gir from the dev shell.
GIR_INCLUDE_DIRS := $(sort $(wildcard $(addsuffix /gir-1.0,$(subst :, ,$(XDG_DATA_DIRS)))))
GIR_INCLUDE_FLAGS := $(addprefix --add-include-path=,$(GIR_INCLUDE_DIRS))

RUST_SOURCES = $(shell find src) Cargo.toml build.rs

HAVE_VAPIGEN := $(shell command -v vapigen 2>/dev/null)

all: $(GIR) $(TYPELIB)
ifneq ($(HAVE_VAPIGEN),)
all: $(VAPI)
endif

vapi: $(VAPI)

doc: $(DOC_INDEX)

$(DOC_INDEX): $(GIR) $(DOC_CONFIG)
	mkdir -p $(@D)
	gi-docgen generate \
		--config=$(DOC_CONFIG) \
		--output-dir=$(DOC_DIR) \
		--no-namespace-dir \
		$(GIR_INCLUDE_FLAGS) \
		$(GIR)

$(HEADER): $(RUST_SOURCES)
	cargo cinstall $(CARGO_FLAGS) $(CARGO_FEATURES) --destdir=inst --prefix=$(PREFIX) --libdir=$(PREFIX)/$(LIB)

$(GIR): $(HEADER)
	mkdir -p $(@D)
	g-ir-scanner -v --warn-all \
		--namespace SpiceUsbPortal --nsversion=0.1 \
		--identifier-prefix SpiceUsbPortal \
		--symbol-prefix spice_usb_portal \
		--c-include "spice-usb-portal.h" \
		-Iinst/$(PREFIX)/include/spice-gtk-usb-portal \
		-Iinst/$(PREFIX)/include \
		-I$(PREFIX)/include \
		-Linst/$(PREFIX)/$(LIB) \
		-L$(PREFIX)/$(LIB) \
		--include=Gtk-4.0 --pkg gtk4 \
		--include=SpiceClientGLib-2.0 --pkg spice-client-glib-2.0 \
		--library=spice-gtk-usb-portal \
		--output $@ \
		$<
	# Strip absolute path from shared-library= so the loader resolves the
	# library via LD_LIBRARY_PATH / ldconfig at runtime.
	sed -i 's|shared-library="[^"]*/\(libspice-gtk-usb-portal\.so[^"]*\)"|shared-library="\1"|' $@

$(TYPELIB): $(GIR)
	mkdir -p $(@D)
	g-ir-compiler $< -o $@

$(VAPI): $(GIR)
	mkdir -p $(@D)
	vapigen \
		--pkg gtk4 \
		--pkg spice-client-glib-2.0 \
		--library spice-gtk-usb-portal \
		$< -d $(@D)
	echo gtk4 > $(@D)/spice-gtk-usb-portal.deps
	echo spice-client-glib-2.0 >> $(@D)/spice-gtk-usb-portal.deps

install: all
	cp -r inst/* $(DESTDIR)/

clean:
	rm -rf inst target

.PHONY: all vapi doc install clean
