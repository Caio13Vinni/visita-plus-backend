// =============================================================================
// Entidade SolicitacaoRetorno — pedido de retorno a familia (UC08)
// Mapeada para a tabela `solicitacao_retorno` do MER.
// =============================================================================

use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use uuid::Uuid;

use crate::dominio::enums::{PrioridadeVisita, StatusSolicitacao};

/// Pedido de retorno a familia, criado pelo tecnico e decidido pelo
/// coordenador (RN19). Ao aprovar, o sistema agenda a nova visita e o pedido
/// passa para 'Agendada' (RN19/RN20).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SolicitacaoRetorno {
    pub id: Uuid,
    /// Visita original que gerou a demanda de retorno.
    pub id_visita: Uuid,
    /// Motivo do retorno informado pelo tecnico solicitante (RN19).
    pub motivo: String,
    pub prioridade: PrioridadeVisita,
    pub solicitado_por: Uuid,
    pub data_solicitacao: DateTime<Utc>,
    pub status: StatusSolicitacao,
    /// Coordenador que decidiu o pedido (aprovacao ou rejeicao).
    pub decidido_por: Option<Uuid>,
    /// Justificativa obrigatoria em caso de rejeicao (RN19).
    pub decisao_motivo: Option<String>,
    pub decidido_em: Option<DateTime<Utc>>,
}

/// Dados de entrada para aprovar e programar o retorno (RN19).
/// Somente o coordenador decide, definindo tecnico, data e horario da nova
/// visita (a prioridade e a familia sao herdadas do pedido).
#[derive(Debug, Clone)]
pub struct AprovacaoRetorno {
    pub id_tecnico: Uuid,
    pub data_visita: NaiveDate,
    pub horario: NaiveTime,
}
