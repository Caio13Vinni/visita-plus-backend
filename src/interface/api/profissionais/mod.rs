// =============================================================================
// UC02 — Interface HTTP de gerenciamento de profissionais
// GET    /api/v1/profissionais           -> listar (Coordenador)
// POST   /api/v1/profissionais           -> cadastrar (Coordenador)
// PATCH  /api/v1/profissionais/:id       -> editar (Coordenador)
// PATCH  /api/v1/profissionais/:id/status -> ativar/desativar (Coordenador)
// =============================================================================

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, patch};
use axum::{Json, Router};
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;

use crate::casos_de_uso::profissionais::{CadastroProfissional, EdicaoProfissional};
use crate::dominio::enums::{PerfilUsuario, StatusUsuario};
use crate::dominio::erros::ErroCadastro;
use crate::interface::api::autenticacao::RespostaUsuario;
use crate::interface::api::extratores::UsuarioAutenticado;
use crate::interface::api::modelos::ErroApi;
use crate::interface::estado::EstadoAplicacao;

/// Constroi o sub-roteador do modulo de profissionais.
pub fn construir_rotas() -> Router<Arc<EstadoAplicacao>> {
    Router::new()
        .route("/", get(listar_profissionais).post(cadastrar_profissional))
        .route("/{id}", patch(editar_profissional))
        .route("/{id}/status", patch(alterar_status_profissional))
}

// -----------------------------------------------------------------------------
// DTOs
// -----------------------------------------------------------------------------

/// Filtro opcional por unidade na listagem.
#[derive(Debug, Deserialize)]
pub struct FiltroUnidade {
    pub id_unidade: Option<Uuid>,
}

/// Corpo da requisicao de cadastro de profissional.
#[derive(Debug, Deserialize)]
pub struct RequisicaoCadastroProfissional {
    pub id_unidade: Uuid,
    pub nome: String,
    pub email: String,
    pub senha: String,
    pub perfil: PerfilUsuario,
}

/// Corpo da requisicao de edicao de profissional.
#[derive(Debug, Deserialize)]
pub struct RequisicaoEdicaoProfissional {
    pub nome: String,
    pub perfil: PerfilUsuario,
}

/// Corpo da requisicao de mudanca de status.
#[derive(Debug, Deserialize)]
pub struct RequisicaoStatus {
    pub status: StatusUsuario,
}

// -----------------------------------------------------------------------------
// Manipuladores
// -----------------------------------------------------------------------------

/// GET /profissionais — lista os profissionais (somente Coordenador).
async fn listar_profissionais(
    State(estado): State<Arc<EstadoAplicacao>>,
    UsuarioAutenticado(claims): UsuarioAutenticado,
    Query(filtro): Query<FiltroUnidade>,
) -> Result<Json<Vec<RespostaUsuario>>, (StatusCode, Json<ErroApi>)> {
    let profissionais = estado
        .profissionais
        .listar_profissionais(claims.sub, filtro.id_unidade)
        .await
        .map_err(mapear_erro_cadastro)?;

    Ok(Json(
        profissionais
            .iter()
            .map(RespostaUsuario::from)
            .collect::<Vec<_>>(),
    ))
}

/// POST /profissionais — cadastra um novo profissional.
async fn cadastrar_profissional(
    State(estado): State<Arc<EstadoAplicacao>>,
    UsuarioAutenticado(claims): UsuarioAutenticado,
    Json(corpo): Json<RequisicaoCadastroProfissional>,
) -> Result<(StatusCode, Json<RespostaUsuario>), (StatusCode, Json<ErroApi>)> {
    let cadastro = CadastroProfissional {
        id_unidade: corpo.id_unidade,
        nome: corpo.nome,
        email: corpo.email,
        senha: corpo.senha,
        perfil: corpo.perfil,
    };

    let criado = estado
        .profissionais
        .cadastrar_profissional(claims.sub, cadastro)
        .await
        .map_err(mapear_erro_cadastro)?;

    Ok((StatusCode::CREATED, Json(RespostaUsuario::from(&criado))))
}

/// PATCH /profissionais/{id} — edita nome e perfil.
async fn editar_profissional(
    State(estado): State<Arc<EstadoAplicacao>>,
    UsuarioAutenticado(claims): UsuarioAutenticado,
    Path(id): Path<Uuid>,
    Json(corpo): Json<RequisicaoEdicaoProfissional>,
) -> Result<Json<RespostaUsuario>, (StatusCode, Json<ErroApi>)> {
    let atualizado = estado
        .profissionais
        .editar_profissional(
            claims.sub,
            id,
            EdicaoProfissional {
                nome: corpo.nome,
                perfil: corpo.perfil,
            },
        )
        .await
        .map_err(mapear_erro_cadastro)?;

    Ok(Json(RespostaUsuario::from(&atualizado)))
}

/// PATCH /profissionais/{id}/status — ativa/desativa a conta.
async fn alterar_status_profissional(
    State(estado): State<Arc<EstadoAplicacao>>,
    UsuarioAutenticado(claims): UsuarioAutenticado,
    Path(id): Path<Uuid>,
    Json(corpo): Json<RequisicaoStatus>,
) -> Result<Json<RespostaUsuario>, (StatusCode, Json<ErroApi>)> {
    let atualizado = estado
        .profissionais
        .alterar_status_profissional(claims.sub, id, corpo.status)
        .await
        .map_err(mapear_erro_cadastro)?;

    Ok(Json(RespostaUsuario::from(&atualizado)))
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
