// =============================================================================
// Repositorio de visitas — contrato (trait) e implementacao PostgreSQL
// Na FASE 3 cobre apenas a regra de desativacao de tecnicos (UC02);
// a FASE 4 (UC04/UC05) expande este contrato.
// =============================================================================

use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

/// Contrato de persistencia da entidade Visita (parcial — expandido na FASE 4).
#[async_trait]
pub trait RepositorioVisita: Send + Sync {
    /// Verifica se o tecnico possui visitas futuras em status ativo ('Agendada').
    /// Usado pelo UC02 para impedir a desativacao de tecnico com compromissos.
    async fn existem_visitas_futuras_ativas(&self, id_tecnico: Uuid) -> Result<bool, sqlx::Error>;
}

/// Implementacao concreta do repositorio de visitas sobre o PostgreSQL.
#[derive(Clone)]
pub struct RepositorioVisitaPostgres {
    pool: PgPool,
}

impl RepositorioVisitaPostgres {
    /// Cria o repositorio com o pool fornecido.
    pub fn novo(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl RepositorioVisita for RepositorioVisitaPostgres {
    async fn existem_visitas_futuras_ativas(&self, id_tecnico: Uuid) -> Result<bool, sqlx::Error> {
        let existe: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT 1 FROM visita
                 WHERE id_tecnico = $1
                   AND status = 'Agendada'
                   AND (data_visita > CURRENT_DATE
                        OR (data_visita = CURRENT_DATE AND horario >= CURRENT_TIME))
             )",
        )
        .bind(id_tecnico)
        .fetch_one(&self.pool)
        .await?;
        Ok(existe)
    }
}
