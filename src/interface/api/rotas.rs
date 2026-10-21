// Registro dos sub-roteadores da API (cada caso de uso agrega suas rotas).

use axum::Router;
use std::sync::Arc;

use crate::interface::api::{
    auditoria, autenticacao, encaminhamentos, familias, historico, indicadores, notificacoes,
    profissionais, registros, sincronizacao, solicitacoes, visitas,
};
use crate::interface::estado::EstadoAplicacao;

/// Constroi o conjunto de rotas das funcionalidades da aplicacao.
pub fn construir_rotas() -> Router<Arc<EstadoAplicacao>> {
    Router::new()
        .nest("/auth", autenticacao::construir_rotas())
        .nest("/profissionais", profissionais::construir_rotas())
        .nest("/familias", familias::construir_rotas())
        .nest("/visitas", visitas::construir_rotas())
        .nest("/registros", registros::construir_rotas())
        .nest("/solicitacoes-retorno", solicitacoes::construir_rotas())
        .nest("/notificacoes", notificacoes::construir_rotas())
        .nest("/indicadores", indicadores::construir_rotas())
        .nest("/auditoria", auditoria::construir_rotas())
        .nest("/historico", historico::construir_rotas())
        .nest("/sincronizacao", sincronizacao::construir_rotas())
        .nest("/encaminhamentos", encaminhamentos::construir_rotas())
}
