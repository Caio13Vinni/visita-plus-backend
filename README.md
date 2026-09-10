# VISITA+

Sistema de gestão de visitas domiciliares para unidades CRAS/CREAS, desenvolvido em **Rust** com **Clean Architecture**. Projeto acadêmico baseado no documento `Documentacao_Academica_Visitas_Domiciliares.docx` e no Contexto do Projeto (`PROJECT_CONTEXT.md`).

## Arquitetura

```
src/
├── dominio/        # Entidades, enums (estados controlados) e regras de negócio (RN01-RN25)
├── casos_de_uso/   # Orquestração dos fluxos de aplicação (UC01-UC12)
├── infraestrutura/ # Adaptadores técnicos: banco (PostgreSQL + SQLite), segurança, repositórios
└── interface/      # API HTTP (axum), DTOs, estado compartilhado
```

## Banco de dados

- **PostgreSQL** — banco central online com o esquema completo do MER (12 tabelas).
- **SQLite** — simulação do banco local mobile para o mecanismo de sincronização offline.
- Migrações SQL em `migrations/` (aplicadas automaticamente na subida da aplicação).

Tabelas do MER: `unidade`, `usuario`, `sessao`, `recuperacao_acesso`, `familia`, `solicitacao_retorno`, `visita`, `registro_visita`, `complemento_registro`, `encaminhamento`, `notificacao`, `auditoria`.

## Pré-requisitos

- Rust (estável atual) — para desenvolvimento local
- Docker + Docker Compose — para os bancos e a API conteinerizada
- GitHub CLI (`gh`) autenticado — para as rotinas de PR

## Como executar

### Subir os serviços (PostgreSQL + SQLite + API)

```bash
cp .env.example .env   # ajuste as credenciais se necessário
docker compose up -d --build
```

### Desenvolvimento local (sem conteinerizar a API)

```bash
docker compose up -d postgres sqlite
cargo run
```

### Testes

```bash
docker compose up -d postgres sqlite
DATABASE_URL=postgres://visita:visita_forte_2026@localhost:5432/visita_plus cargo test
```

## Fluxo de trabalho Git

- `main` — protegida; apenas o desenvolvedor humano promove código a partir de `develop`.
- `develop` — integração; todos os PRs de `feature/*` apontam para cá.
- `feature/*` — desenvolvimento de cada caso de uso / infraestrutura.

Nenhum PR é aberto para `develop` sem que `cargo test` passe com sucesso contra o banco.