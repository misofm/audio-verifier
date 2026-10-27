REGISTRY ?= local
OUT_DIR ?= out
ENCLAVE_PACKAGE ?= audio-verifier-enclave
ENCLAVE_BIN ?= $(ENCLAVE_PACKAGE)
ENCLAVE_CID ?= 16
ENCLAVE_CPUS ?= 4
ENCLAVE_MEMORY ?= 6144M
EIF_INPUTS := Cargo.toml Cargo.lock rust-toolchain.toml Containerfile.eif .dockerignore $(shell find crates eif scripts -type f -not -path '*/target/*')

.DEFAULT_GOAL := eif

.PHONY: eif
eif: $(OUT_DIR)/nitro.eif

$(OUT_DIR):
	mkdir -p $(OUT_DIR)

$(OUT_DIR)/nitro.eif: $(EIF_INPUTS) | $(OUT_DIR)
	cargo vendor --locked vendor > vendor.toml
	mv vendor.toml vendor/config.toml
	docker build \
		--pull \
		--tag $(REGISTRY)/audio-verifier-enclave \
		--progress=plain \
		--platform linux/amd64 \
		--provenance=false \
		--output type=local,rewrite-timestamp=true,dest=$(OUT_DIR) \
		-f Containerfile.eif \
		--build-arg ENCLAVE_PACKAGE=$(ENCLAVE_PACKAGE) \
		--build-arg ENCLAVE_BIN=$(ENCLAVE_BIN) \
		.

.PHONY: run-eif
run-eif: $(OUT_DIR)/nitro.eif
	sudo nitro-cli run-enclave \
		--cpu-count $(ENCLAVE_CPUS) \
		--memory $(ENCLAVE_MEMORY) \
		--enclave-cid $(ENCLAVE_CID) \
		--eif-path $(OUT_DIR)/nitro.eif

.PHONY: run-eif-debug
run-eif-debug: $(OUT_DIR)/nitro.eif
	@echo 'WARNING: debug mode reports all-zero PCRs; this enclave cannot be attested or registered with Kagi.' >&2
	sudo nitro-cli run-enclave \
		--cpu-count $(ENCLAVE_CPUS) \
		--memory $(ENCLAVE_MEMORY) \
		--enclave-cid $(ENCLAVE_CID) \
		--eif-path $(OUT_DIR)/nitro.eif \
		--debug-mode \
		--attach-console
