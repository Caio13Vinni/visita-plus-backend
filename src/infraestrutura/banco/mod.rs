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
    fn a_configuracao_do_pool_aplica_limites_padrao() {
        let opcoes = PgPoolOptions::new()
            .max_connections(5)
            .min_connections(1)
            .acquire_timeout(std::time::Duration::from_secs(30));

        // Verifica que os limites configurados sao repassados ao pool
        assert_eq!(opcoes.get_max_connections(), 5);
        assert_eq!(opcoes.get_min_connections(), 1);
    }
}