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

# Each gate is split into a Rust half and a web half so CI can run them in jobs
# with only the toolchain each needs, without restating the flags that *are* the
# gate. The halves carry no `##`, so `help` still shows the same list it did.
.PHONY: help deps test test-rust test-web lint lint-rust lint-web fmt \
        cov cov-rust cov-web e2e frontend serve dev clean

help: ## Show available targets
	@grep -hE '^[a-z][a-z-]*:.*?## ' $(MAKEFILE_LIST) \
		| awk 'BEGIN {FS = ":.*?## "} {printf "  \033[36m%-9s\033[0m %s\n", $$1, $$2}'

deps:
	cd frontend && $(PNPM) install --frozen-lockfile

test: test-rust test-web ## Run the Rust and frontend test suites

test-rust:
	$(CARGO) test --workspace --all-features

test-web: deps
	cd frontend && $(PNPM) run test

lint: lint-rust lint-web ## Deny clippy warnings, check formatting and type-check the UI

lint-rust:
	$(CARGO) clippy --workspace --all-targets --all-features -- -D warnings
	$(CARGO) fmt --all --check

lint-web: deps
	cd frontend && $(PNPM) run check && $(PNPM) run lint

fmt: ## Format everything
	$(CARGO) fmt --all
	cd frontend && $(PNPM) run format

cov: cov-rust cov-web ## Measure coverage, failing under the floor

# Depends on `frontend` because the UI is part of what is measured: with no
# `assets/` embedded, no path in `assets.rs` ever matches, `respond` is never
# called, and the number comes out lower for a reason nobody would guess.
cov-rust: frontend
	@command -v cargo-llvm-cov >/dev/null 2>&1 \
		|| { echo "cargo-llvm-cov missing: cargo install cargo-llvm-cov --locked"; exit 1; }
	$(CARGO) llvm-cov --workspace --all-features \
		--ignore-filename-regex '$(COVERAGE_EXCLUDE)' \
		--fail-under-lines $(COVERAGE_MIN)

cov-web: deps
	cd frontend && $(PNPM) run coverage

# Deliberately not part of `test`: it needs a downloaded browser and a compiled
# server, and the design it checks is geometry rather than behaviour. The suite
# seeds its own markdown tree under `.e2e-data` and never reads `./data`.
#
# The binary is built here rather than left to Playwright: its `webServer` gives
# `cargo run` 240s to answer on /api/health, and on a cold target directory that
# budget goes entirely on compiling the dependency tree.
e2e: frontend ## Check alignment and adaptive layout in a real browser
	cd frontend && $(PNPM) exec playwright install --with-deps chromium
	$(CARGO) build --bin timemd
	cd frontend && $(PNPM) run e2e

frontend: deps ## Build the web UI into the server crate for embedding
	cd frontend && $(PNPM) run build

serve: frontend ## Build the UI and run the app locally
	$(CARGO) run --bin timemd -- serve --addr 127.0.0.1:8080

dev: ## Run the Vite dev server, proxying /api to a separately-running `make serve`
	cd frontend && $(PNPM) run dev

clean: ## Remove build artifacts
	$(CARGO) clean
	rm -rf frontend/.svelte-kit frontend/node_modules crates/server/assets/_app
