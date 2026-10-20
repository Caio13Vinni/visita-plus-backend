// =============================================================================
// Entidade EventoAuditoria — trilha de acoes criticas (UC12)
// Mapeada para a tabela `auditoria` do MER.
// =============================================================================

use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Evento registrado na trilha de auditoria (RN22).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventoAuditoria {
    pub id: Uuid,
    /// Usuario que realizou a acao; ausente em eventos anonimos (ex.: login).
    pub id_usuario: Option<Uuid>,
    /// Entidade afetada (ex.: 'visita', 'registro_visita').
    pub entidade: String,
    pub entidade_id: Uuid,
    /// Acao padronizada (ex.: 'VISITA_REAGENDADA').
    pub acao: String,
    pub descricao: Option<String>,
    pub data_hora: DateTime<Utc>,
}
/// Filtros da consulta de auditoria (UC12): periodo, usuario, acao ou item.
#[derive(Debug, Clone, Default)]
pub struct FiltroAuditoria {
    /// Filtra pelo autor do evento.
    pub id_usuario: Option<Uuid>,
    /// Filtra pela acao padronizada (ex.: 'VISITA_CANCELADA').
    pub acao: Option<String>,
    /// Filtra pela entidade afetada (ex.: 'visita').
    pub entidade: Option<String>,
    /// Filtra pelo item exato (ex.: id da visita).
    pub entidade_id: Option<Uuid>,
    /// Inicio do periodo (inclusive).
    pub data_inicio: Option<DateTime<Utc>>,
    /// Fim do periodo (inclusive).
    pub data_fim: Option<DateTime<Utc>>,
}
