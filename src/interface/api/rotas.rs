// Registro dos sub-roteadores da API (cada caso de uso agrega suas rotas).

use axum::Router;
use std::sync::Arc;

use crate::interface::estado::EstadoAplicacao;

/// Constroi o conjunto de rotas das funcionalidades da aplicacao.
pub fn construir_rotas() -> Router<Arc<EstadoAplicacao>> {
    // As rotas de cada caso de uso (UC01 a UC12) serao registradas
    // progressivamente nas fases seguintes.
    Router::new()
}