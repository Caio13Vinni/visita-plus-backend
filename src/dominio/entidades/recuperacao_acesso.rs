// =============================================================================
// Entidade RecuperacaoAcesso — codigo de recuperacao de acesso (UC01/RF03)
// Mapeada para a tabela `recuperacao_acesso` do MER.
// =============================================================================

use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Codigo temporario de recuperacao de acesso vinculado a um usuario (RF03).
/// Uso unico: apos consumido na redefinicao, `usado_em` e preenchido.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecuperacaoAcesso {
    pub id: Uuid,
    pub id_usuario: Uuid,
    /// Codigo numerico de 6 digitos informado pelo usuario na redefinicao.
    pub codigo: String,
    /// Limite de validade do codigo (15 minutos a partir da solicitacao).
    pub expira_em: DateTime<Utc>,
    /// Preenchido quando o codigo e consumido na redefinicao de senha.
    pub usado_em: Option<DateTime<Utc>>,
    pub criado_em: DateTime<Utc>,
}
