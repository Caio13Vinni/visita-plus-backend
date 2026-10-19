// =============================================================================
// UC11 — Interface HTTP de indicadores operacionais
// GET    /api/v1/indicadores?data_inicio=AAAA-MM-DD&data_fim=AAAA-MM-DD
// =============================================================================

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use chrono::{Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::dominio::entidades::indicadores::IndicadoresOperacionais;
use crate::dominio::erros::ErroIndicadores;
use crate::interface::api::extratores::UsuarioAutenticado;
use crate::interface::api::modelos::ErroApi;
use crate::interface::estado::EstadoAplicacao;

/// Constroi o sub-roteador do modulo de indicadores.
pub fn construir_rotas() -> Router<Arc<EstadoAplicacao>> {
    Router::new().route("/", get(consultar_indicadores))
}

// -----------------------------------------------------------------------------
// DTOs
// -----------------------------------------------------------------------------

/// Periodo da consulta. Sem parametros, considera os ultimos 30 dias.
#[derive(Debug, Deserialize)]
pub struct FiltroIndicadoresQuery {
    pub data_inicio: Option<NaiveDate>,
    pub data_fim: Option<NaiveDate>,
}

/// Indicadores na resposta da API: apenas quantidades (RN23).
#[derive(Debug, Serialize)]
pub struct RespostaIndicadores {
    pub periodo_inicio: NaiveDate,
    pub periodo_fim: NaiveDate,
    pub visitas_agendadas: i64,
    pub visitas_em_andamento: i64,
    pub visitas_realizadas: i64,
    pub visitas_nao_realizadas: i64,
    pub visitas_canceladas: i64,
    pub total_visitas: i64,
    pub retornos_pendentes: i64,
    /// `true` quando o periodo nao possui dados (mensagem explicativa).
    pub periodo_sem_dados: bool,
}

impl From<&IndicadoresOperacionais> for RespostaIndicadores {
    fn from(indicadores: &IndicadoresOperacionais) -> Self {
        Self {
            periodo_inicio: indicadores.periodo_inicio,
            periodo_fim: indicadores.periodo_fim,
            visitas_agendadas: indicadores.visitas_agendadas,
            visitas_em_andamento: indicadores.visitas_em_andamento,
            visitas_realizadas: indicadores.visitas_realizadas,
            visitas_nao_realizadas: indicadores.visitas_nao_realizadas,
            visitas_canceladas: indicadores.visitas_canceladas,
            total_visitas: indicadores.total_visitas(),
            retornos_pendentes: indicadores.retornos_pendentes,
            periodo_sem_dados: indicadores.periodo_sem_dados(),
        }
    }
}

// -----------------------------------------------------------------------------
// Manipuladores
// -----------------------------------------------------------------------------

/// GET /indicadores — quantidades operacionais da unidade no periodo (UC11).
async fn consultar_indicadores(
    State(estado): State<Arc<EstadoAplicacao>>,
    UsuarioAutenticado(claims): UsuarioAutenticado,
    Query(filtro): Query<FiltroIndicadoresQuery>,
) -> Result<Json<RespostaIndicadores>, (StatusCode, Json<ErroApi>)> {
    let hoje = Utc::now().date_naive();
    let data_inicio = filtro.data_inicio.unwrap_or(hoje - Duration::days(30));
    let data_fim = filtro.data_fim.unwrap_or(hoje);

    let indicadores = estado
        .indicadores
        .consultar_indicadores(claims.sub, data_inicio, data_fim)
        .await
        .map_err(mapear_erro_indicadores)?;

    Ok(Json(RespostaIndicadores::from(&indicadores)))
}

// -----------------------------------------------------------------------------
// Conversao de erros
// -----------------------------------------------------------------------------

/// Converte erros de indicadores em respostas HTTP.
fn mapear_erro_indicadores(erro: ErroIndicadores) -> (StatusCode, Json<ErroApi>) {
    match erro {
        ErroIndicadores::SemPermissao => {
            (StatusCode::FORBIDDEN, Json(ErroApi::nova(erro.to_string())))
        }
        ErroIndicadores::DadosInvalidos(_) => (
            StatusCode::BAD_REQUEST,
            Json(ErroApi::nova(erro.to_string())),
        ),
        ErroIndicadores::Tecnica(mensagem) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErroApi::nova(mensagem)),
        ),
    }
}
