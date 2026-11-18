<div align="center">

<h1>VISITA+</h1>

**Sistema de gestão de visitas domiciliares para unidades CRAS/CREAS.**

Backend em **Rust** com **Clean Architecture**, suporte a **operação offline** com sincronização posterior e **auditoria completa** de ações.

**[API](API.md)** · [Documentação Acadêmica](Documentacao_Academica_Visitas_Domiciliares.docx) · [Contexto do Projeto](PROJECT_CONTEXT.md) · **Português (Brasil)**

[](https://github.com/Caio13Vinni/visita-plus-backend/actions)[](LICENSE)[](https://github.com/Caio13Vinni/visita-plus-backend/issues)[](https://github.com/Caio13Vinni/visita-plus-backend/pulls)

</div>

---

## Por quê

Visitas domiciliares são o coração do trabalho das equipes técnicas do **SUAS** (Sistema Único de Assistência Social). Cada família acompanhada gera um volume enorme de registros: fichas de visita, encaminhamentos, solicitações de retorno, complementos de cadastro.

Hoje, boa parte desse processo ainda é feita em papel ou em planilhas improvisadas. O resultado é previsível:

- **Registros perdidos ou incompletos** — papel danifica, planilha se corrompe
- **Nenhuma rastreabilidade** — quem alterou o quê, quando e por quê?
- **Regras de negócio na cabeça do técnico** — cada unidade faz do seu jeito
- **Impossibilidade de trabalho offline** — campo muitas vezes não tem sinal

O VISITA+ resolve isso com um backend que **codifica as regras de negócio** (RN01-RN25) diretamente no domínio, garante **auditoria de toda ação** e suporta **sincronização offline** para trabalho em campo.

## Características

- **Clean Architecture estrita.** Domínio puro, sem dependências de framework ou banco. Casos de uso orquestram fluxos. Infraestrutura implementa portas.
- **Duplo banco de dados.** PostgreSQL como banco central online; SQLite simulando o banco local mobile para o mecanismo de sincronização offline.
- **12 tabelas no MER.** Modelagem completa: unidades, usuários, sessões, famílias, visitas, registros, encaminhamentos, notificações, auditoria.
- **25 regras de negócio** (RN01-RN25) validadas no domínio — não no controller, não no banco.
- **12 casos de uso** (UC01-UC12) cobrindo autenticação, gestão de famílias, visitas, encaminhamentos, notificações e sincronização.
- **Migrações automáticas.** SQL em `migrations/` aplicadas na subida da aplicação.
- **API REST documentada.** Contrato completo em [`API.md`](API.md): endpoints, payloads, enums e códigos de erro.
- **Auditoria nativa.** Toda ação sensível gera registro na tabela `auditoria`.

## Arquitetura

```
┌──────────── interface (axum) ────────────┐
│  HTTP handlers · DTOs · Estado compartilhado │
└────────────────┬───────────────────────────┘
                 │
┌──────────── casos_de_uso ─────────────────┐
│  UC01-UC12 · Orquestração de fluxos        │
│  entrada/ · saida/ · executores/           │
└────────────────┬───────────────────────────┘
                 │
┌──────────── dominio ──────────────────────┐
│  Entidades · Enums · RN01-RN25             │
│  Sem dependências externas                 │
└────────────────────────────────────────────┘
                 ▲
┌──────────── infraestrutura ───────────────┐
│  PostgreSQL · SQLite · Repositórios        │
│  Segurança · Adaptadores técnicos          │
└────────────────────────────────────────────┘
```

A regra é simples: **o domínio não conhece axum, sqlx ou tokio**. Infraestrutura implementa as abstrações que os casos de uso definem. Isso permite testar regras de negócio sem banco, sem HTTP, sem async.

```
src/
├── dominio/           # Entidades, enums (estados controlados) e regras de negócio
├── casos_de_uso/      # Orquestração dos fluxos de aplicação (UC01-UC12)
├── infraestrutura/    # Adaptadores: banco (PostgreSQL + SQLite), segurança, repositórios
└── interface/         # API HTTP (axum), DTOs, estado compartilhado
```

## Banco de dados

| Banco | Papel | Quando usar |
|---|---|---|
| **PostgreSQL** | Banco central online | Sempre que há conectividade — esquema completo do MER |
| **SQLite** | Simulação do banco local mobile | Operação offline em campo, sincronização posterior |

**Tabelas do MER** (12 no total):

```
unidade ──┬── usuario ──┬── sessao
          │             └── recuperacao_acesso
          │
          ├── familia ──┬── solicitacao_retorno
          │             ├── visita ──┬── registro_visita
          │             │            └── complemento_registro
          │             └── encaminhamento
          │
          ├── notificacao
          └── auditoria
```

Migrações em `migrations/` são aplicadas automaticamente na subida.

## Começando

**Pré-requisitos:** Rust estável, Docker + Docker Compose, GitHub CLI (`gh`) autenticado para rotinas de PR.

```bash
git clone https://github.com/Caio13Vinni/visita-plus-backend.git
cd visita-plus-backend

cp .env.example .env   # ajuste as credenciais se necessário
docker compose up -d --build
```

Para desenvolvimento local sem conteinerizar a API:

```bash
docker compose up -d postgres sqlite
cargo run
```

### Testes

```bash
docker compose up -d postgres sqlite
DATABASE_URL=postgres://visita:visita_forte_2026@localhost:5432/visita_plus cargo test
```

Nenhum PR é aberto para `develop` sem que `cargo test` passe com sucesso contra o banco.

## API

O contrato completo da API — endpoints, payloads de request/response, enums e códigos de erro — está em [`API.md`](API.md).

Exemplo de resposta padronizada:

```json
{
  "sucesso": true,
  "dados": { ... },
  "erro": null
}
```

## Fluxo de trabalho Git

```
main (protegida) ◄── develop (integração) ◄── feature/* (desenvolvimento)
```

| Branch | Papel | Regra |
|---|---|---|
| `main` | Produção | Apenas o desenvolvedor humano promove código a partir de `develop` |
| `develop` | Integração | Todos os PRs de `feature/*` apontam para cá |
| `feature/*` | Desenvolvimento | Um caso de uso ou infraestrutura por branch |

**Checklist para PR:**

- [ ] `cargo test` passando contra o banco
- [ ] Regras de negócio no domínio, não no controller
- [ ] Migrações SQL incluídas se o MER mudou
- [ ] Documentação da API atualizada em `API.md`

## Regras de negócio

As **25 regras** (RN01-RN25) vivem em `src/dominio/`. Exemplos do tipo de validação:

- RN01-RN05: Autenticação e sessões
- RN06-RN12: Gestão de famílias e visitas
- RN13-RN18: Encaminhamentos e solicitações de retorno
- RN19-RN22: Sincronização offline
- RN23-RN25: Auditoria e notificações

Cada regra é testável isoladamente, sem banco, sem HTTP.

## Roadmap

Direção, não promessa. A ordem pode mudar conforme o projeto evolui.

| Ideia | Estágio | O que significa |
|---|---|---|
| **Frontend mobile** | planejado | App para trabalho offline em campo, sincronizando com o backend |
| **Relatórios gerenciais** | planejado | Exportação de dados de visitas por período, unidade, família |
| **Notificações push** | planejado | Alertas de visitas agendadas, solicitações de retorno pendentes |
| **Dashboard web** | explorando | Visão consolidada para gestores de unidade |
| **Integração com CadÚnico** | explorando | Sincronização de dados de famílias com o Cadastro Único |

## Contribuindo

Contribuições são bem-vindas: código, documentação, revisão de regras de negócio.

1. Crie uma `feature/*` a partir de `develop`
2. Implemente o caso de uso ou infraestrutura
3. Garanta que `cargo test` passa
4. Abra PR para `develop`

| | |
|---|---|
| [API](API.md) | contrato completo da API |
| [Documentação Acadêmica](Documentacao_Academica_Visitas_Domiciliares.docx) | base teórica do projeto |
| [Contexto do Projeto](PROJECT_CONTEXT.md) | escopo, regras e casos de uso |

## Licença

[MIT](LICENSE) © Caio Vinícius e contribuidores do VISITA+.
