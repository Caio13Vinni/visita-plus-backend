// =============================================================================
// Modelo de leitura do historico (UC09) — agrega visita, registro,
// complementos e mudancas auditadas, conforme RN05/RN06 e RN16 a RN18.
// =============================================================================

use uuid::Uuid;

use crate::dominio::entidades::auditoria::EventoAuditoria;
use crate::dominio::entidades::registro_visita::{ComplementoRegistro, RegistroVisita};
use crate::dominio::entidades::visita::Visita;

/// Detalhe completo de um item do historico: a visita, o registro (quando
/// houver), seus complementos imutaveis e as mudancas auditadas — cada uma
/// com autor e data (RN16 a RN18, RN22).
#[derive(Debug, Clone)]
pub struct DetalheHistorico {
    pub visita: Visita,
    /// Registro da visita (relacao 1 para 0..1; `None` se ainda nao iniciado).
    pub registro: Option<RegistroVisita>,
    /// Complementos do registro finalizado, com autor e data (RN16/RN17).
    pub complementos: Vec<ComplementoRegistro>,
    /// Mudancas criticas da visita preservadas na auditoria (RN18/RN22).
    pub mudancas: Vec<EventoAuditoria>,
}

impl DetalheHistorico {
    /// Identificador da visita a que o item se refere.
    pub fn id_visita(&self) -> Uuid {
        self.visita.id
    }
}
