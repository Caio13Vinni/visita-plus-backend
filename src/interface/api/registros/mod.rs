// =============================================================================
// UC05 — Interface HTTP do registro da visita
// POST   /api/v1/registros                     -> iniciar visita (rascunho)
// GET    /api/v1/registros/visita/:id_visita   -> detalhar registro da visita
// PATCH  /api/v1/registros/:id                 -> salvar rascunho (RN12)
// POST   /api/v1/registros/:id/finalizar       -> finalizar (RN13/RN14)
// POST   /api/v1/registros/:id/complementos    -> complemento (RN16/RN17)
// GET    /api/v1/registros/:id/complementos    -> listar complementos
// =============================================================================

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::dominio::entidades::registro_visita::{
    ComplementoRegistro, DadosRegistro, NovoComplemento, RegistroVisita,
};
use crate::dominio::enums::{ResultadoRegistro, StatusRegistro};
use crate::dominio::erros::ErroRegistro;
use crate::interface::api::extratores::UsuarioAutenticado;
use crate::interface::api::modelos::ErroApi;
use crate::interface::estado::EstadoAplicacao;

/// Constroi o sub-roteador do modulo de registros.
pub fn construir_rotas() -> Router<Arc<EstadoAplicacao>> {
    Router::new()
        .route("/", post(iniciar_visita))
        .route("/visita/{id_visita}", get(detalhar_por_visita))
        .route("/{id}", patch(salvar_rascunho))
        .route("/{id}/finalizar", post(finalizar_registro))
        .route(
            "/{id}/complementos",
            get(listar_complementos).post(adicionar_complemento),
        )
}

// -----------------------------------------------------------------------------
// DTOs
// -----------------------------------------------------------------------------

/// Corpo da requisicao de iniciar a visita.
#[derive(Debug, Deserialize)]
pub struct RequisicaoIniciar {
    pub id_visita: Uuid,
}

/// Corpo da requisicao de salvar rascunho / finalizar.
#[derive(Debug, Deserialize)]
pub struct RequisicaoRegistro {
    pub resultado: Option<ResultadoRegistro>,
    pub descricao: Option<String>,
    pub desfecho: Option<String>,
    #[serde(default)]
    pub necessita_retorno: bool,
    pub motivo_nao_realizada: Option<String>,
}

/// Corpo da requisicao de complemento (RN17).
#[derive(Debug, Deserialize)]
pub struct RequisicaoComplemento {
    pub descricao: String,
    pub motivo: String,
}

/// Dados publicos do registro na resposta da API.
#[derive(Debug, Serialize)]
pub struct RespostaRegistro {
    pub id: Uuid,
    pub id_visita: Uuid,
    pub status: StatusRegistro,
    pub resultado: Option<ResultadoRegistro>,
    pub descricao: Option<String>,
    pub desfecho: Option<String>,
    pub necessita_retorno: bool,
    pub motivo_nao_realizada: Option<String>,
    pub data_hora_inicio: Option<chrono::DateTime<chrono::Utc>>,
    pub data_hora_fim: Option<chrono::DateTime<chrono::Utc>>,
    pub registrado_por: Uuid,
}

impl From<&RegistroVisita> for RespostaRegistro {
    fn from(registro: &RegistroVisita) -> Self {
        Self {
            id: registro.id,
            id_visita: registro.id_visita,
            status: registro.status,
            resultado: registro.resultado,
            descricao: registro.descricao.clone(),
            desfecho: registro.desfecho.clone(),
            necessita_retorno: registro.necessita_retorno,
            motivo_nao_realizada: registro.motivo_nao_realizada.clone(),
            data_hora_inicio: registro.data_hora_inicio,
            data_hora_fim: registro.data_hora_fim,
            registrado_por: registro.registrado_por,
        }
    }
}

/// Dados publicos do complemento na resposta da API.
#[derive(Debug, Serialize)]
pub struct RespostaComplemento {
    pub id: Uuid,
    pub id_registro: Uuid,
    pub descricao: String,
    pub motivo: String,
    pub registrado_por: Uuid,
}

impl From<&ComplementoRegistro> for RespostaComplemento {
    fn from(complemento: &ComplementoRegistro) -> Self {
        Self {
            id: complemento.id,
            id_registro: complemento.id_registro,
            descricao: complemento.descricao.clone(),
            motivo: complemento.motivo.clone(),
            registrado_por: complemento.registrado_por,
        }
    }
}

// -----------------------------------------------------------------------------
// Manipuladores
// -----------------------------------------------------------------------------

/// POST /registros — inicia a visita e cria o registro em rascunho (RN12).
async fn iniciar_visita(
    State(estado): State<Arc<EstadoAplicacao>>,
    UsuarioAutenticado(claims): UsuarioAutenticado,
    Json(corpo): Json<RequisicaoIniciar>,
) -> Result<(StatusCode, Json<RespostaRegistro>), (StatusCode, Json<ErroApi>)> {
    let registro = estado
        .registro
        .iniciar_visita(claims.sub, corpo.id_visita)
        .await
        .map_err(mapear_erro_registro)?;

    Ok((StatusCode::CREATED, Json(RespostaRegistro::from(&registro))))
}

/// GET /registros/visita/{id_visita} — detalha o registro da visita.
async fn detalhar_por_visita(
    State(estado): State<Arc<EstadoAplicacao>>,
    UsuarioAutenticado(claims): UsuarioAutenticado,
    Path(id_visita): Path<Uuid>,
) -> Result<Json<RespostaRegistro>, (StatusCode, Json<ErroApi>)> {
    let registro = estado
        .registro
        .detalhar_por_visita(claims.sub, id_visita)
        .await
        .map_err(mapear_erro_registro)?;

    Ok(Json(RespostaRegistro::from(&registro)))
}

/// PATCH /registros/{id} — salva o rascunho sem finalizar (RN12).
async fn salvar_rascunho(
    State(estado): State<Arc<EstadoAplicacao>>,
    UsuarioAutenticado(claims): UsuarioAutenticado,
    Path(id): Path<Uuid>,
    Json(corpo): Json<RequisicaoRegistro>,
) -> Result<Json<RespostaRegistro>, (StatusCode, Json<ErroApi>)> {
    let registro = estado
        .registro
        .salvar_rascunho(claims.sub, id, &para_dados(&corpo))
        .await
        .map_err(mapear_erro_registro)?;

    Ok(Json(RespostaRegistro::from(&registro)))
}

/// POST /registros/{id}/finalizar — finaliza o registro (RN13/RN14).
async fn finalizar_registro(
    State(estado): State<Arc<EstadoAplicacao>>,
    UsuarioAutenticado(claims): UsuarioAutenticado,
    Path(id): Path<Uuid>,
    Json(corpo): Json<RequisicaoRegistro>,
) -> Result<Json<RespostaRegistro>, (StatusCode, Json<ErroApi>)> {
    let registro = estado
        .registro
        .finalizar_registro(claims.sub, id, &para_dados(&corpo))
        .await
        .map_err(mapear_erro_registro)?;

    Ok(Json(RespostaRegistro::from(&registro)))
}

/// POST /registros/{id}/complementos — complemento imutavel (RN16/RN17).
async fn adicionar_complemento(
    State(estado): State<Arc<EstadoAplicacao>>,
    UsuarioAutenticado(claims): UsuarioAutenticado,
    Path(id): Path<Uuid>,
    Json(corpo): Json<RequisicaoComplemento>,
) -> Result<(StatusCode, Json<RespostaComplemento>), (StatusCode, Json<ErroApi>)> {
    let complemento = NovoComplemento {
        descricao: corpo.descricao,
        motivo: corpo.motivo,
    };

    let criado = estado
        .registro
        .adicionar_complemento(claims.sub, id, &complemento)
        .await
        .map_err(mapear_erro_registro)?;

    Ok((
        StatusCode::CREATED,
        Json(RespostaComplemento::from(&criado)),
    ))
}

/// GET /registros/{id}/complementos — lista o historico de complementos.
async fn listar_complementos(
    State(estado): State<Arc<EstadoAplicacao>>,
    UsuarioAutenticado(claims): UsuarioAutenticado,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<RespostaComplemento>>, (StatusCode, Json<ErroApi>)> {
    let complementos = estado
        .registro
        .listar_complementos(claims.sub, id)
        .await
        .map_err(mapear_erro_registro)?;

    Ok(Json(
        complementos
            .iter()
            .map(RespostaComplemento::from)
            .collect::<Vec<_>>(),
    ))
}

// -----------------------------------------------------------------------------
// Conversao de erros
// -----------------------------------------------------------------------------

fn para_dados(corpo: &RequisicaoRegistro) -> DadosRegistro {
    DadosRegistro {
        resultado: corpo.resultado,
        descricao: corpo.descricao.clone(),
        desfecho: corpo.desfecho.clone(),
        necessita_retorno: corpo.necessita_retorno,
        motivo_nao_realizada: corpo.motivo_nao_realizada.clone(),
    }
}

/// Converte erros de registro em respostas HTTP.
fn mapear_erro_registro(erro: ErroRegistro) -> (StatusCode, Json<ErroApi>) {
    match erro {
        ErroRegistro::SemPermissao => {
            (StatusCode::FORBIDDEN, Json(ErroApi::nova(erro.to_string())))
        }
        ErroRegistro::NaoEncontrado => {
            (StatusCode::NOT_FOUND, Json(ErroApi::nova(erro.to_string())))
        }
        ErroRegistro::VisitaNaoAgendada
        | ErroRegistro::VisitaNaoEmAndamento
        | ErroRegistro::RegistroNaoRascunho => {
            (StatusCode::CONFLICT, Json(ErroApi::nova(erro.to_string())))
        }
        ErroRegistro::RegistroNaoFinalizado | ErroRegistro::DadosInvalidos(_) => (
            StatusCode::BAD_REQUEST,
            Json(ErroApi::nova(erro.to_string())),
        ),
        ErroRegistro::Tecnica(mensagem) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErroApi::nova(mensagem)),
        ),
    }
}
