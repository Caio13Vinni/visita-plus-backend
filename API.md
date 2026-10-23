# VISITA+ — Contrato da API REST

Contrato completo dos endpoints para integração do front-end (Dart/Flutter).
Backend: Rust + axum 0.8. Todas as rotas falam JSON (`Content-Type: application/json`).

## 1. Visão geral

- **Base URL:** `http://<host>:<porta>/api/v1` — porta via env `API_PORTA` (default **8080**), host via `API_HOST` (default `0.0.0.0`).
- **Saúde (pública):** `GET /saude` → 200 `{"status":"ok","servico":"visita_plus","schema_mer":"aplicado","ultima_migracao":<n>}` ou `{"status":"degradado",...}`.
- **CORS:** permissivo (qualquer origem) — não é necessário para app Flutter, mas facilita testes via web.
- **Erro padrão:** todo erro de domínio vira `{"mensagem": "<texto>"}` com o status HTTP correspondente.
- **Paths sem barra final:** `GET /api/v1/visitas` existe; `/api/v1/visitas/` dá 404.

### Formatos de dados no wire

| Tipo Rust | Formato JSON | Exemplo |
|---|---|---|
| `Uuid` | string | `"550e8400-e29b-41d4-a716-446655440000"` |
| `NaiveDate` | `"AAAA-MM-DD"` | `"2026-10-25"` |
| `NaiveTime` | `"HH:MM:SS"` | `"09:30:00"` |
| `DateTime<Utc>` | RFC 3339 | `"2026-10-22T14:03:00Z"` |
| `Option<T>` | `T` ou `null` | — |

## 2. Autenticação

**Header em todas as rotas protegidas:** `Authorization: Bearer <jwt>`

- **JWT (HS256)** com claims: `sub` (id do usuário), `sid` (id da sessão), `email`, `perfil`, `iat`, `exp` (epoch segundos). Expiração default: **24 h** (`JWT_EXPIRACAO_MINUTOS`), com **renovação deslizante**: se restar menos da metade da janela, a expiração é prorrogada a cada chamada autenticada.
- **Sessão server-side (RF04):** o token só vale se a sessão `sid` estiver ativa no servidor. **Logout encerra a sessão imediatamente** — o token morre na hora em todas as rotas. O app Flutter deve descartar o token local no logout e tratar 401 como "sair para a tela de login".
- **Rejeições do extrator** (todas 401 + `{"mensagem"}`): `cabecalho de autorizacao ausente`, `token de autorizacao invalido`, `token invalido ou expirado`, `sessao invalida, expirada ou encerrada`.

**Endpoints públicos (sem Bearer):** `POST /api/v1/auth/login`, `POST /api/v1/auth/recuperar-senha`, `POST /api/v1/auth/recuperar-senha/redefinir`, `GET /saude`.

## 3. Enums — valores exatos no JSON

Serde serializa em PascalCase, com renames nos casos acentuados. **Compare com estes literais:**

| Enum | Valores |
|---|---|
| `PerfilUsuario` | `"Coordenador"`, `"Tecnico"` |
| `StatusUsuario` | `"Ativo"`, `"Inativo"` |
| `StatusFamilia` | `"Ativa"`, `"Inativa"` |
| `PrioridadeVisita` | `"Baixa"`, `"Normal"` (default), `"Alta"` |
| `StatusVisita` | `"Agendada"`, `"Em andamento"`, `"Realizada"`, `"Não realizada"`, `"Cancelada"` |
| `StatusRegistro` | `"Rascunho"`, `"Finalizado"` |
| `ResultadoRegistro` | `"Realizada"`, `"Não realizada"` |
| `StatusSolicitacao` | `"Pendente"`, `"Aprovada"`, `"Rejeitada"`, `"Cancelada"`, `"Agendada"` |
| `TipoNotificacao` | `"Lembrete"`, `"Aviso"`, `"Sistema"` |
| `StatusEncaminhamento` | `"Aberto"`, `"Concluído"` |

Atenção: `"Em andamento"`, `"Não realizada"` e `"Concluído"` têm **espaço e acento**.

## 4. Autenticação — endpoints

Prefixo: `/api/v1/auth`

| Método | Path | Auth | Body request | Response |
|---|---|---|---|---|
| POST | `/auth/login` | não | `{email, senha}` | 200 `{token, usuario: RespostaUsuario, exige_troca_senha}` |
| POST | `/auth/trocar-senha` | sim | `{senha_atual, nova_senha}` | 204 sem corpo |
| POST | `/auth/recuperar-senha` | não | `{email}` | 200 `{mensagem}` (genérica — não revela se a conta existe; código vai pro log do servidor) |
| POST | `/auth/recuperar-senha/redefinir` | não | `{email, codigo, nova_senha}` | 200 sem corpo |
| POST | `/auth/logout` | sim | — | 204 sem corpo |

Erros: `CredenciaisInvalidas`, `EmailInvalido`, `SenhaFraca` → 400; `ContaInativa` → 403; `NaoEncontrado` → 404; sessão/token → 401.

**RespostaUsuario:** `{id, nome, email, perfil, status}`

## 5. Profissionais (UC02 — Coordenador)

Prefixo: `/api/v1/profissionais`

| Método | Path | Query | Body | Response |
|---|---|---|---|---|
| GET | `/profissionais` | `id_unidade?` | — | 200 `RespostaUsuario[]` (sempre da própria unidade) |
| POST | `/profissionais` | — | `{id_unidade, nome, email, senha, perfil}` | 201 `RespostaUsuario` |
| PATCH | `/profissionais/{id}` | — | `{nome, perfil}` | 200 `RespostaUsuario` |
| PATCH | `/profissionais/{id}/status` | — | `{status}` | 200 `RespostaUsuario` |

## 6. Famílias (UC03)

Prefixo: `/api/v1/familias`

| Método | Path | Query | Body | Response |
|---|---|---|---|---|
| GET | `/familias` | `id_unidade?, status?` | — | 200 `RespostaFamilia[]` — coordenador: da própria unidade; técnico: só famílias com visita atribuída a ele (RN05/RN06) |
| POST | `/familias` | **`id_unidade` obrigatório** | `{codigo, nome_referencia, contato, endereco}` | 201 `RespostaFamilia` |
| PATCH | `/familias/{id}` | — | mesmo body do POST | 200 `RespostaFamilia` |
| PATCH | `/familias/{id}/status` | — | `{status}` | 200 `RespostaFamilia` |

**RespostaFamilia:** `{id, id_unidade, codigo, nome_referencia, contato, endereco, status}`

## 7. Visitas (UC04)

Prefixo: `/api/v1/visitas`

| Método | Path | Query | Body | Response |
|---|---|---|---|---|
| GET | `/visitas` | `id_tecnico?, id_familia?, id_unidade?, status?, data_inicio?, data_fim?` | — | 200 `RespostaVisita[]` — técnico vê só a própria agenda; coordenador, a da própria unidade |
| POST | `/visitas` | — | `{id_familia, id_tecnico, data_visita, horario, prioridade, objetivo}` | 201 `RespostaVisita` |
| GET | `/visitas/{id}` | — | — | 200 `RespostaVisita` **com `familia` preenchido** (RF12) |
| PATCH | `/visitas/{id}` | — | `{data_visita, horario, id_tecnico?}` | 200 `RespostaVisita` (reagendar) |
| PATCH | `/visitas/{id}/cancelar` | — | `{motivo}` | 200 `RespostaVisita` |

**RespostaVisita:** `{id, id_familia, id_tecnico, id_agendado_por, data_visita, horario, prioridade, objetivo, status, motivo_cancelamento, familia}`

- `familia: RespostaFamilia | null` — **só preenchido no `GET /visitas/{id}`**; nas listas/agendamento/cancelamento vem `null` (evita consulta em cascata).

Erros: `SemPermissao` → 403; `NaoEncontrado` → 404; `FamiliaInativa`, `ConflitoDeHorario`, `VisitaNaoPodeSerAlterada` → 409; `TecnicoInvalido`, `DataRetroativa`, `MotivoObrigatorio` → 400.

## 8. Registros da visita (UC05)

Prefixo: `/api/v1/registros`

| Método | Path | Body | Response |
|---|---|---|---|
| POST | `/registros` | `{id_visita}` | 201 `RespostaRegistro` (cria rascunho com `status: "Rascunho"`; a visita passa para `"Em andamento"`) |
| GET | `/registros/visita/{id_visita}` | — | 200 `RespostaRegistro` |
| PATCH | `/registros/{id}` | `{resultado?, descricao?, desfecho?, necessita_retorno?, motivo_nao_realizada?}` | 200 `RespostaRegistro` (salva rascunho) |
| POST | `/registros/{id}/finalizar` | mesmo body do PATCH | 200 `RespostaRegistro` (fecha e torna imutável) |
| POST | `/registros/{id}/complementos` | `{descricao, motivo}` | 201 `RespostaComplemento` |
| GET | `/registros/{id}/complementos` | — | 200 `RespostaComplemento[]` |

**RespostaRegistro:** `{id, id_visita, status, resultado, descricao, desfecho, necessita_retorno, motivo_nao_realizada, data_hora_inicio, data_hora_fim, registrado_por}`

**RespostaComplemento:** `{id, id_registro, descricao, motivo, registrado_por}`

## 9. Solicitações de retorno (UC08)

Prefixo: `/api/v1/solicitacoes-retorno`

| Método | Path | Query | Body | Response |
|---|---|---|---|---|
| GET | `/solicitacoes-retorno` | `status?` | — | 200 `RespostaSolicitacao[]` |
| POST | `/solicitacoes-retorno` | — | `{id_visita, motivo, prioridade}` | 201 `RespostaSolicitacao` (técnico) |
| PATCH | `/solicitacoes-retorno/{id}/aprovar` | — | `{id_tecnico, data_visita, horario}` | 200 `{solicitacao, visita}` — aprova **e agenda** a visita de retorno |
| PATCH | `/solicitacoes-retorno/{id}/rejeitar` | — | `{justificativa}` | 200 `RespostaSolicitacao` |
| PATCH | `/solicitacoes-retorno/{id}/cancelar` | — | — | 200 `RespostaSolicitacao` (só o solicitante, enquanto pendente) |

**RespostaSolicitacao:** `{id, id_visita, motivo, prioridade, solicitado_por, data_solicitacao, status, decidido_por?, decisao_motivo?, decidido_em?}`

## 10. Notificações (UC10)

Prefixo: `/api/v1/notificacoes`

| Método | Path | Query | Response |
|---|---|---|---|
| GET | `/notificacoes` | `somente_nao_lidas?` (default false) | 200 `RespostaNotificacao[]` (mais recente primeiro) |
| PATCH | `/notificacoes/{id}/abrir` | — | 200 `RespostaNotificacao` (marca como lida) |

**RespostaNotificacao:** `{id, titulo, mensagem, tipo, lida, lida_em?, data_hora}` — título/mensagem sempre genéricos (RN21), sem dados de família.

## 11. Indicadores (UC11 — Coordenador)

Prefixo: `/api/v1/indicadores`

| Método | Path | Query | Response |
|---|---|---|---|
| GET | `/indicadores` | `data_inicio?, data_fim?` (sem params: últimos 30 dias) | 200 `RespostaIndicadores` |

**RespostaIndicadores:** `{periodo_inicio, periodo_fim, visitas_agendadas, visitas_em_andamento, visitas_realizadas, visitas_nao_realizadas, visitas_canceladas, total_visitas, retornos_pendentes, periodo_sem_dados}`

## 12. Auditoria (UC12 — Coordenador)

Prefixo: `/api/v1/auditoria`

| Método | Path | Query | Response |
|---|---|---|---|
| GET | `/auditoria` | `id_usuario?, acao?, entidade?, entidade_id?, data_inicio?, data_fim?` (datas RFC 3339) | 200 `RespostaEventoAuditoria[]` (mais recente primeiro) |

**RespostaEventoAuditoria:** `{id, id_usuario?, entidade, entidade_id, acao, descricao?, data_hora}`

Ações emitidas pelo backend (filtre `acao` por estes valores exatos):

- **Auth/sessão:** `LOGIN_SUCESSO`, `LOGIN_FALHA`, `LOGIN_BLOQUEADO`, `TROCA_SENHA`, `SESSAO_ENCERRADA`
- **Recuperação de senha:** `RECUPERACAO_SOLICITADA`, `RECUPERACAO_CONCLUIDA`, `RECUPERACAO_FALHA`, `RECUPERACAO_NEGADA`
- **Cadastros:** `PROFISSIONAL_CRIADO`, `PROFISSIONAL_ALTERADO`, `FAMILIA_CRIADA`, `FAMILIA_ALTERADA`
- **Visitas/registros:** `VISITA_AGENDADA`, `VISITA_REAGENDADA`, `VISITA_CANCELADA`, `REGISTRO_INICIADO`, `REGISTRO_FINALIZADO`, `COMPLEMENTO_REGISTRADO`
- **Retornos:** `RETORNO_SOLICITADO`, `RETORNO_AGENDADO`, `RETORNO_REJEITADO`, `RETORNO_CANCELADO`
- **Outros:** `ENCAMINHAMENTO_REGISTRADO`, `ENCAMINHAMENTO_CONCLUIDO`, `SINCRONIZACAO_REALIZADA`
- **Acessos negados:** `NOTIFICACAO_ACESSO_NEGADO`, `AUDITORIA_ACESSO_NEGADO`, `HISTORICO_ACESSO_NEGADO`

## 13. Histórico (UC09)

Prefixo: `/api/v1/historico`

| Método | Path | Query | Response |
|---|---|---|---|
| GET | `/historico` | `id_familia?, status?, data_inicio?, data_fim?` | 200 `RespostaVisita[]` (mais recente primeiro; `familia: null`) |
| GET | `/historico/{id}` | — | 200 `RespostaDetalheHistorico` |

**RespostaDetalheHistorico:** `{visita, registro?, complementos, mudancas}` — `mudancas: {id, id_usuario?, acao, descricao?, data_hora}[]`

## 14. Encaminhamentos (RF17)

Prefixo: `/api/v1/encaminhamentos`

| Método | Path | Query | Body | Response |
|---|---|---|---|---|
| GET | `/encaminhamentos` | **`id_visita` obrigatório** | — | 200 `RespostaEncaminhamento[]` |
| POST | `/encaminhamentos` | — | `{id_visita, destino, motivo}` | 201 `RespostaEncaminhamento` |
| PATCH | `/encaminhamentos/{id}/concluir` | — | — | 200 `RespostaEncaminhamento` |

**RespostaEncaminhamento:** `{id, id_visita, destino, motivo, data_encaminhamento, status, registrado_por}`

## 15. Sincronização offline/online

Prefixo: `/api/v1/sincronizacao`

| Método | Path | Auth | Body | Response |
|---|---|---|---|---|
| POST | `/sincronizacao` | sim | nenhum | 200 `{enviados, recebidos, erros}` — reconcilia a fila do SQLite server-side (protótipo) |
| POST | `/sincronizacao/push` | sim | `{pendencias: [{tipo, payload}]}` | 200 `{enviados, erros, itens: [{tipo, sucesso, erro?}]}` |
| GET | `/sincronizacao/pull` | sim | — | 200 `{visitas: RespostaVisita[], familias: RespostaFamilia[]}` |

**Fluxo do app Flutter:** trabalhe offline no SQLite local → ao voltar à rede, `POST /push` com as pendências acumuladas → `GET /pull` para atualizar a agenda local. As duas operações são independentes: dá pra chamar só push, só pull, ou os dois.

**Push** — tipos de pendência e payloads:

- `"registro_visita"` → `{id_visita, resultado, descricao?, desfecho?, necessita_retorno, motivo_nao_realizada?}`
- `"encaminhamento"` → `{id_visita, destino, motivo}`

Regras aplicadas no push:

1. A visita precisa existir e pertencer ao técnico autenticado, senão o item retorna `sucesso: false` com `erro` — **itens inválidos não abortam o lote** (`enviados`/`erros` refletem o resultado por item em `itens`).
2. Registro de visita é **idempotente** — reenvio não duplica (cardinalidade 1:0..1 do MER).
3. Se a visita ainda está `"Agendada"`, o push abre `"Em andamento"` e depois fecha com o resultado (`"Realizada"`/`"Não realizada"`), como no fluxo online.
4. O push é auditado (`SINCRONIZACAO_REALIZADA`, RN22).

**Pull** — devolve a agenda do perfil (RN05/RN06): técnico recebe as **suas** visitas; coordenador, as da **sua unidade**; com as famílias correspondentes (sem duplicidade). `RespostaVisita.familia` vem `null` no pull — os dados da família vêm no array `familias`.

## 16. Cheat sheet

- **201 Created:** POST de profissionais, famílias, visitas, registros (iniciar), complementos, solicitações, encaminhamentos.
- **204:** `trocar-senha`, `logout`.
- **Técnico vs Coordenador:** técnico só opera na própria agenda e nas famílias das próprias visitas; coordenador, na própria unidade. Violações → 403 (ou 404 no detalhar visita, para não revelar existência).
- **Tratar 401** em qualquer rota: token/sessão morreu → voltar ao login.
