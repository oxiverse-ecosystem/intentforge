# IntentForge v2 — Docker Compose Makefile
# All commands run from project root (where this file lives)
#
# PROD: Full stack with VPN + Tor + Traefik + SSL
# DEV:  Minimal stack for local testing (no VPN, no Traefik)

COMPOSE_DIR := services
PROD_FILE   := docker-compose.prod.yml
DEV_FILE    := docker-compose.dev.yml

# ─────────────────────────────────────────────────────────────────
# PRODUCTION
# ─────────────────────────────────────────────────────────────────

.PHONY: prod-up prod-down prod-nuke prod-logs prod-ps prod-rebuild

prod-up:                              ## Build + start production stack
	cd $(COMPOSE_DIR) && docker compose -f $(PROD_FILE) up -d --build

prod-down:                            ## Stop production stack
	cd $(COMPOSE_DIR) && docker compose -f $(PROD_FILE) down

prod-nuke:                            ## Nuke prod: containers + volumes + images
	cd $(COMPOSE_DIR) && docker compose -f $(PROD_FILE) down -v --rmi all --remove-orphans

prod-rebuild: prod-nuke prod-up       ## Full clean rebuild (nuke + build)

prod-logs:                            ## Tail production logs
	cd $(COMPOSE_DIR) && docker compose -f $(PROD_FILE) logs -f

prod-ps:                              ## List production containers
	cd $(COMPOSE_DIR) && docker compose -f $(PROD_FILE) ps

# ─────────────────────────────────────────────────────────────────
# DEVELOPMENT
# ─────────────────────────────────────────────────────────────────

.PHONY: dev-up dev-down dev-nuke dev-logs dev-ps dev-rebuild dev-shell

dev-up:                               ## Build + start dev stack
	cd $(COMPOSE_DIR) && docker compose -f $(DEV_FILE) up -d --build

dev-down:                             ## Stop dev stack
	cd $(COMPOSE_DIR) && docker compose -f $(DEV_FILE) down

dev-nuke:                             ## Nuke dev: containers + volumes + images
	cd $(COMPOSE_DIR) && docker compose -f $(DEV_FILE) down -v --rmi all --remove-orphans

dev-rebuild: dev-nuke dev-up          ## Full clean rebuild (nuke + build)

dev-logs:                             ## Tail dev logs
	cd $(COMPOSE_DIR) && docker compose -f $(DEV_FILE) logs -f

dev-ps:                               ## List dev containers
	cd $(COMPOSE_DIR) && docker compose -f $(DEV_FILE) ps

dev-shell SVC=gateway:                ## Shell into a dev service (make dev-shell SVC=gateway)
	docker exec -it if-dev-$(SVC) sh

# ─────────────────────────────────────────────────────────────────
# NUCLEAR OPTION — wipe ALL intentforge containers/images/volumes
# ─────────────────────────────────────────────────────────────────

.PHONY: nuke-all

nuke-all:                             ## Nuke EVERYTHING (prod + dev + dangling)
	cd $(COMPOSE_DIR) && docker compose -f $(PROD_FILE) down -v --rmi all --remove-orphans 2>/dev/null || true
	cd $(COMPOSE_DIR) && docker compose -f $(DEV_FILE)  down -v --rmi all --remove-orphans 2>/dev/null || true
	docker image prune -f
	docker volume prune -f

# ─────────────────────────────────────────────────────────────────
# HELP
# ─────────────────────────────────────────────────────────────────

.PHONY: help

help:                                 ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-18s\033[0m %s\n", $$1, $$2}'

.DEFAULT_GOAL := help
