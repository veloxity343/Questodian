# Common tasks. Run `make` or `make help` to list them.

FRONTEND := frontend
NPM := npm --prefix $(FRONTEND)

.DEFAULT_GOAL := help
.PHONY: help install dev api web run app build ui package desktop test typecheck check migration clean

help: ## List available targets
	@awk 'BEGIN {FS = ":.*## "} /^[a-zA-Z_-]+:.*## / {printf "  \033[36m%-10s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)

install: ## Install Python and frontend dependencies
	uv sync
	$(NPM) install

dev: ## Run API + UI dev servers with hot reload (one Ctrl+C stops both)
	$(NPM) run dev:all

api: ## Run only the API dev server (:8765)
	uv run questodian serve --no-browser

web: ## Run only the Vite dev server (:5173)
	$(NPM) run dev

run: ui ## Build the UI and start the app as users run it
	uv run questodian

app: ui ## Build the UI and open Questodian in its own window
	uv run --extra desktop questodian app

ui: ## Build the frontend into src/questodian/web
	$(NPM) run build

package: ui ## Build the release wheel and sdist into dist/
	uv build
	@unzip -l dist/*.whl | grep -q 'questodian/web/index.html' || { echo "error: wheel is missing the UI"; exit 1; }

build: package ## Alias for package

desktop: ui ## Build the desktop app for this OS into dist-desktop/ (PyInstaller)
	uv sync --group build --extra desktop
	cd packaging && uv run pyinstaller --noconfirm --distpath ../dist-desktop --workpath ../build-desktop questodian.spec

test: ## Run backend tests
	uv run pytest

typecheck: ## Typecheck the frontend
	$(NPM) run typecheck

check: test typecheck ## Run everything CI runs except packaging (tests also catch models/migrations drift)

migration: ## Create a migration from model changes: make migration m="add portraits"
	@test -n "$(m)" || { echo 'usage: make migration m="describe the change"'; exit 1; }
	uv run alembic revision --autogenerate -m "$(m)"

clean: ## Remove build output (keeps your database)
	rm -rf dist dist-desktop build-desktop src/questodian/web $(FRONTEND)/*.tsbuildinfo
