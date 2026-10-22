// =============================================================================
// Interface HTTP — construcao do roteador da API
// =============================================================================

use axum::Router;
use axum::routing::get;
use std::sync::Arc;
use tower_http::cors::CorsLayer;

use super::estado::EstadoAplicacao;
use crate::interface::api::manipuladores::verificar_saude;

pub mod auditoria;
pub mod autenticacao;
pub mod encaminhamentos;
pub mod extratores;
pub mod familias;
pub mod historico;
pub mod indicadores;
pub mod manipuladores;
pub mod modelos;
pub mod notificacoes;
pub mod profissionais;
pub mod registros;
pub mod rotas;
pub mod sincronizacao;
pub mod solicitacoes;
pub mod visitas;

/// Constroi o roteador raiz da API com todos os modulos registrados.
pub fn construir_roteador(estado: Arc<EstadoAplicacao>) -> Router {
    // CORS permissivo para o front integrar em desenvolvimento: a
    // autenticacao usa Bearer token (nao cookie), logo o risco de CSRF
    // e inexistente e a origem pode ser liberada sem credenciais de sessao.
    Router::new()
        .route("/saude", get(verificar_saude))
        .nest("/api/v1", rotas::construir_rotas())
        .layer(CorsLayer::permissive())
        .with_state(estado)
}
