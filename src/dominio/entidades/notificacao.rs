// =============================================================================
// Entidade Notificacao — aviso generico ao usuario (UC10)
// Mapeada para a tabela `notificacao` do MER.
// =============================================================================

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::dominio::enums::TipoNotificacao;

/// Notificacao entregue a um usuario. O texto e sempre generico (RN21):
/// nunca expõe nome, endereco ou situacao de familias.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notificacao {
    pub id: Uuid,
    /// Destinatario da notificacao (visivel somente para ele — RN05/RN06).
    pub id_usuario_destino: Uuid,
    pub titulo: String,
    pub mensagem: String,
    pub tipo: TipoNotificacao,
    /// Indica se o usuario ja abriu a notificacao.
    pub lida: bool,
    /// Horario em que a notificacao foi aberta (marcada como lida).
    pub lida_em: Option<DateTime<Utc>>,
    pub data_hora: DateTime<Utc>,
}
