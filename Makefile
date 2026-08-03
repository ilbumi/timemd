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
        cov cov-rust cov-web e2e image frontend serve dev clean

# Tag whose released binaries `image` wraps, and what to call the result.
IMAGE ?= timemd:$(TAG)

help: ## Show available targets
	@grep -hE '^[a-z][a-z-]*:.*?## ' $(MAKEFILE_LIST) \
		| awk 'BEGIN {FS = ":.*?## "} {printf "  \033[36m%-9s\033[0m %s\n", $$1, $$2}'

deps:
	cd frontend && $(PNPM) install --frozen-lockfile

test: test-rust test-web ## Run the Rust and frontend test suites

# The property tests draw a fresh seed every run, so the default 256 cases can
# pass here and fail on CI minutes later — which is exactly how a lossy `##`
# heading round-trip reached main. Sixteen times the cases costs about a second
# and makes the local gate mean what it claims. Failures found anywhere are
# pinned in the `.proptest-regressions` files and replay first.
test-rust: export PROPTEST_CASES ?= 4096
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

# The image is never a second compile: it wraps binaries a release already
# published, so this needs neither Rust nor Node and does the same thing on a Mac
# as on the runner — which is why it belongs here and not only in the workflow.
# The two Linux targets are the Linux half of `build-binaries.yml`'s matrix; add
# one there and it has to be added here too, or it silently misses the image.
#
# `install -D` is GNU-only, so the recipe sticks to `mkdir -p`.
#
# What the smoke test asserts, in order: the binary answers at all; the server
# comes up; `/` is not a 404, which is what a binary built without `make frontend`
# serves — an exit code rather than a message to keep in sync; and, with nothing
# mounted, that uid 65532 can write /data as the image ships it, the failure mode
# a `RUN mkdir` would have introduced.
image: ## Build the container image from a released tag and prove it runs (TAG=v0.1.0)
	@test -n "$(TAG)" || { echo "TAG is required, e.g. make image TAG=v0.1.0"; exit 1; }
	rm -rf dist/image
	set -eu; \
	stage() { \
	  gh release download "$(TAG)" --pattern "timemd-$$1.tar.gz" --dir dist --clobber; \
	  tar -xzf "dist/timemd-$$1.tar.gz" -C dist; \
	  mkdir -p "dist/image/$$2"; \
	  install -m755 "dist/timemd-$$1/timemd" "dist/image/$$2/timemd"; \
	}; \
	stage x86_64-unknown-linux-gnu  amd64; \
	stage aarch64-unknown-linux-gnu arm64
	docker buildx build --load -f Dockerfile -t $(IMAGE) dist/image
	docker run --rm $(IMAGE) --version
	-docker rm -f timemd-smoke >/dev/null 2>&1
	docker run -d --name timemd-smoke -p 127.0.0.1:18080:8080 $(IMAGE)
	curl --retry 15 --retry-delay 1 --retry-all-errors --retry-connrefused -fsS \
		http://127.0.0.1:18080/api/health
	curl -fsS -o /dev/null http://127.0.0.1:18080/
	docker exec timemd-smoke /usr/local/bin/timemd start smoke
	docker rm -f timemd-smoke

frontend: deps ## Build the web UI into the server crate for embedding
	cd frontend && $(PNPM) run build

serve: frontend ## Build the UI and run the app locally
	$(CARGO) run --bin timemd -- serve --addr 127.0.0.1:8080

dev: ## Run the Vite dev server, proxying /api to a separately-running `make serve`
	cd frontend && $(PNPM) run dev

clean: ## Remove build artifacts
	$(CARGO) clean
	rm -rf frontend/.svelte-kit frontend/node_modules crates/server/assets/_app
