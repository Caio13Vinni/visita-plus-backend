// =============================================================================
// UC06/RF17 — Interface HTTP de encaminhamentos
// POST   /api/v1/encaminhamentos              -> registrar (tecnico)
// GET    /api/v1/encaminhamentos?id_visita=   -> listar (conforme perfil)
// PATCH  /api/v1/encaminhamentos/:id/concluir -> concluir
// =============================================================================

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, patch};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::dominio::entidades::encaminhamento::Encaminhamento;
use crate::dominio::enums::StatusEncaminhamento;
use crate::dominio::erros::ErroEncaminhamento;
use crate::interface::api::extratores::UsuarioAutenticado;
use crate::interface::api::modelos::ErroApi;
use crate::interface::estado::EstadoAplicacao;

/// Constroi o sub-roteador do modulo de encaminhamentos.
pub fn construir_rotas() -> Router<Arc<EstadoAplicacao>> {
    Router::new()
        .route("/", get(listar_encaminhamentos).post(registrar_encaminhamento))
        .route("/{id}/concluir", patch(concluir_encaminhamento))
}

// -----------------------------------------------------------------------------
// DTOs
// -----------------------------------------------------------------------------

/// Filtros da listagem de encaminhamentos.
#[derive(Debug, Deserialize)]
pub struct FiltroEncaminhamentoQuery {
    pub id_visita: Uuid,
}

/// Corpo da requisicao de registro de encaminhamento.
#[derive(Debug, Deserialize)]
pub struct RequisicaoEncaminhamento {
    pub id_visita: Uuid,
    pub destino: String,
    pub motivo: String,
}

/// Dados publicos do encaminhamento na resposta da API.
#[derive(Debug, Serialize)]
pub struct RespostaEncaminhamento {
    pub id: Uuid,
    pub id_visita: Uuid,
    pub destino: String,
    pub motivo: String,
    pub data_encaminhamento: DateTime<Utc>,
    pub status: StatusEncaminhamento,
    pub registrado_por: Uuid,
}

impl From<&Encaminhamento> for RespostaEncaminhamento {
    fn from(encaminhamento: &Encaminhamento) -> Self {
        Self {
            id: encaminhamento.id,
            id_visita: encaminhamento.id_visita,
            destino: encaminhamento.destino.clone(),
            motivo: encaminhamento.motivo.clone(),
            data_encaminhamento: encaminhamento.data_encaminhamento,
            status: encaminhamento.status,
            registrado_por: encaminhamento.registrado_por,
        }
    }
}

// -----------------------------------------------------------------------------
// Manipuladores
// -----------------------------------------------------------------------------

/// GET /encaminhamentos?id_visita= — lista os encaminhamentos da visita,
/// conforme o perfil (RN05/RN06).
async fn listar_encaminhamentos(
    State(estado): State<Arc<EstadoAplicacao>>,
    UsuarioAutenticado(claims): UsuarioAutenticado,
    Query(filtro): Query<FiltroEncaminhamentoQuery>,
) -> Result<Json<Vec<RespostaEncaminhamento>>, (StatusCode, Json<ErroApi>)> {
    let encaminhamentos = estado
        .encaminhamentos
        .listar_encaminhamentos(claims.sub, filtro.id_visita)
        .await
        .map_err(mapear_erro_encaminhamento)?;

    Ok(Json(
        encaminhamentos
            .iter()
            .map(RespostaEncaminhamento::from)
            .collect::<Vec<_>>(),
    ))
}

/// POST /encaminhamentos — registra um encaminhamento durante o
/// atendimento (RF17).
async fn registrar_encaminhamento(
    State(estado): State<Arc<EstadoAplicacao>>,
    UsuarioAutenticado(claims): UsuarioAutenticado,
    Json(corpo): Json<RequisicaoEncaminhamento>,
) -> Result<(StatusCode, Json<RespostaEncaminhamento>), (StatusCode, Json<ErroApi>)> {
    let criado = estado
        .encaminhamentos
        .registrar_encaminhamento(claims.sub, corpo.id_visita, &corpo.destino, &corpo.motivo)
        .await
        .map_err(mapear_erro_encaminhamento)?;

    Ok((StatusCode::CREATED, Json(RespostaEncaminhamento::from(&criado))))
}

/// PATCH /encaminhamentos/{id}/concluir — conclui o encaminhamento.
async fn concluir_encaminhamento(
    State(estado): State<Arc<EstadoAplicacao>>,
    UsuarioAutenticado(claims): UsuarioAutenticado,
    Path(id): Path<Uuid>,
) -> Result<Json<RespostaEncaminhamento>, (StatusCode, Json<ErroApi>)> {
    let concluido = estado
        .encaminhamentos
        .concluir_encaminhamento(claims.sub, id)
        .await
        .map_err(mapear_erro_encaminhamento)?;

    Ok(Json(RespostaEncaminhamento::from(&concluido)))
}

// -----------------------------------------------------------------------------
// Conversao de erros
// -----------------------------------------------------------------------------

/// Converte erros de encaminhamentos em respostas HTTP.
fn mapear_erro_encaminhamento(erro: ErroEncaminhamento) -> (StatusCode, Json<ErroApi>) {
    match erro {
        ErroEncaminhamento::SemPermissao => {
            (StatusCode::FORBIDDEN, Json(ErroApi::nova(erro.to_string())))
        }
        ErroEncaminhamento::NaoEncontrado => {
            (StatusCode::NOT_FOUND, Json(ErroApi::nova(erro.to_string())))
        }
        ErroEncaminhamento::VisitaInvalida | ErroEncaminhamento::EstadoInvalido => {
            (StatusCode::CONFLICT, Json(ErroApi::nova(erro.to_string())))
        }
        ErroEncaminhamento::DestinoObrigatorio
        | ErroEncaminhamento::MotivoObrigatorio
        | ErroEncaminhamento::DadosInvalidos(_) => {
            (StatusCode::BAD_REQUEST, Json(ErroApi::nova(erro.to_string())))
        }
        ErroEncaminhamento::Tecnica(mensagem) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErroApi::nova(mensagem)),
        ),
    }
}
