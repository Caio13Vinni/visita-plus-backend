// =============================================================================
// Sincronizacao offline/online — Interface HTTP
// POST /api/v1/sincronizacao      -> envia pendencias locais e recebe a agenda
//                                    (reconciliacao do SQLite server-side)
// POST /api/v1/sincronizacao/push -> aplica pendencias enviadas pelo app mobile
// GET  /api/v1/sincronizacao/pull -> devolve a agenda do perfil para o app
// =============================================================================

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::casos_de_uso::sincronizacao::{
    AgendaSincronizacao, PendenciaRecebida, ResumoPush, ResumoSincronizacao,
};
use crate::dominio::erros::ErroSincronizacao;
use crate::interface::api::extratores::UsuarioAutenticado;
use crate::interface::api::familias::RespostaFamilia;
use crate::interface::api::modelos::ErroApi;
use crate::interface::api::visitas::RespostaVisita;
use crate::interface::estado::EstadoAplicacao;

/// Constroi o sub-roteador do modulo de sincronizacao.
pub fn construir_rotas() -> Router<Arc<EstadoAplicacao>> {
    Router::new()
        .route("/", post(sincronizar))
        .route("/push", post(push))
        .route("/pull", get(pull))
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

/// Pendencia do push HTTP (app mobile).
#[derive(Debug, Deserialize)]
pub struct RequisicaoPendencia {
    pub tipo: String,
    pub payload: serde_json::Value,
}

/// Corpo do push HTTP.
#[derive(Debug, Deserialize)]
pub struct RequisicaoPush {
    pub pendencias: Vec<RequisicaoPendencia>,
}

/// Resultado individual de uma pendencia no push.
#[derive(Debug, Serialize)]
pub struct RespostaItemPush {
    pub tipo: String,
    pub sucesso: bool,
    pub erro: Option<String>,
}

/// Resposta do push HTTP.
#[derive(Debug, Serialize)]
pub struct RespostaPush {
    pub enviados: u64,
    pub erros: u64,
    pub itens: Vec<RespostaItemPush>,
}

impl From<&ResumoPush> for RespostaPush {
    fn from(resumo: &ResumoPush) -> Self {
        Self {
            enviados: resumo.enviados,
            erros: resumo.erros,
            itens: resumo
                .itens
                .iter()
                .map(|item| RespostaItemPush {
                    tipo: item.tipo.clone(),
                    sucesso: item.sucesso,
                    erro: item.erro.clone(),
                })
                .collect(),
        }
    }
}

/// Agenda do perfil devolvida no pull.
#[derive(Debug, Serialize)]
pub struct RespostaPull {
    pub visitas: Vec<RespostaVisita>,
    pub familias: Vec<RespostaFamilia>,
}

impl From<&AgendaSincronizacao> for RespostaPull {
    fn from(agenda: &AgendaSincronizacao) -> Self {
        Self {
            visitas: agenda.visitas.iter().map(RespostaVisita::from).collect(),
            familias: agenda.familias.iter().map(RespostaFamilia::from).collect(),
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

/// POST /sincronizacao/push — aplica as pendencias enviadas pelo app
/// mobile (registros de visita e encaminhamentos feitos offline). Itens
/// invalidos retornam erro individual sem abortar o lote.
async fn push(
    State(estado): State<Arc<EstadoAplicacao>>,
    UsuarioAutenticado(claims): UsuarioAutenticado,
    Json(corpo): Json<RequisicaoPush>,
) -> Result<Json<RespostaPush>, (StatusCode, Json<ErroApi>)> {
    let pendencias = corpo
        .pendencias
        .into_iter()
        .map(|pendencia| PendenciaRecebida {
            tipo: pendencia.tipo,
            payload: pendencia.payload,
        })
        .collect::<Vec<_>>();

    let resumo = estado
        .sincronizacao
        .receber_pendencias(claims.sub, &pendencias)
        .await
        .map_err(mapear_erro_sincronizacao)?;

    Ok(Json(RespostaPush::from(&resumo)))
}

/// GET /sincronizacao/pull — devolve a agenda do perfil (tecnico: propria
/// agenda; coordenador: da unidade — RN05/RN06) com as familias
/// correspondentes, para o app mobile gravar no banco local.
async fn pull(
    State(estado): State<Arc<EstadoAplicacao>>,
    UsuarioAutenticado(claims): UsuarioAutenticado,
) -> Result<Json<RespostaPull>, (StatusCode, Json<ErroApi>)> {
    let agenda = estado
        .sincronizacao
        .baixar_agenda(claims.sub)
        .await
        .map_err(mapear_erro_sincronizacao)?;

    Ok(Json(RespostaPull::from(&agenda)))
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
