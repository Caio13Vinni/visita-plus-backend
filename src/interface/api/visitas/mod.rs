// =============================================================================
// UC04 — Interface HTTP de agendamento de visitas
// GET    /api/v1/visitas               -> listar (tecnico: propria agenda)
// POST   /api/v1/visitas               -> agendar
// GET    /api/v1/visitas/:id           -> detalhar
// PATCH  /api/v1/visitas/:id           -> reagendar
// PATCH  /api/v1/visitas/:id/cancelar  -> cancelar (com motivo)
// =============================================================================

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, patch};
use axum::{Json, Router};
use chrono::{NaiveDate, NaiveTime};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::dominio::entidades::visita::{CadastroVisita, ReagendamentoVisita, Visita};
use crate::dominio::enums::{PrioridadeVisita, StatusVisita};
use crate::dominio::erros::ErroAgendamento;
use crate::infraestrutura::repositorios::visita_repositorio::FiltroVisita;
use crate::interface::api::extratores::UsuarioAutenticado;
use crate::interface::api::modelos::ErroApi;
use crate::interface::estado::EstadoAplicacao;

/// Constroi o sub-roteador do modulo de visitas.
pub fn construir_rotas() -> Router<Arc<EstadoAplicacao>> {
    Router::new()
        .route("/", get(listar_visitas).post(agendar_visita))
        .route("/{id}", get(detalhar_visita).patch(reagendar_visita))
        .route("/{id}/cancelar", patch(cancelar_visita))
}

// -----------------------------------------------------------------------------
// DTOs
// -----------------------------------------------------------------------------

/// Filtros opcionais da listagem de visitas.
#[derive(Debug, Deserialize)]
pub struct FiltroVisitaQuery {
    pub id_tecnico: Option<Uuid>,
    pub id_familia: Option<Uuid>,
    pub id_unidade: Option<Uuid>,
    pub status: Option<StatusVisita>,
    pub data_inicio: Option<NaiveDate>,
    pub data_fim: Option<NaiveDate>,
}

/// Corpo da requisicao de agendamento.
#[derive(Debug, Deserialize)]
pub struct RequisicaoVisita {
    pub id_familia: Uuid,
    pub id_tecnico: Uuid,
    pub data_visita: NaiveDate,
    pub horario: NaiveTime,
    pub prioridade: PrioridadeVisita,
    pub objetivo: String,
}

/// Corpo da requisicao de reagendamento.
#[derive(Debug, Deserialize)]
pub struct RequisicaoReagendamento {
    pub data_visita: NaiveDate,
    pub horario: NaiveTime,
    pub id_tecnico: Option<Uuid>,
}

/// Corpo da requisicao de cancelamento.
#[derive(Debug, Deserialize)]
pub struct RequisicaoCancelamento {
    pub motivo: String,
}

/// Dados publicos de visita na resposta da API.
#[derive(Debug, Serialize)]
pub struct RespostaVisita {
    pub id: Uuid,
    pub id_familia: Uuid,
    pub id_tecnico: Uuid,
    pub id_agendado_por: Uuid,
    pub data_visita: NaiveDate,
    pub horario: NaiveTime,
    pub prioridade: PrioridadeVisita,
    pub objetivo: String,
    pub status: StatusVisita,
    pub motivo_cancelamento: Option<String>,
}

impl From<&Visita> for RespostaVisita {
    fn from(visita: &Visita) -> Self {
        Self {
            id: visita.id,
            id_familia: visita.id_familia,
            id_tecnico: visita.id_tecnico,
            id_agendado_por: visita.id_agendado_por,
            data_visita: visita.data_visita,
            horario: visita.horario,
            prioridade: visita.prioridade,
            objetivo: visita.objetivo.clone(),
            status: visita.status,
            motivo_cancelamento: visita.motivo_cancelamento.clone(),
        }
    }
}

// -----------------------------------------------------------------------------
// Manipuladores
// -----------------------------------------------------------------------------

/// GET /visitas — lista as visitas conforme o perfil (RN05/RN06).
async fn listar_visitas(
    State(estado): State<Arc<EstadoAplicacao>>,
    UsuarioAutenticado(claims): UsuarioAutenticado,
    Query(filtro): Query<FiltroVisitaQuery>,
) -> Result<Json<Vec<RespostaVisita>>, (StatusCode, Json<ErroApi>)> {
    let filtro = FiltroVisita {
        id_tecnico: filtro.id_tecnico,
        id_familia: filtro.id_familia,
        id_unidade: filtro.id_unidade,
        status: filtro.status,
        data_inicio: filtro.data_inicio,
        data_fim: filtro.data_fim,
    };

    let visitas = estado
        .agendamento
        .listar_visitas(claims.sub, filtro)
        .await
        .map_err(mapear_erro_agendamento)?;

    Ok(Json(
        visitas.iter().map(RespostaVisita::from).collect::<Vec<_>>(),
    ))
}

/// POST /visitas — agenda uma nova visita.
async fn agendar_visita(
    State(estado): State<Arc<EstadoAplicacao>>,
    UsuarioAutenticado(claims): UsuarioAutenticado,
    Json(corpo): Json<RequisicaoVisita>,
) -> Result<(StatusCode, Json<RespostaVisita>), (StatusCode, Json<ErroApi>)> {
    let cadastro = CadastroVisita {
        id_familia: corpo.id_familia,
        id_tecnico: corpo.id_tecnico,
        data_visita: corpo.data_visita,
        horario: corpo.horario,
        prioridade: corpo.prioridade,
        objetivo: corpo.objetivo,
    };

    let criada = estado
        .agendamento
        .agendar_visita(claims.sub, cadastro)
        .await
        .map_err(mapear_erro_agendamento)?;

    Ok((StatusCode::CREATED, Json(RespostaVisita::from(&criada))))
}

/// GET /visitas/{id} — detalha um agendamento.
async fn detalhar_visita(
    State(estado): State<Arc<EstadoAplicacao>>,
    UsuarioAutenticado(claims): UsuarioAutenticado,
    Path(id): Path<Uuid>,
) -> Result<Json<RespostaVisita>, (StatusCode, Json<ErroApi>)> {
    let visita = estado
        .agendamento
        .detalhar_visita(claims.sub, id)
        .await
        .map_err(mapear_erro_agendamento)?;

    Ok(Json(RespostaVisita::from(&visita)))
}

/// PATCH /visitas/{id} — reagenda a visita (data, horario e/ou tecnico).
async fn reagendar_visita(
    State(estado): State<Arc<EstadoAplicacao>>,
    UsuarioAutenticado(claims): UsuarioAutenticado,
    Path(id): Path<Uuid>,
    Json(corpo): Json<RequisicaoReagendamento>,
) -> Result<Json<RespostaVisita>, (StatusCode, Json<ErroApi>)> {
    let reagendamento = ReagendamentoVisita {
        data_visita: corpo.data_visita,
        horario: corpo.horario,
        id_tecnico: corpo.id_tecnico,
    };

    let atualizada = estado
        .agendamento
        .reagendar_visita(claims.sub, id, reagendamento)
        .await
        .map_err(mapear_erro_agendamento)?;

    Ok(Json(RespostaVisita::from(&atualizada)))
}

/// PATCH /visitas/{id}/cancelar — cancela o agendamento, com motivo (RN18).
async fn cancelar_visita(
    State(estado): State<Arc<EstadoAplicacao>>,
    UsuarioAutenticado(claims): UsuarioAutenticado,
    Path(id): Path<Uuid>,
    Json(corpo): Json<RequisicaoCancelamento>,
) -> Result<Json<RespostaVisita>, (StatusCode, Json<ErroApi>)> {
    let cancelada = estado
        .agendamento
        .cancelar_visita(claims.sub, id, &corpo.motivo)
        .await
        .map_err(mapear_erro_agendamento)?;

    Ok(Json(RespostaVisita::from(&cancelada)))
}

// -----------------------------------------------------------------------------
// Conversao de erros
// -----------------------------------------------------------------------------

/// Converte erros de agendamento em respostas HTTP.
fn mapear_erro_agendamento(erro: ErroAgendamento) -> (StatusCode, Json<ErroApi>) {
    match erro {
        ErroAgendamento::SemPermissao => {
            (StatusCode::FORBIDDEN, Json(ErroApi::nova(erro.to_string())))
        }
        ErroAgendamento::NaoEncontrado => {
            (StatusCode::NOT_FOUND, Json(ErroApi::nova(erro.to_string())))
        }
        ErroAgendamento::FamiliaInativa | ErroAgendamento::ConflitoDeHorario => {
            (StatusCode::CONFLICT, Json(ErroApi::nova(erro.to_string())))
        }
        ErroAgendamento::VisitaNaoPodeSerAlterada => {
            (StatusCode::CONFLICT, Json(ErroApi::nova(erro.to_string())))
        }
        ErroAgendamento::TecnicoInvalido
        | ErroAgendamento::DataRetroativa
        | ErroAgendamento::MotivoObrigatorio
        | ErroAgendamento::DadosInvalidos(_) => (
            StatusCode::BAD_REQUEST,
            Json(ErroApi::nova(erro.to_string())),
        ),
        ErroAgendamento::Tecnica(mensagem) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErroApi::nova(mensagem)),
        ),
    }
}
