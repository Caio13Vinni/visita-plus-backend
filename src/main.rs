// =============================================================================
// VISITA+ — Ponto de entrada da API
// -----------------------------------------------------------------------------
// Carrega as variaveis de ambiente, conecta no banco central (PostgreSQL),
// aplica as migracoes do MER e inicia o servidor HTTP.
// =============================================================================

use visita_plus::infraestrutura::banco::{aplicar_migracoes, criar_pool};
use visita_plus::interface::api::construir_roteador;
use visita_plus::interface::estado::EstadoAplicacao;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Carrega o arquivo .env quando existir
    dotenvy::dotenv().ok();

    // Inicializa o log estruturado
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let url_banco = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://visita:visita_forte_2026@localhost:5432/visita_plus".to_string()
    });
    let endereco_sqlite = std::env::var("SQLITE_ENDERECO")
        .unwrap_or_else(|_| "/tmp/visita_plus_local.db".to_string());

    // Conexao com o banco central (PostgreSQL)
    let pool = criar_pool(&url_banco).await?;

    // Aplica as migracoes do MER (schema completo das 12 tabelas)
    aplicar_migracoes(&pool).await?;
    tracing::info!("Migracoes do MER aplicadas com sucesso.");

    tracing::info!("Banco local SQLite configurado em: {}", endereco_sqlite);

    // Estado compartilhado da aplicacao
    let estado = EstadoAplicacao::novo(pool, endereco_sqlite);

    // Constroi e inicia o servidor HTTP
    let roteador = construir_roteador(estado);

    let host = std::env::var("API_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let porta = std::env::var("API_PORTA").unwrap_or_else(|_| "8080".to_string());
    let endereco = format!("{}:{}", host, porta);

    let ouvinte = tokio::net::TcpListener::bind(&endereco).await?;
    tracing::info!("Visita+ API ouvindo em http://{}", endereco);

    axum::serve(ouvinte, roteador).await?;
    Ok(())
}
