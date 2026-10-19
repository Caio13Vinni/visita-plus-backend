// =============================================================================
// Interface HTTP — construcao do roteador da API
// =============================================================================

use axum::Router;
use axum::routing::get;
use std::sync::Arc;

use super::estado::EstadoAplicacao;
use crate::interface::api::manipuladores::verificar_saude;

pub mod auditoria;
pub mod autenticacao;
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
pub mod solicitacoes;
pub mod visitas;

/// Constroi o roteador raiz da API com todos os modulos registrados.
pub fn construir_roteador(estado: Arc<EstadoAplicacao>) -> Router {
    Router::new()
        .route("/saude", get(verificar_saude))
        .nest("/api/v1", rotas::construir_rotas())
        .with_state(estado)
}
