-- =============================================================================
-- VISITA+ — Migração 0001: Esquema completo do MER no PostgreSQL
-- -----------------------------------------------------------------------------
-- Implementa rigorosamente as 12 tabelas do Modelo Entidade-Relacionamento:
--   unidade, usuario, sessao, recuperacao_acesso, familia, solicitacao_retorno,
--   visita, registro_visita, complemento_registro, encaminhamento, notificacao,
--   auditoria
-- Os estados controlados (documentacao) sao modelados como ENUMs.
-- =============================================================================

-- Habilita a geracao de UUIDs v4 no banco
CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- =============================================================================
-- ENUMS — estados controlados pelo sistema (documentacao academica)
-- =============================================================================
CREATE TYPE tipo_unidade      AS ENUM ('CRAS', 'CREAS');
CREATE TYPE perfil_usuario    AS ENUM ('Coordenador', 'Tecnico');
CREATE TYPE status_usuario    AS ENUM ('Ativo', 'Inativo');
CREATE TYPE status_familia    AS ENUM ('Ativa', 'Inativa');
CREATE TYPE prioridade_visita AS ENUM ('Baixa', 'Normal', 'Alta');
CREATE TYPE status_visita     AS ENUM ('Agendada', 'Em andamento', 'Realizada', 'Não realizada', 'Cancelada');
CREATE TYPE status_registro   AS ENUM ('Rascunho', 'Finalizado');
CREATE TYPE resultado_registro AS ENUM ('Realizada', 'Não realizada');
CREATE TYPE status_solicitacao AS ENUM ('Pendente', 'Aprovada', 'Rejeitada', 'Cancelada', 'Agendada');
CREATE TYPE status_encaminhamento AS ENUM ('Aberto', 'Concluído');
CREATE TYPE tipo_notificacao  AS ENUM ('Lembrete', 'Aviso', 'Sistema');

-- =============================================================================
-- TABELA: unidade (CRAS/CREAS)
-- =============================================================================
CREATE TABLE unidade (
    id_unidade UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    nome       TEXT NOT NULL,
    tipo       tipo_unidade NOT NULL,
    cidade     TEXT NOT NULL,
    criado_em  TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT uq_unidade_nome UNIQUE (nome)
);

-- =============================================================================
-- TABELA: usuario (profissionais: Coordenador e Tecnico de Referencia)
-- =============================================================================
CREATE TABLE usuario (
    id_usuario        UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    id_unidade        UUID NOT NULL REFERENCES unidade(id_unidade),
    nome              TEXT NOT NULL,
    email             TEXT NOT NULL,
    senha_hash        TEXT NOT NULL,
    perfil            perfil_usuario NOT NULL,
    status            status_usuario NOT NULL DEFAULT 'Ativo',
    exige_troca_senha BOOLEAN NOT NULL DEFAULT TRUE,
    ultimo_acesso_em  TIMESTAMPTZ,
    criado_em         TIMESTAMPTZ NOT NULL DEFAULT now(),
    atualizado_em     TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT uq_usuario_email UNIQUE (email),
    CONSTRAINT chk_usuario_email_formato CHECK (
        email ~* '^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}$'
    )
);

-- =============================================================================
-- TABELA: sessao (token JWT / sessoes ativas)
-- =============================================================================
CREATE TABLE sessao (
    id_sessao    UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    id_usuario   UUID NOT NULL REFERENCES usuario(id_usuario) ON DELETE CASCADE,
    token_jwt    TEXT NOT NULL,
    criado_em    TIMESTAMPTZ NOT NULL DEFAULT now(),
    expira_em    TIMESTAMPTZ NOT NULL,
    encerrada_em TIMESTAMPTZ,
    ip_origem    TEXT,
    dispositivo  TEXT,
    CONSTRAINT uq_sessao_token UNIQUE (token_jwt)
);

-- =============================================================================
-- TABELA: recuperacao_acesso (fluxo de recuperacao de senha por codigo)
-- =============================================================================
CREATE TABLE recuperacao_acesso (
    id_recuperacao UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    id_usuario     UUID NOT NULL REFERENCES usuario(id_usuario) ON DELETE CASCADE,
    codigo         TEXT NOT NULL,
    expira_em      TIMESTAMPTZ NOT NULL,
    usado_em       TIMESTAMPTZ,
    criado_em      TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- =============================================================================
-- TABELA: familia (cadastros estritamente ficticios - RN24)
-- =============================================================================
CREATE TABLE familia (
    id_familia     UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    id_unidade     UUID NOT NULL REFERENCES unidade(id_unidade),
    codigo         TEXT NOT NULL,
    nome_referencia TEXT NOT NULL,
    contato        TEXT NOT NULL,
    endereco       TEXT NOT NULL,
    status         status_familia NOT NULL DEFAULT 'Ativa',
    criado_por     UUID REFERENCES usuario(id_usuario),
    criado_em      TIMESTAMPTZ NOT NULL DEFAULT now(),
    atualizado_em  TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT uq_familia_codigo UNIQUE (codigo)
);

-- =============================================================================
-- TABELA: solicitacao_retorno (UC08 - RN19/RN20)
-- =============================================================================
CREATE TABLE solicitacao_retorno (
    id_solicitacao      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    id_visita           UUID NOT NULL,
    motivo              TEXT NOT NULL,
    prioridade          prioridade_visita NOT NULL DEFAULT 'Normal',
    solicitado_por      UUID NOT NULL REFERENCES usuario(id_usuario),
    data_solicitacao    TIMESTAMPTZ NOT NULL DEFAULT now(),
    status              status_solicitacao NOT NULL DEFAULT 'Pendente',
    decidido_por        UUID REFERENCES usuario(id_usuario),
    decisao_motivo      TEXT,
    decidido_em         TIMESTAMPTZ
);

-- =============================================================================
-- TABELA: visita (UC04/UC05 - RN08 a RN11, RN15, RN16, RN18)
-- =============================================================================
CREATE TABLE visita (
    id_visita          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    id_familia         UUID NOT NULL REFERENCES familia(id_familia),
    id_tecnico         UUID NOT NULL REFERENCES usuario(id_usuario),
    id_agendado_por    UUID NOT NULL REFERENCES usuario(id_usuario),
    id_origem_retorno  UUID,
    data_visita        DATE NOT NULL,
    horario            TIME NOT NULL,
    prioridade         prioridade_visita NOT NULL DEFAULT 'Normal',
    objetivo           TEXT NOT NULL,
    status             status_visita NOT NULL DEFAULT 'Agendada',
    motivo_cancelamento TEXT,
    criado_em          TIMESTAMPTZ NOT NULL DEFAULT now(),
    atualizado_em      TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- RN09 (reforco em nivel de banco): data da visita nao pode ser anterior a hoje
    CONSTRAINT chk_visita_data_futura CHECK (data_visita >= CURRENT_DATE - 1),
    -- RN08: visita exige objetivo nao vazio
    CONSTRAINT chk_visita_objetivo CHECK (length(btrim(objetivo)) > 0),
    -- Visitas canceladas exigem motivo (RFC: motivo_cancelamento)
    CONSTRAINT chk_visita_cancelamento CHECK (
        status <> 'Cancelada' OR (motivo_cancelamento IS NOT NULL AND length(btrim(motivo_cancelamento)) > 0)
    )
);

-- Complemento da tabela: origens de retorno (preenchidas apos a tabela existir)
ALTER TABLE visita
    ADD CONSTRAINT fk_visita_origem_retorno
        FOREIGN KEY (id_origem_retorno) REFERENCES solicitacao_retorno(id_solicitacao);

-- =============================================================================
-- TABELA: registro_visita (UC06/UC07 - RN12 a RN17)
--   1 para 0..1 com visita (UNIQUE)
-- =============================================================================
CREATE TABLE registro_visita (
    id_registro           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    id_visita             UUID NOT NULL UNIQUE REFERENCES visita(id_visita) ON DELETE CASCADE,
    status                status_registro NOT NULL DEFAULT 'Rascunho',
    resultado             resultado_registro,
    descricao             TEXT,
    desfecho              TEXT,
    necessita_retorno     BOOLEAN NOT NULL DEFAULT FALSE,
    motivo_nao_realizada  TEXT,
    data_hora_inicio      TIMESTAMPTZ,
    data_hora_fim         TIMESTAMPTZ,
    registrado_por        UUID NOT NULL REFERENCES usuario(id_usuario),
    criado_em             TIMESTAMPTZ NOT NULL DEFAULT now(),
    atualizado_em         TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- RN13/RN14: registro finalizado como realizado exige descricao e hora de
    -- fim; finalizado como nao realizado exige motivo obrigatorio. Rascunhos
    -- (RN12) permanecem livres para salvar dados parciais.
    CONSTRAINT chk_registro_realizada CHECK (
        status IS DISTINCT FROM 'Finalizado'
        OR resultado IS DISTINCT FROM 'Realizada'
        OR (descricao IS NOT NULL AND length(btrim(descricao)) > 0 AND data_hora_fim IS NOT NULL)
    ),
    CONSTRAINT chk_registro_nao_realizada CHECK (
        status IS DISTINCT FROM 'Finalizado'
        OR resultado IS DISTINCT FROM 'Não realizada'
        OR (motivo_nao_realizada IS NOT NULL AND length(btrim(motivo_nao_realizada)) > 0)
    )
);

-- =============================================================================
-- TABELA: complemento_registro (RN16/RN17 - correcoes sem reabrir a visita)
-- =============================================================================
CREATE TABLE complemento_registro (
    id_complemento  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    id_registro     UUID NOT NULL REFERENCES registro_visita(id_registro) ON DELETE CASCADE,
    descricao       TEXT NOT NULL,
    motivo          TEXT NOT NULL,
    registrado_por  UUID NOT NULL REFERENCES usuario(id_usuario),
    criado_em       TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- =============================================================================
-- TABELA: encaminhamento (UC06 - RF17)
-- =============================================================================
CREATE TABLE encaminhamento (
    id_encaminhamento  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    id_visita          UUID NOT NULL REFERENCES visita(id_visita) ON DELETE CASCADE,
    destino            TEXT NOT NULL,
    motivo             TEXT NOT NULL,
    data_encaminhamento TIMESTAMPTZ NOT NULL DEFAULT now(),
    status             status_encaminhamento NOT NULL DEFAULT 'Aberto',
    registrado_por     UUID NOT NULL REFERENCES usuario(id_usuario),
    criado_em          TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- =============================================================================
-- TABELA: notificacao (UC10 - RN21: dados sensiveis mascarados)
-- =============================================================================
CREATE TABLE notificacao (
    id_notificacao     UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    id_usuario_destino UUID NOT NULL REFERENCES usuario(id_usuario) ON DELETE CASCADE,
    titulo             TEXT NOT NULL,
    mensagem           TEXT NOT NULL,
    tipo               tipo_notificacao NOT NULL DEFAULT 'Lembrete',
    lida               BOOLEAN NOT NULL DEFAULT FALSE,
    lida_em            TIMESTAMPTZ,
    data_hora          TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- =============================================================================
-- TABELA: auditoria (UC12 - RN22)
-- =============================================================================
CREATE TABLE auditoria (
    id_auditoria  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    id_usuario    UUID REFERENCES usuario(id_usuario) ON DELETE SET NULL,
    entidade      TEXT NOT NULL,
    entidade_id   UUID NOT NULL,
    acao          TEXT NOT NULL,
    descricao     TEXT,
    ip_origem     TEXT,
    dispositivo   TEXT,
    data_hora     TIMESTAMPTZ NOT NULL DEFAULT now()
);