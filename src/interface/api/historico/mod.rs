// =============================================================================
// UC09 — Interface HTTP de consulta ao historico
// GET    /api/v1/historico          -> listar (periodo, status, familia)
// GET    /api/v1/historico/:id      -> detalhar item (registro, complementos,
//                                      mudancas com autor e data)
// =============================================================================

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::dominio::entidades::auditoria::EventoAuditoria;
use crate::dominio::enums::StatusVisita;
use crate::dominio::erros::ErroHistorico;
use crate::infraestrutura::repositorios::visita_repositorio::FiltroVisita;
use crate::interface::api::extratores::UsuarioAutenticado;
use crate::interface::api::modelos::ErroApi;
use crate::interface::api::registros::{RespostaComplemento, RespostaRegistro};
use crate::interface::api::visitas::RespostaVisita;
use crate::interface::estado::EstadoAplicacao;

/// Constroi o sub-roteador do modulo de historico.
pub fn construir_rotas() -> Router<Arc<EstadoAplicacao>> {
    Router::new()
        .route("/", get(consultar_historico))
        .route("/{id}", get(detalhar_historico))
}

// -----------------------------------------------------------------------------
// DTOs
// -----------------------------------------------------------------------------

/// Filtros opcionais da consulta de historico.
#[derive(Debug, Deserialize)]
pub struct FiltroHistoricoQuery {
    pub id_familia: Option<Uuid>,
    pub status: Option<StatusVisita>,
    pub data_inicio: Option<NaiveDate>,
    pub data_fim: Option<NaiveDate>,
}

/// Mudanca auditada exibida no historico, com autor e data (RN18/RN22).
#[derive(Debug, Serialize)]
pub struct RespostaMudanca {
    pub id: Uuid,
    pub id_usuario: Option<Uuid>,
    pub acao: String,
    pub descricao: Option<String>,
    pub data_hora: DateTime<Utc>,
}

impl From<&EventoAuditoria> for RespostaMudanca {
    fn from(evento: &EventoAuditoria) -> Self {
        Self {
            id: evento.id,
            id_usuario: evento.id_usuario,
            acao: evento.acao.clone(),
            descricao: evento.descricao.clone(),
            data_hora: evento.data_hora,
        }
    }
}

/// Detalhe de um item do historico na resposta da API.
#[derive(Debug, Serialize)]
pub struct RespostaDetalheHistorico {
    pub visita: RespostaVisita,
    pub registro: Option<RespostaRegistro>,
    pub complementos: Vec<RespostaComplemento>,
    pub mudancas: Vec<RespostaMudanca>,
}

// -----------------------------------------------------------------------------
// Manipuladores
// -----------------------------------------------------------------------------

/// GET /historico — lista as visitas do historico, da mais recente para a
/// mais antiga, conforme o perfil (RN05/RN06).
async fn consultar_historico(
    State(estado): State<Arc<EstadoAplicacao>>,
    UsuarioAutenticado(claims): UsuarioAutenticado,
    Query(filtro): Query<FiltroHistoricoQuery>,
) -> Result<Json<Vec<RespostaVisita>>, (StatusCode, Json<ErroApi>)> {
    let filtro = FiltroVisita {
        id_tecnico: None,
        id_familia: filtro.id_familia,
        id_unidade: None,
        status: filtro.status,
        data_inicio: filtro.data_inicio,
        data_fim: filtro.data_fim,
    };

    let visitas = estado
        .historico
        .consultar_historico(claims.sub, filtro)
        .await
        .map_err(mapear_erro_historico)?;

    Ok(Json(
        visitas.iter().map(RespostaVisita::from).collect::<Vec<_>>(),
    ))
}

/// GET /historico/{id} — detalha um item: registro, complementos e mudancas
/// com autor e data (RN16 a RN18).
async fn detalhar_historico(
    State(estado): State<Arc<EstadoAplicacao>>,
    UsuarioAutenticado(claims): UsuarioAutenticado,
    Path(id): Path<Uuid>,
) -> Result<Json<RespostaDetalheHistorico>, (StatusCode, Json<ErroApi>)> {
    let detalhe = estado
        .historico
        .detalhar_historico(claims.sub, id)
        .await
        .map_err(mapear_erro_historico)?;

    Ok(Json(RespostaDetalheHistorico {
        visita: RespostaVisita::from(&detalhe.visita),
        registro: detalhe.registro.as_ref().map(RespostaRegistro::from),
        complementos: detalhe
            .complementos
            .iter()
            .map(RespostaComplemento::from)
            .collect(),
        mudancas: detalhe.mudancas.iter().map(RespostaMudanca::from).collect(),
    }))
}

// -----------------------------------------------------------------------------
// Conversao de erros
// -----------------------------------------------------------------------------

/// Converte erros de consulta ao historico em respostas HTTP.
fn mapear_erro_historico(erro: ErroHistorico) -> (StatusCode, Json<ErroApi>) {
    match erro {
        ErroHistorico::SemPermissao => {
            (StatusCode::FORBIDDEN, Json(ErroApi::nova(erro.to_string())))
        }
        ErroHistorico::NaoEncontrado => {
            (StatusCode::NOT_FOUND, Json(ErroApi::nova(erro.to_string())))
        }
        ErroHistorico::DadosInvalidos(_) => (
            StatusCode::BAD_REQUEST,
            Json(ErroApi::nova(erro.to_string())),
        ),
        ErroHistorico::Tecnica(mensagem) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErroApi::nova(mensagem)),
        ),
    }
}
