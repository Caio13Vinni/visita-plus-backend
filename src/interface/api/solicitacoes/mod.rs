// =============================================================================
// UC08 — Interface HTTP de solicitacao de retorno
// GET    /api/v1/solicitacoes-retorno               -> listar (conforme perfil)
// POST   /api/v1/solicitacoes-retorno               -> solicitar (tecnico)
// PATCH  /api/v1/solicitacoes-retorno/:id/aprovar   -> aprovar e agendar
// PATCH  /api/v1/solicitacoes-retorno/:id/rejeitar  -> rejeitar (com motivo)
// PATCH  /api/v1/solicitacoes-retorno/:id/cancelar  -> cancelar (solicitante)
// =============================================================================

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, patch};
use axum::{Json, Router};
use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::dominio::entidades::solicitacao_retorno::{AprovacaoRetorno, SolicitacaoRetorno};
use crate::dominio::enums::{PrioridadeVisita, StatusSolicitacao};
use crate::dominio::erros::ErroSolicitacao;
use crate::infraestrutura::repositorios::solicitacao_repositorio::FiltroSolicitacao;
use crate::interface::api::extratores::UsuarioAutenticado;
use crate::interface::api::modelos::ErroApi;
use crate::interface::api::visitas::RespostaVisita;
use crate::interface::estado::EstadoAplicacao;

/// Constroi o sub-roteador do modulo de solicitacoes de retorno.
pub fn construir_rotas() -> Router<Arc<EstadoAplicacao>> {
    Router::new()
        .route("/", get(listar_solicitacoes).post(solicitar_retorno))
        .route("/{id}/aprovar", patch(aprovar_retorno))
        .route("/{id}/rejeitar", patch(rejeitar_retorno))
        .route("/{id}/cancelar", patch(cancelar_solicitacao))
}

// -----------------------------------------------------------------------------
// DTOs
// -----------------------------------------------------------------------------

/// Filtros opcionais da listagem de solicitacoes.
#[derive(Debug, Deserialize)]
pub struct FiltroSolicitacaoQuery {
    pub status: Option<StatusSolicitacao>,
}

/// Corpo da requisicao de solicitacao de retorno.
#[derive(Debug, Deserialize)]
pub struct RequisicaoSolicitacao {
    pub id_visita: Uuid,
    pub motivo: String,
    pub prioridade: PrioridadeVisita,
}

/// Corpo da requisicao de aprovacao (agendamento do retorno).
#[derive(Debug, Deserialize)]
pub struct RequisicaoAprovacao {
    pub id_tecnico: Uuid,
    pub data_visita: NaiveDate,
    pub horario: NaiveTime,
}

/// Corpo da requisicao de rejeicao.
#[derive(Debug, Deserialize)]
pub struct RequisicaoRejeicao {
    pub justificativa: String,
}

/// Dados publicos da solicitacao de retorno na resposta da API.
#[derive(Debug, Serialize)]
pub struct RespostaSolicitacao {
    pub id: Uuid,
    pub id_visita: Uuid,
    pub motivo: String,
    pub prioridade: PrioridadeVisita,
    pub solicitado_por: Uuid,
    pub data_solicitacao: DateTime<Utc>,
    pub status: StatusSolicitacao,
    pub decidido_por: Option<Uuid>,
    pub decisao_motivo: Option<String>,
    pub decidido_em: Option<DateTime<Utc>>,
}

impl From<&SolicitacaoRetorno> for RespostaSolicitacao {
    fn from(solicitacao: &SolicitacaoRetorno) -> Self {
        Self {
            id: solicitacao.id,
            id_visita: solicitacao.id_visita,
            motivo: solicitacao.motivo.clone(),
            prioridade: solicitacao.prioridade,
            solicitado_por: solicitacao.solicitado_por,
            data_solicitacao: solicitacao.data_solicitacao,
            status: solicitacao.status,
            decidido_por: solicitacao.decidido_por,
            decisao_motivo: solicitacao.decisao_motivo.clone(),
            decidido_em: solicitacao.decidido_em,
        }
    }
}

/// Resposta da aprovacao: o pedido atualizado e a visita de retorno criada.
#[derive(Debug, Serialize)]
pub struct RespostaAprovacao {
    pub solicitacao: RespostaSolicitacao,
    pub visita: RespostaVisita,
}

// -----------------------------------------------------------------------------
// Manipuladores
// -----------------------------------------------------------------------------

/// GET /solicitacoes-retorno — lista os pedidos conforme o perfil (RN05/RN06).
async fn listar_solicitacoes(
    State(estado): State<Arc<EstadoAplicacao>>,
    UsuarioAutenticado(claims): UsuarioAutenticado,
    Query(filtro): Query<FiltroSolicitacaoQuery>,
) -> Result<Json<Vec<RespostaSolicitacao>>, (StatusCode, Json<ErroApi>)> {
    let filtro = FiltroSolicitacao {
        status: filtro.status,
        ..Default::default()
    };

    let solicitacoes = estado
        .solicitacao_retorno
        .listar_solicitacoes(claims.sub, filtro)
        .await
        .map_err(mapear_erro_solicitacao)?;

    Ok(Json(
        solicitacoes
            .iter()
            .map(RespostaSolicitacao::from)
            .collect::<Vec<_>>(),
    ))
}

/// POST /solicitacoes-retorno — solicita retorno para visita realizada (RN19).
async fn solicitar_retorno(
    State(estado): State<Arc<EstadoAplicacao>>,
    UsuarioAutenticado(claims): UsuarioAutenticado,
    Json(corpo): Json<RequisicaoSolicitacao>,
) -> Result<(StatusCode, Json<RespostaSolicitacao>), (StatusCode, Json<ErroApi>)> {
    let criada = estado
        .solicitacao_retorno
        .solicitar_retorno(claims.sub, corpo.id_visita, &corpo.motivo, corpo.prioridade)
        .await
        .map_err(mapear_erro_solicitacao)?;

    Ok((
        StatusCode::CREATED,
        Json(RespostaSolicitacao::from(&criada)),
    ))
}

/// PATCH /solicitacoes-retorno/{id}/aprovar — aprova e agenda o retorno
/// (RN19/RN20).
async fn aprovar_retorno(
    State(estado): State<Arc<EstadoAplicacao>>,
    UsuarioAutenticado(claims): UsuarioAutenticado,
    Path(id): Path<Uuid>,
    Json(corpo): Json<RequisicaoAprovacao>,
) -> Result<Json<RespostaAprovacao>, (StatusCode, Json<ErroApi>)> {
    let aprovacao = AprovacaoRetorno {
        id_tecnico: corpo.id_tecnico,
        data_visita: corpo.data_visita,
        horario: corpo.horario,
    };

    let resultado = estado
        .solicitacao_retorno
        .aprovar_retorno(claims.sub, id, aprovacao)
        .await
        .map_err(mapear_erro_solicitacao)?;

    Ok(Json(RespostaAprovacao {
        solicitacao: RespostaSolicitacao::from(&resultado.solicitacao),
        visita: RespostaVisita::from(&resultado.visita),
    }))
}

/// PATCH /solicitacoes-retorno/{id}/rejeitar — rejeita o pedido, com
/// justificativa obrigatoria (RN19).
async fn rejeitar_retorno(
    State(estado): State<Arc<EstadoAplicacao>>,
    UsuarioAutenticado(claims): UsuarioAutenticado,
    Path(id): Path<Uuid>,
    Json(corpo): Json<RequisicaoRejeicao>,
) -> Result<Json<RespostaSolicitacao>, (StatusCode, Json<ErroApi>)> {
    let rejeitada = estado
        .solicitacao_retorno
        .rejeitar_retorno(claims.sub, id, &corpo.justificativa)
        .await
        .map_err(mapear_erro_solicitacao)?;

    Ok(Json(RespostaSolicitacao::from(&rejeitada)))
}

/// PATCH /solicitacoes-retorno/{id}/cancelar — cancela o proprio pedido
/// enquanto pendente (RN20).
async fn cancelar_solicitacao(
    State(estado): State<Arc<EstadoAplicacao>>,
    UsuarioAutenticado(claims): UsuarioAutenticado,
    Path(id): Path<Uuid>,
) -> Result<Json<RespostaSolicitacao>, (StatusCode, Json<ErroApi>)> {
    let cancelada = estado
        .solicitacao_retorno
        .cancelar_solicitacao(claims.sub, id)
        .await
        .map_err(mapear_erro_solicitacao)?;

    Ok(Json(RespostaSolicitacao::from(&cancelada)))
}

// -----------------------------------------------------------------------------
// Conversao de erros
// -----------------------------------------------------------------------------

/// Converte erros de solicitacao de retorno em respostas HTTP.
fn mapear_erro_solicitacao(erro: ErroSolicitacao) -> (StatusCode, Json<ErroApi>) {
    match erro {
        ErroSolicitacao::SemPermissao => {
            (StatusCode::FORBIDDEN, Json(ErroApi::nova(erro.to_string())))
        }
        ErroSolicitacao::NaoEncontrado => {
            (StatusCode::NOT_FOUND, Json(ErroApi::nova(erro.to_string())))
        }
        ErroSolicitacao::SolicitacaoAbertaExistente
        | ErroSolicitacao::EstadoInvalido
        | ErroSolicitacao::ConflitoDeHorario => {
            (StatusCode::CONFLICT, Json(ErroApi::nova(erro.to_string())))
        }
        ErroSolicitacao::VisitaInvalida
        | ErroSolicitacao::MotivoObrigatorio
        | ErroSolicitacao::JustificativaObrigatoria
        | ErroSolicitacao::DataRetroativa
        | ErroSolicitacao::DadosInvalidos(_) => (
            StatusCode::BAD_REQUEST,
            Json(ErroApi::nova(erro.to_string())),
        ),
        ErroSolicitacao::Tecnica(mensagem) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErroApi::nova(mensagem)),
        ),
    }
}
