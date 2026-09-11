// =============================================================================
// Interface HTTP — construcao do roteador da API
// =============================================================================

use axum::routing::get;
use axum::Router;
use std::sync::Arc;

use super::estado::EstadoAplicacao;
use crate::interface::api::manipuladores::verificar_saude;

pub mod autenticacao;
pub mod manipuladores;
pub mod modelos;
pub mod rotas;

/// Constroi o roteador raiz da API com todos os modulos registrados.
pub fn construir_roteador(estado: Arc<EstadoAplicacao>) -> Router {
    Router::new()
        .route("/saude", get(verificar_saude))
        .nest("/api/v1", rotas::construir_rotas())
        .with_state(estado)
}