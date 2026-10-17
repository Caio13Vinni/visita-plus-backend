// =============================================================================
// UC03 — Interface HTTP de gerenciamento de familias
// GET    /api/v1/familias               -> listar
// POST   /api/v1/familias               -> cadastrar (Coordenador)
// PATCH  /api/v1/familias/:id           -> editar (Coordenador)
// PATCH  /api/v1/familias/:id/status    -> ativar/inativar (Coordenador)
// =============================================================================

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, patch};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::dominio::entidades::familia::{CadastroFamilia, Familia};
use crate::dominio::enums::StatusFamilia;
use crate::dominio::erros::ErroCadastro;
use crate::interface::api::extratores::UsuarioAutenticado;
use crate::interface::api::modelos::ErroApi;
use crate::interface::estado::EstadoAplicacao;

/// Constroi o sub-roteador do modulo de familias.
pub fn construir_rotas() -> Router<Arc<EstadoAplicacao>> {
    Router::new()
        .route("/", get(listar_familias).post(cadastrar_familia))
        .route("/{id}", patch(editar_familia))
        .route("/{id}/status", patch(alterar_status_familia))
}

// -----------------------------------------------------------------------------
// DTOs
// -----------------------------------------------------------------------------

/// Filtros opcionais da listagem de familias.
#[derive(Debug, Deserialize)]
pub struct FiltroFamilias {
    pub id_unidade: Option<Uuid>,
    pub status: Option<StatusFamilia>,
}

/// Corpo da requisicao de cadastro/edicao de familia.
#[derive(Debug, Deserialize)]
pub struct RequisicaoFamilia {
    pub codigo: String,
    pub nome_referencia: String,
    pub contato: String,
    pub endereco: String,
}

/// Corpo da requisicao de mudanca de status.
#[derive(Debug, Deserialize)]
pub struct RequisicaoStatus {
    pub status: StatusFamilia,
}

/// Dados publicos de familia na resposta da API.
#[derive(Debug, Serialize)]
pub struct RespostaFamilia {
    pub id: Uuid,
    pub id_unidade: Uuid,
    pub codigo: String,
    pub nome_referencia: String,
    pub contato: String,
    pub endereco: String,
    pub status: StatusFamilia,
}

impl From<&Familia> for RespostaFamilia {
    fn from(familia: &Familia) -> Self {
        Self {
            id: familia.id,
            id_unidade: familia.id_unidade,
            codigo: familia.codigo.clone(),
            nome_referencia: familia.nome_referencia.clone(),
            contato: familia.contato.clone(),
            endereco: familia.endereco.clone(),
            status: familia.status,
        }
    }
}

// -----------------------------------------------------------------------------
// Manipuladores
// -----------------------------------------------------------------------------

/// GET /familias — lista as familias (coordenador e tecnico).
async fn listar_familias(
    State(estado): State<Arc<EstadoAplicacao>>,
    UsuarioAutenticado(claims): UsuarioAutenticado,
    Query(filtro): Query<FiltroFamilias>,
) -> Result<Json<Vec<RespostaFamilia>>, (StatusCode, Json<ErroApi>)> {
    let familias = estado
        .familias
        .listar_familias(claims.sub, filtro.id_unidade, filtro.status)
        .await
        .map_err(mapear_erro_cadastro)?;

    Ok(Json(
        familias
            .iter()
            .map(RespostaFamilia::from)
            .collect::<Vec<_>>(),
    ))
}

/// POST /familias — cadastra uma nova familia ficticia (Coordenador).
async fn cadastrar_familia(
    State(estado): State<Arc<EstadoAplicacao>>,
    UsuarioAutenticado(claims): UsuarioAutenticado,
    Query(filtro): Query<FiltroFamilias>,
    Json(corpo): Json<RequisicaoFamilia>,
) -> Result<(StatusCode, Json<RespostaFamilia>), (StatusCode, Json<ErroApi>)> {
    let id_unidade = filtro.id_unidade.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErroApi::nova("informe o id_unidade da familia")),
        )
    })?;

    let cadastro = CadastroFamilia {
        codigo: corpo.codigo,
        nome_referencia: corpo.nome_referencia,
        contato: corpo.contato,
        endereco: corpo.endereco,
    };

    let criada = estado
        .familias
        .cadastrar_familia(claims.sub, id_unidade, &cadastro)
        .await
        .map_err(mapear_erro_cadastro)?;

    Ok((StatusCode::CREATED, Json(RespostaFamilia::from(&criada))))
}

/// PATCH /familias/{id} — edita os dados cadastrais (Coordenador).
async fn editar_familia(
    State(estado): State<Arc<EstadoAplicacao>>,
    UsuarioAutenticado(claims): UsuarioAutenticado,
    Path(id): Path<Uuid>,
    Json(corpo): Json<RequisicaoFamilia>,
) -> Result<Json<RespostaFamilia>, (StatusCode, Json<ErroApi>)> {
    let cadastro = CadastroFamilia {
        codigo: corpo.codigo,
        nome_referencia: corpo.nome_referencia,
        contato: corpo.contato,
        endereco: corpo.endereco,
    };

    let atualizada = estado
        .familias
        .editar_familia(claims.sub, id, &cadastro)
        .await
        .map_err(mapear_erro_cadastro)?;

    Ok(Json(RespostaFamilia::from(&atualizada)))
}

/// PATCH /familias/{id}/status — ativa/inativa a familia (Coordenador).
async fn alterar_status_familia(
    State(estado): State<Arc<EstadoAplicacao>>,
    UsuarioAutenticado(claims): UsuarioAutenticado,
    Path(id): Path<Uuid>,
    Json(corpo): Json<RequisicaoStatus>,
) -> Result<Json<RespostaFamilia>, (StatusCode, Json<ErroApi>)> {
    let atualizada = estado
        .familias
        .alterar_status_familia(claims.sub, id, corpo.status)
        .await
        .map_err(mapear_erro_cadastro)?;

    Ok(Json(RespostaFamilia::from(&atualizada)))
}

// -----------------------------------------------------------------------------
// Conversao de erros
// -----------------------------------------------------------------------------

/// Converte erros de cadastro em respostas HTTP.
fn mapear_erro_cadastro(erro: ErroCadastro) -> (StatusCode, Json<ErroApi>) {
    match erro {
        ErroCadastro::SemPermissao => {
            (StatusCode::FORBIDDEN, Json(ErroApi::nova(erro.to_string())))
        }
        ErroCadastro::EmailJaExistente | ErroCadastro::CodigoJaExistente => {
            (StatusCode::CONFLICT, Json(ErroApi::nova(erro.to_string())))
        }
        ErroCadastro::TecnicoComVisitasFuturas => {
            (StatusCode::CONFLICT, Json(ErroApi::nova(erro.to_string())))
        }
        ErroCadastro::NaoEncontrado => {
            (StatusCode::NOT_FOUND, Json(ErroApi::nova(erro.to_string())))
        }
        ErroCadastro::DadosInvalidos(mensagem) => {
            (StatusCode::BAD_REQUEST, Json(ErroApi::nova(mensagem)))
        }
        ErroCadastro::Tecnica(mensagem) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErroApi::nova(mensagem)),
        ),
    }
}
