# =============================================================================
# VISITA+ — Comandos utilitarios de desenvolvimento
# =============================================================================

# Obtem a URL do banco a partir do .env quando existir
-include .env
export DATABASE_URL ?= postgres://visita:visita_forte_2026@localhost:5432/visita_plus

.PHONY: up down build test test-integracao fmt check logs ps reset

## Sobe todos os servicos (postgres, sqlite, api)
up:
	docker compose up -d --build

## Sobe apenas os bancos (desenvolvimento local)
db-up:
	docker compose up -d postgres sqlite

## Deruba os servicos
down:
	docker compose down

## Compila a imagem da API (multi-stage)
build:
	docker compose build api

## Executa a suite completa de testes (exige os bancos de pe)
test: db-up
	cargo test

## Executa os testes de integridade do MER
test-integracao: db-up
	cargo test --test integridade_mer

## Formata o codigo
fmt:
	cargo fmt --all

## Checa possiveis problemas estaticos
check:
	cargo clippy --all-targets --all-features -- -D warnings

## Acompanha os logs da API
logs:
	docker compose logs -f api

## Status dos servicos
ps:
	docker compose ps

## Recria o schema do banco a partir da migracao inicial
reset: down
	docker volume rm -f visita_plus_postgres_dados visita_plus_sqlite_dados
	docker compose up -d postgres sqlite