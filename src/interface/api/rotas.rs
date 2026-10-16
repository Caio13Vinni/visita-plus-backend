// Registro dos sub-roteadores da API (cada caso de uso agrega suas rotas).

use axum::Router;
use std::sync::Arc;

use crate::interface::api::{autenticacao, familias, profissionais};
use crate::interface::estado::EstadoAplicacao;

/// Constroi o conjunto de rotas das funcionalidades da aplicacao.
pub fn construir_rotas() -> Router<Arc<EstadoAplicacao>> {
    Router::new()
        .nest("/auth", autenticacao::construir_rotas())
        .nest("/profissionais", profissionais::construir_rotas())
        .nest("/familias", familias::construir_rotas())
}
