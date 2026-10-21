-- =============================================================================
-- 0004 — Índices de sessão (RF04)
-- -----------------------------------------------------------------------------
-- Suporte de índice às consultas de sessões por usuário (encerramento em
-- lote no logout e na redefinição de senha por recuperação, RF03/RF04).
-- =============================================================================

CREATE INDEX idx_sessao_usuario ON sessao (id_usuario);
