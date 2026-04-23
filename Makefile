.DEFAULT_GOAL := help

.PHONY: help build test fmt clippy client client-build mapgen terrain-cache wasm-target trunk web-setup web-build web-serve

help: ## Show available targets
	@awk 'BEGIN {FS = ": ## "}; /^[a-zA-Z0-9_.-]+: ## / {printf "\033[36m%-16s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)

build: ## Build the workspace
	cargo build

test: ## Run the test suite
	cargo test --quiet

fmt: ## Format the workspace
	cargo fmt

clippy: ## Run Clippy checks
	cargo clippy --all-targets

client-build: ## Build the native editor client
	cargo build -p client

client: ## Run the native editor client
	cargo run -p client

mapgen: ## Generate assets/map.bin and assets/terrain.bin
	cargo run --release -p mapgen

terrain-cache: ## Generate assets/terrain_adjacency.bin
	cargo run --release -p terrain_cache

wasm-target: ## Install the wasm32 target
	rustup target add wasm32-unknown-unknown

trunk: ## Install Trunk if missing
	trunk --version >/dev/null 2>&1 || cargo install trunk --locked

web-setup: wasm-target trunk ## Prepare local web build prerequisites

web-build: web-setup ## Build the web frontend bundle into web/dist
	trunk build --config web/Trunk.toml --release

web-serve: web-setup ## Serve the web frontend locally with Trunk
	trunk serve --config web/Trunk.toml
