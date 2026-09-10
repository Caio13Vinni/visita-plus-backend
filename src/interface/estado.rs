// =============================================================================
// Estado compartilhado da aplicacao (pool do banco central e caminho do SQLite)
// =============================================================================

use sqlx::PgPool;
use std::sync::Arc;

/// Estado transportado pelos handlers da API.
#[derive(Clone)]
pub struct EstadoAplicacao {
    /// Pool de conexoes com o banco central (PostgreSQL).
    pub pool: PgPool,
    /// Endereco do arquivo SQLite que simula o banco local mobile.
    pub endereco_sqlite: String,
}

impl EstadoAplicacao {
    /// Cria um novo estado compartilhado da aplicacao.
    pub fn novo(pool: PgPool, endereco_sqlite: String) -> Arc<EstadoAplicacao> {
        Arc::new(EstadoAplicacao {
            pool,
            endereco_sqlite,
        })
    }
}