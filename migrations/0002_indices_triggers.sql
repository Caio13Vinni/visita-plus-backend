-- =============================================================================
-- VISITA+ — Migração 0002: Índices, triggers e reforços de integridade
-- =============================================================================

-- =============================================================================
-- FUNÇÃO: atualizar_timestamp (usada pelas triggers de atualizado_em)
-- =============================================================================
CREATE OR REPLACE FUNCTION atualizar_timestamp()
RETURNS TRIGGER AS $$
BEGIN
    NEW.atualizado_em = now();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- =============================================================================
-- TRIGGERS de atualizado_em
-- =============================================================================
CREATE TRIGGER trg_usuario_atualizado
    BEFORE UPDATE ON usuario
    FOR EACH ROW EXECUTE FUNCTION atualizar_timestamp();

CREATE TRIGGER trg_familia_atualizado
    BEFORE UPDATE ON familia
    FOR EACH ROW EXECUTE FUNCTION atualizar_timestamp();

CREATE TRIGGER trg_visita_atualizado
    BEFORE UPDATE ON visita
    FOR EACH ROW EXECUTE FUNCTION atualizar_timestamp();

CREATE TRIGGER trg_registro_visita_atualizado
    BEFORE UPDATE ON registro_visita
    FOR EACH ROW EXECUTE FUNCTION atualizar_timestamp();

-- =============================================================================
-- ÍNDICES — otimizacao das consultas previstas nos casos de uso
-- =============================================================================

-- usuario
CREATE INDEX idx_usuario_unidade     ON usuario (id_unidade);
CREATE INDEX idx_usuario_perfil      ON usuario (perfil);
CREATE INDEX idx_usuario_status      ON usuario (status);

-- sessao
CREATE INDEX idx_sessao_usuario      ON sessao (id_usuario);
CREATE INDEX idx_sessao_expiracao    ON sessao (expira_em);

-- recuperacao_acesso
CREATE INDEX idx_recuperacao_usuario ON recuperacao_acesso (id_usuario);
CREATE INDEX idx_recuperacao_codigo  ON recuperacao_acesso (codigo);

-- familia
CREATE INDEX idx_familia_unidade     ON familia (id_unidade);
CREATE INDEX idx_familia_status      ON familia (status);

-- solicitacao_retorno
CREATE INDEX idx_solicitacao_visita  ON solicitacao_retorno (id_visita);
CREATE INDEX idx_solicitacao_status  ON solicitacao_retorno (status);
CREATE INDEX idx_solicitacao_solicitado_por ON solicitacao_retorno (solicitado_por);

-- visita (consultas da agenda UC05 e do historico UC09)
CREATE INDEX idx_visita_familia      ON visita (id_familia);
CREATE INDEX idx_visita_tecnico      ON visita (id_tecnico);
CREATE INDEX idx_visita_data         ON visita (data_visita, horario);
CREATE INDEX idx_visita_status       ON visita (status);
CREATE INDEX idx_visita_origem_retorno ON visita (id_origem_retorno);

-- registro_visita
CREATE INDEX idx_registro_visita_resultado ON registro_visita (resultado);
CREATE INDEX idx_registro_visita_status    ON registro_visita (status);

-- complemento_registro
CREATE INDEX idx_complemento_registro ON complemento_registro (id_registro);

-- encaminhamento
CREATE INDEX idx_encaminhamento_visita ON encaminhamento (id_visita);

-- notificacao
CREATE INDEX idx_notificacao_destino ON notificacao (id_usuario_destino);
CREATE INDEX idx_notificacao_lida    ON notificacao (lida) WHERE lida = FALSE;
CREATE INDEX idx_notificacao_data    ON notificacao (data_hora);

-- auditoria
CREATE INDEX idx_auditoria_usuario   ON auditoria (id_usuario);
CREATE INDEX idx_auditoria_entidade  ON auditoria (entidade, entidade_id);
CREATE INDEX idx_auditoria_data      ON auditoria (data_hora DESC);