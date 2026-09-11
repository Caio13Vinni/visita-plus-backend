// =============================================================================
// Estado compartilhado da aplicacao (pool, SQLite, servicos e casos de uso)
// =============================================================================

use sqlx::PgPool;
use std::env;
use std::sync::Arc;

use crate::casos_de_uso::autenticacao::CasoDeUsoAutenticacao;
use crate::infraestrutura::auditoria::ServicoAuditoria;
use crate::infraestrutura::repositorios::usuario_repositorio::RepositorioUsuarioPostgres;
use crate::infraestrutura::seguranca::jwt::ServicoJwt;

/// Estado transportado pelos handlers da API.
#[derive(Clone)]
pub struct EstadoAplicacao {
    /// Pool de conexoes com o banco central (PostgreSQL).
    pub pool: PgPool,
    /// Endereco do arquivo SQLite que simula o banco local mobile.
    pub endereco_sqlite: String,
    /// Caso de uso UC01 (autenticacao) compartilhado pelos handlers.
    pub autenticacao: CasoDeUsoAutenticacao,
    /// Servico de validacao de tokens JWT.
    pub servico_jwt: ServicoJwt,
}

impl EstadoAplicacao {
    /// Cria um novo estado compartilhado, montando os servicos da aplicacao.
    pub fn novo(pool: PgPool, endereco_sqlite: String) -> Arc<EstadoAplicacao> {
        let segredo = env::var("JWT_SEGREDO")
            .unwrap_or_else(|_| "segredo_de_desenvolvimento_visita_plus".to_string());
        let expiracao_minutos = env::var("JWT_EXPIRACAO_MINUTOS")
            .ok()
            .and_then(|valor| valor.parse().ok())
            .unwrap_or(1440);

        let servico_jwt = ServicoJwt::novo(segredo, expiracao_minutos);
        let servico_auditoria = ServicoAuditoria::novo(pool.clone());
        let repositorio = Arc::new(RepositorioUsuarioPostgres::novo(pool.clone()));
        let autenticacao =
            CasoDeUsoAutenticacao::novo(repositorio, servico_jwt.clone(), servico_auditoria);

        Arc::new(EstadoAplicacao {
            pool,
            endereco_sqlite,
            autenticacao,
            servico_jwt,
        })
    }
}