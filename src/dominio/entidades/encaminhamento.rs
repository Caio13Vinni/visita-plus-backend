// =============================================================================
// Entidade Encaminhamento — encaminhamento definido durante a visita (UC06/RF17)
// Mapeada para a tabela `encaminhamento` do MER.
// =============================================================================

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::dominio::enums::StatusEncaminhamento;

/// Encaminhamento definido pelo tecnico durante o atendimento (RF17).
/// Relacionado a uma visita; um ou mais por atendimento.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Encaminhamento {
    pub id: Uuid,
    pub id_visita: Uuid,
    /// Destino do encaminhamento (ex.: CRAS, CREAS, UBS) — obrigatorio.
    pub destino: String,
    /// Motivo do encaminhamento — obrigatorio.
    pub motivo: String,
    pub data_encaminhamento: DateTime<Utc>,
    pub status: StatusEncaminhamento,
    /// Tecnico que registrou o encaminhamento.
    pub registrado_por: Uuid,
    pub criado_em: DateTime<Utc>,
}
