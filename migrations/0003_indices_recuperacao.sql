-- =============================================================================
-- 0003 — Índices da recuperação de acesso (RF03)
-- -----------------------------------------------------------------------------
-- Adiciona suporte de índice às consultas de código de recuperação por
-- usuário (fluxo RF03, UC01). Migração separada para não alterar o
-- checksum da 0002 já aplicada nos ambientes existentes.
-- =============================================================================

CREATE INDEX idx_recuperacao_usuario ON recuperacao_acesso (id_usuario);
