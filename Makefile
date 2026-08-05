SHELL := /bin/bash

export RUST_BACKTRACE ?= 1
export WASMTIME_BACKTRACE_DETAILS ?= 1

COMPONENTS = $(sort $(notdir $(patsubst %/,%,$(dir $(wildcard  components/*/Cargo.toml)))))

.PHONY: all
all: components

.PHONY: clean
clean:
	cargo clean
	rm -rf lib/*.wasm
	rm -rf lib/*.wasm.md

.PHONY: test
test:
	@echo "TODO add tests"

.PHONY: components
components: lib/interface.wasm $(foreach component,$(COMPONENTS),lib/$(component).wasm $(foreach component,$(COMPONENTS),lib/$(component).debug.wasm))

define BUILD_COMPONENT

.PHONY: components/$1
components/$1: lib/$1.wasm lib/$1.debug.wasm

lib/$1.wasm: Cargo.toml Cargo.lock components/wit/deps $(shell find components/$1 -type f)
	cargo build -p $1 --target wasm32-unknown-unknown --release
	wasm-tools component new target/wasm32-unknown-unknown/release/$(subst -,_,$1).wasm -o lib/$1.wasm
	cp components/$1/README.md lib/$1.wasm.md

lib/$1.debug.wasm: Cargo.toml Cargo.lock components/wit/deps $(shell find components/$1 -type f)
	cargo build -p $1 --target wasm32-unknown-unknown
	wasm-tools component new target/wasm32-unknown-unknown/debug/$(subst -,_,$1).wasm -o lib/$1.debug.wasm
	cp components/$1/README.md lib/$1.debug.wasm.md

endef

$(foreach component,$(COMPONENTS),$(eval $(call BUILD_COMPONENT,$(component))))

lib/interface.wasm: wit/deps README.md
	wkg build -o lib/interface.wasm
	cp README.md lib/interface.wasm.md

.PHONY: wit
wit: wit/deps components/wit/deps

wit/deps: wkg.toml $(shell find wit -type f -name "*.wit" -not -path "deps")
	wkg fetch

components/wit/deps: wit/deps components/wkg.toml $(shell find components/wit -type f -name "*.wit" -not -path "deps")
	( cd components && wkg fetch )

.PHONY: publish
publish: $(shell find lib -type f -name "*.wasm" | sed -e 's:^lib/:publish-:g')

.PHONY: publish-%
publish-%:
ifndef VERSION
	$(error VERSION is undefined)
endif
ifndef REPOSITORY
	$(error REPOSITORY is undefined)
endif
	@$(eval FILE := $(@:publish-%=%))
	@$(eval COMPONENT := $(FILE:%.wasm=%))
	@$(eval DESCRIPTION := $(shell head -n 3 "lib/${FILE}.md" | tail -n 1))
	@$(eval REVISION := $(shell git rev-parse HEAD)$(shell git diff --quiet HEAD && echo "+dirty"))
	@$(eval TAG := $(patsubst v%,%,$(subst +,_,$(VERSION))))

	@echo "::group::${FILE} -> ${REPOSITORY}/${COMPONENT}:${TAG}"
	@DIGEST=$$( \
		wkg oci push \
			--annotation "org.opencontainers.image.title=${COMPONENT}" \
			--annotation "org.opencontainers.image.description=${DESCRIPTION}" \
			--annotation "org.opencontainers.image.version=${VERSION}" \
			--annotation "org.opencontainers.image.source=https://github.com/${GITHUB_REPOSITORY}.git" \
			--annotation "org.opencontainers.image.revision=${REVISION}" \
			--annotation "org.opencontainers.image.licenses=Apache-2.0" \
			"${REPOSITORY}/${COMPONENT}:${TAG}" \
			"lib/${FILE}" \
			2>&1 \
			| tee /dev/stderr \
			| grep -o 'sha256:[a-f0-9]\{64\}' \
	) ; \
	cosign sign --yes "${REPOSITORY}/${COMPONENT}:${TAG}@$${DIGEST}"
	@echo "::endgroup::"
