# Common tasks. Run `make` or `make help` to list them.

FRONTEND := frontend
NPM := npm --prefix $(FRONTEND)

.DEFAULT_GOAL := help
.PHONY: help install dev api web ui run app desktop test typecheck check clean

help: ## List available targets
	@awk 'BEGIN {FS = ":.*## "} /^[a-zA-Z_-]+:.*## / {printf "  \033[36m%-10s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)

install: ## Install the frontend and Tauri CLI dependencies (Rust crates download on first build)
	$(NPM) install
	npm install

dev: ## Run the API and UI dev servers with hot reload (one Ctrl+C stops both)
	$(NPM) run dev:all

api: ## Run only the API server (:8765)
	cargo run -p questodian-cli -- serve --no-browser

web: ## Run only the Vite dev server (:5173)
	$(NPM) run dev

ui: ## Build the frontend into frontend/dist
	$(NPM) run build

run: ui ## Build the UI and serve the app in your browser (http://localhost:8765)
	cargo run -p questodian-cli -- serve

app: ui ## Build the UI and open the desktop app (debug build)
	cargo run -p questodian-app

desktop: ## Build the desktop app and its installers for this OS into target/release/bundle/
	npx tauri build

test: ## Rust: format check, lint and tests
	cargo fmt --all --check
	cargo clippy -p questodian-core -p questodian-cli --all-targets -- -D warnings
	cargo test -p questodian-core -p questodian-cli

typecheck: ## Typecheck and test the frontend
	$(NPM) run typecheck
	$(NPM) test

check: test typecheck ## Run everything CI runs

clean: ## Remove build output (keeps your database)
	rm -rf target $(FRONTEND)/dist $(FRONTEND)/*.tsbuildinfo
