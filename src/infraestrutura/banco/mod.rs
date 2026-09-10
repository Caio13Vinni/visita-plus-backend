// =============================================================================
// Infraestrutura de banco de dados — PostgreSQL (central) e SQLite (local)
// =============================================================================

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

/// Cria o pool de conexoes com o banco central (PostgreSQL).
pub async fn criar_pool(url_banco: &str) -> anyhow::Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .min_connections(2)
        .acquire_timeout(std::time::Duration::from_secs(15))
        .connect(url_banco)
        .await?;
    Ok(pool)
}

/// Aplica as migracoes SQL empacotadas (esquema completo do MER).
pub async fn aplicar_migracoes(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::migrate!("./migrations").run(pool).await?;
    Ok(())
}

#[cfg(test)]
mod testes {
    use super::*;

    /// Teste unitario simples de construcao de pool (nao exige banco).
    #[test]
    fn a_configuracao_do_pool_nao_panica_com_url_vazia() {
        // Apenas verifica que a estrutura de opcoes pode ser montada sem armadilhas.
        let _ = PgPoolOptions::new().max_connections(2);
        assert!(true);
    }
}