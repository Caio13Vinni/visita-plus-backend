// =============================================================================
// UC12 — Interface HTTP de consulta a auditoria
// GET    /api/v1/auditoria?id_usuario=&acao=&entidade=&entidade_id=
//                       &data_inicio=<RFC3339>&data_fim=<RFC3339>
// =============================================================================

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::dominio::entidades::auditoria::{EventoAuditoria, FiltroAuditoria};
use crate::dominio::erros::ErroAuditoria;
use crate::interface::api::extratores::UsuarioAutenticado;
use crate::interface::api::modelos::ErroApi;
use crate::interface::estado::EstadoAplicacao;

/// Constroi o sub-roteador do modulo de auditoria.
pub fn construir_rotas() -> Router<Arc<EstadoAplicacao>> {
    Router::new().route("/", get(consultar_auditoria))
}

// -----------------------------------------------------------------------------
// DTOs
// -----------------------------------------------------------------------------

/// Filtros opcionais da consulta de auditoria.
#[derive(Debug, Deserialize)]
pub struct FiltroAuditoriaQuery {
    pub id_usuario: Option<Uuid>,
    pub acao: Option<String>,
    pub entidade: Option<String>,
    pub entidade_id: Option<Uuid>,
    pub data_inicio: Option<DateTime<Utc>>,
    pub data_fim: Option<DateTime<Utc>>,
}

/// Evento da trilha na resposta da API.
#[derive(Debug, Serialize)]
pub struct RespostaEventoAuditoria {
    pub id: Uuid,
    pub id_usuario: Option<Uuid>,
    pub entidade: String,
    pub entidade_id: Uuid,
    pub acao: String,
    pub descricao: Option<String>,
    pub data_hora: DateTime<Utc>,
}

impl From<&EventoAuditoria> for RespostaEventoAuditoria {
    fn from(evento: &EventoAuditoria) -> Self {
        Self {
            id: evento.id,
            id_usuario: evento.id_usuario,
            entidade: evento.entidade.clone(),
            entidade_id: evento.entidade_id,
            acao: evento.acao.clone(),
            descricao: evento.descricao.clone(),
            data_hora: evento.data_hora,
        }
    }
}

// -----------------------------------------------------------------------------
// Manipuladores
// -----------------------------------------------------------------------------

/// GET /auditoria — lista as acoes criticas, da mais recente para a mais
/// antiga, com filtros por periodo, usuario, acao ou item (UC12).
async fn consultar_auditoria(
    State(estado): State<Arc<EstadoAplicacao>>,
    UsuarioAutenticado(claims): UsuarioAutenticado,
    Query(filtro): Query<FiltroAuditoriaQuery>,
) -> Result<Json<Vec<RespostaEventoAuditoria>>, (StatusCode, Json<ErroApi>)> {
    let filtro = FiltroAuditoria {
        id_usuario: filtro.id_usuario,
        acao: filtro.acao,
        entidade: filtro.entidade,
        entidade_id: filtro.entidade_id,
        data_inicio: filtro.data_inicio,
        data_fim: filtro.data_fim,
    };

    let eventos = estado
        .auditoria
        .consultar_auditoria(claims.sub, filtro)
        .await
        .map_err(mapear_erro_auditoria)?;

    Ok(Json(
        eventos
            .iter()
            .map(RespostaEventoAuditoria::from)
            .collect::<Vec<_>>(),
    ))
}

// -----------------------------------------------------------------------------
// Conversao de erros
// -----------------------------------------------------------------------------

/// Converte erros de consulta a auditoria em respostas HTTP.
fn mapear_erro_auditoria(erro: ErroAuditoria) -> (StatusCode, Json<ErroApi>) {
    match erro {
        ErroAuditoria::SemPermissao => {
            (StatusCode::FORBIDDEN, Json(ErroApi::nova(erro.to_string())))
        }
        ErroAuditoria::DadosInvalidos(_) => (
            StatusCode::BAD_REQUEST,
            Json(ErroApi::nova(erro.to_string())),
        ),
        ErroAuditoria::Tecnica(mensagem) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErroApi::nova(mensagem)),
        ),
    }
}
