CARGO ?= cargo
PNPM  ?= pnpm

# Coverage floors from the project guidelines: >85% package, >80% per file.
COVERAGE_MIN ?= 85

# `crates/cli/src/main.rs` is excluded: it is a process shim (argument parsing
# into a call, logging setup, exit code) with no branch a test could exercise
# without spawning the binary. Its logic lives in the cli crate's library, which
# is measured.
COVERAGE_EXCLUDE ?= crates/cli/src/main\.rs

.DEFAULT_GOAL := help
.PHONY: help test lint fmt cov frontend serve dev clean

help: ## Show available targets
	@grep -hE '^[a-z][a-z-]*:.*?## ' $(MAKEFILE_LIST) \
		| awk 'BEGIN {FS = ":.*?## "} {printf "  \033[36m%-9s\033[0m %s\n", $$1, $$2}'

test: ## Run the Rust and frontend test suites
	$(CARGO) test --workspace --all-features
	cd frontend && $(PNPM) run test

lint: ## Deny clippy warnings, check formatting and type-check the UI
	$(CARGO) clippy --workspace --all-targets --all-features -- -D warnings
	$(CARGO) fmt --all --check
	cd frontend && $(PNPM) run check && $(PNPM) run lint

fmt: ## Format everything
	$(CARGO) fmt --all
	cd frontend && $(PNPM) run format

cov: ## Measure coverage, failing under the floor
	@command -v cargo-llvm-cov >/dev/null 2>&1 \
		|| { echo "cargo-llvm-cov missing: cargo install cargo-llvm-cov --locked"; exit 1; }
	$(CARGO) llvm-cov --workspace --all-features \
		--ignore-filename-regex '$(COVERAGE_EXCLUDE)' \
		--fail-under-lines $(COVERAGE_MIN)
	cd frontend && $(PNPM) run coverage

frontend: ## Build the web UI into the server crate for embedding
	cd frontend && $(PNPM) install --frozen-lockfile && $(PNPM) run build

serve: frontend ## Build the UI and run the app locally
	$(CARGO) run --bin timemd -- serve --addr 127.0.0.1:8080

dev: ## Run the Vite dev server, proxying /api to a separately-running `make serve`
	cd frontend && $(PNPM) run dev

clean: ## Remove build artifacts
	$(CARGO) clean
	rm -rf frontend/.svelte-kit frontend/node_modules crates/server/assets/_app
