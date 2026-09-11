// Registro dos sub-roteadores da API (cada caso de uso agrega suas rotas).

use axum::Router;
use std::sync::Arc;

use crate::interface::api::autenticacao;
use crate::interface::estado::EstadoAplicacao;

/// Constroi o conjunto de rotas das funcionalidades da aplicacao.
pub fn construir_rotas() -> Router<Arc<EstadoAplicacao>> {
    Router::new().nest("/auth", autenticacao::construir_rotas())
}