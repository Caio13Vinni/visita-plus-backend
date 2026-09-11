// Manipuladores (handlers) das rotas HTTP da API.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde_json::json;
use std::sync::Arc;

use crate::interface::estado::EstadoAplicacao;

/// Rota pública de verificação de saúde da API:
/// confirma que o banco central responde e que as migrações estão aplicadas.
pub async fn verificar_saude(
    State(estado): State<Arc<EstadoAplicacao>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let migracao = sqlx::query_scalar::<_, i64>(
        "SELECT version FROM _sqlx_migrations ORDER BY version DESC LIMIT 1",
    )
    .fetch_optional(&estado.pool)
    .await
    .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;

    match migracao {
        Some(versao) => Ok(Json(json!({
            "status": "ok",
            "servico": "visita_plus",
            "schema_mer": "aplicado",
            "ultima_migracao": versao,
        }))),
        None => Ok(Json(json!({
            "status": "degradado",
            "servico": "visita_plus",
            "schema_mer": "pendente",
        }))),
    }
}