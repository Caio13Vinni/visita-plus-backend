// =============================================================================
// Sincronizacao offline/online — Interface HTTP
// POST /api/v1/sincronizacao -> envia pendencias locais e recebe a agenda
// =============================================================================

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use serde::Serialize;
use std::sync::Arc;

use crate::casos_de_uso::sincronizacao::ResumoSincronizacao;
use crate::dominio::erros::ErroSincronizacao;
use crate::interface::api::extratores::UsuarioAutenticado;
use crate::interface::api::modelos::ErroApi;
use crate::interface::estado::EstadoAplicacao;

/// Constroi o sub-roteador do modulo de sincronizacao.
pub fn construir_rotas() -> Router<Arc<EstadoAplicacao>> {
    Router::new().route("/", post(sincronizar))
}

/// Resposta da sincronizacao executada.
#[derive(Debug, Serialize)]
pub struct RespostaSincronizacao {
    pub enviados: u64,
    pub recebidos: u64,
    pub erros: u64,
}

impl From<&ResumoSincronizacao> for RespostaSincronizacao {
    fn from(resumo: &ResumoSincronizacao) -> Self {
        Self {
            enviados: resumo.enviados,
            recebidos: resumo.recebidos,
            erros: resumo.erros,
        }
    }
}

/// POST /sincronizacao — envia as pendencias do banco local para o
/// central e reflete a agenda do perfil no banco local (SQLite).
async fn sincronizar(
    State(estado): State<Arc<EstadoAplicacao>>,
    UsuarioAutenticado(claims): UsuarioAutenticado,
) -> Result<Json<RespostaSincronizacao>, (StatusCode, Json<ErroApi>)> {
    let resumo = estado
        .sincronizacao
        .sincronizar(claims.sub)
        .await
        .map_err(mapear_erro_sincronizacao)?;

    Ok(Json(RespostaSincronizacao::from(&resumo)))
}

/// Converte erros de sincronizacao em respostas HTTP.
fn mapear_erro_sincronizacao(erro: ErroSincronizacao) -> (StatusCode, Json<ErroApi>) {
    match erro {
        ErroSincronizacao::SemPermissao => {
            (StatusCode::FORBIDDEN, Json(ErroApi::nova(erro.to_string())))
        }
        ErroSincronizacao::NaoEncontrado => {
            (StatusCode::NOT_FOUND, Json(ErroApi::nova(erro.to_string())))
        }
        ErroSincronizacao::VisitaInvalida | ErroSincronizacao::PendenciaInvalida(_) => {
            (StatusCode::CONFLICT, Json(ErroApi::nova(erro.to_string())))
        }
        ErroSincronizacao::Tecnica(mensagem) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErroApi::nova(mensagem)),
        ),
    }
}
