// =============================================================================
// Entidade Sessao — sessao ativa do usuario (UC01/RF04)
// Mapeada para a tabela `sessao` do MER.
// =============================================================================

use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Sessao de acesso vinculada ao token JWT emitido no login (RF04).
/// Permite encerramento manual e expiracao por inatividade (RNF07).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sessao {
    pub id: Uuid,
    pub id_usuario: Uuid,
    pub token_jwt: String,
    pub criado_em: DateTime<Utc>,
    pub expira_em: DateTime<Utc>,
    /// Momento do encerramento manual (logout); nulo enquanto ativa.
    pub encerrada_em: Option<DateTime<Utc>>,
    pub ip_origem: Option<String>,
    pub dispositivo: Option<String>,
}

impl Sessao {
    /// Indica se a sessao ainda pode ser usada: nao encerrada e dentro
    /// da validade (RF04/RNF07).
    pub fn esta_ativa_em(&self, agora: DateTime<Utc>) -> bool {
        self.encerrada_em.is_none() && self.expira_em > agora
    }
}
