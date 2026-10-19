// =============================================================================
// UC10 — Interface HTTP de notificacoes
// GET    /api/v1/notificacoes            -> listar (filtro: nao lidas)
// PATCH  /api/v1/notificacoes/:id/abrir  -> abrir (marca como lida)
// =============================================================================

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::dominio::entidades::notificacao::Notificacao;
use crate::dominio::enums::TipoNotificacao;
use crate::dominio::erros::ErroNotificacao;
use crate::interface::api::extratores::UsuarioAutenticado;
use crate::interface::api::modelos::ErroApi;
use crate::interface::estado::EstadoAplicacao;

/// Constroi o sub-roteador do modulo de notificacoes.
pub fn construir_rotas() -> Router<Arc<EstadoAplicacao>> {
    Router::new()
        .route("/", get(listar_notificacoes))
        .route("/{id}/abrir", axum::routing::patch(abrir_notificacao))
}

// -----------------------------------------------------------------------------
// DTOs
// -----------------------------------------------------------------------------

/// Filtros opcionais da listagem de notificacoes.
#[derive(Debug, Deserialize)]
pub struct FiltroNotificacaoQuery {
    pub somente_nao_lidas: Option<bool>,
}

/// Dados publicos da notificacao na resposta da API.
///
/// Titulo e mensagem sao sempre genericos (RN21): a API nunca expõe nome,
/// endereco ou situacao de familias neste recurso.
#[derive(Debug, Serialize)]
pub struct RespostaNotificacao {
    pub id: Uuid,
    pub titulo: String,
    pub mensagem: String,
    pub tipo: TipoNotificacao,
    pub lida: bool,
    pub lida_em: Option<DateTime<Utc>>,
    pub data_hora: DateTime<Utc>,
}

impl From<&Notificacao> for RespostaNotificacao {
    fn from(notificacao: &Notificacao) -> Self {
        Self {
            id: notificacao.id,
            titulo: notificacao.titulo.clone(),
            mensagem: notificacao.mensagem.clone(),
            tipo: notificacao.tipo,
            lida: notificacao.lida,
            lida_em: notificacao.lida_em,
            data_hora: notificacao.data_hora,
        }
    }
}

// -----------------------------------------------------------------------------
// Manipuladores
// -----------------------------------------------------------------------------

/// GET /notificacoes — lista as notificacoes do usuario autenticado, da mais
/// recente para a mais antiga (RN05/RN06).
async fn listar_notificacoes(
    State(estado): State<Arc<EstadoAplicacao>>,
    UsuarioAutenticado(claims): UsuarioAutenticado,
    Query(filtro): Query<FiltroNotificacaoQuery>,
) -> Result<Json<Vec<RespostaNotificacao>>, (StatusCode, Json<ErroApi>)> {
    let notificacoes = estado
        .notificacoes
        .listar_notificacoes(claims.sub, filtro.somente_nao_lidas.unwrap_or(false))
        .await
        .map_err(mapear_erro_notificacao)?;

    Ok(Json(
        notificacoes
            .iter()
            .map(RespostaNotificacao::from)
            .collect::<Vec<_>>(),
    ))
}

/// PATCH /notificacoes/{id}/abrir — abre a notificacao, confere o
/// destinatario e marca como lida (RN21/RN22).
async fn abrir_notificacao(
    State(estado): State<Arc<EstadoAplicacao>>,
    UsuarioAutenticado(claims): UsuarioAutenticado,
    Path(id): Path<Uuid>,
) -> Result<Json<RespostaNotificacao>, (StatusCode, Json<ErroApi>)> {
    let notificacao = estado
        .notificacoes
        .abrir_notificacao(claims.sub, id)
        .await
        .map_err(mapear_erro_notificacao)?;

    Ok(Json(RespostaNotificacao::from(&notificacao)))
}

// -----------------------------------------------------------------------------
// Conversao de erros
// -----------------------------------------------------------------------------

/// Converte erros de notificacoes em respostas HTTP.
fn mapear_erro_notificacao(erro: ErroNotificacao) -> (StatusCode, Json<ErroApi>) {
    match erro {
        ErroNotificacao::SemPermissao => {
            (StatusCode::FORBIDDEN, Json(ErroApi::nova(erro.to_string())))
        }
        ErroNotificacao::NaoEncontrado => {
            (StatusCode::NOT_FOUND, Json(ErroApi::nova(erro.to_string())))
        }
        ErroNotificacao::DadosInvalidos(_) => (
            StatusCode::BAD_REQUEST,
            Json(ErroApi::nova(erro.to_string())),
        ),
        ErroNotificacao::Tecnica(mensagem) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErroApi::nova(mensagem)),
        ),
    }
}
