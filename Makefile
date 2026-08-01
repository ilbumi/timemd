CARGO ?= cargo

# Coverage floors from the project guidelines: >85% package, >80% per file.
COVERAGE_MIN ?= 85

.DEFAULT_GOAL := help
.PHONY: help test lint fmt cov serve clean

help: ## Show available targets
	@grep -hE '^[a-z][a-z-]*:.*?## ' $(MAKEFILE_LIST) \
		| awk 'BEGIN {FS = ":.*?## "} {printf "  \033[36m%-8s\033[0m %s\n", $$1, $$2}'

test: ## Run the test suite
	$(CARGO) test --workspace --all-features

lint: ## Deny clippy warnings and check formatting
	$(CARGO) clippy --workspace --all-targets --all-features -- -D warnings
	$(CARGO) fmt --all --check

fmt: ## Format the workspace
	$(CARGO) fmt --all

cov: ## Measure coverage, failing under the floor
	@command -v cargo-llvm-cov >/dev/null 2>&1 \
		|| { echo "cargo-llvm-cov missing: cargo install cargo-llvm-cov --locked"; exit 1; }
	$(CARGO) llvm-cov --workspace --all-features --fail-under-lines $(COVERAGE_MIN)

serve: ## Run the app locally
	$(CARGO) run --bin timemd -- serve --addr 127.0.0.1:8080

clean: ## Remove build artifacts
	$(CARGO) clean
