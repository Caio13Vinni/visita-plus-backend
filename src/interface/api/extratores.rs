// =============================================================================
// Extratores de autenticacao — claims do token JWT (Bearer)
// =============================================================================

use axum::Json;
use axum::extract::FromRequestParts;
use axum::http::StatusCode;
use axum::http::request::Parts;
use std::sync::Arc;

use crate::infraestrutura::seguranca::jwt::Claims;
use crate::interface::api::modelos::ErroApi;
use crate::interface::estado::EstadoAplicacao;

/// Extrai e valida os claims do token JWT enviado no cabecalho Authorization.
pub struct UsuarioAutenticado(pub Claims);

impl FromRequestParts<Arc<EstadoAplicacao>> for UsuarioAutenticado {
    type Rejection = (StatusCode, Json<ErroApi>);

    async fn from_request_parts(
        partes: &mut Parts,
        estado: &Arc<EstadoAplicacao>,
    ) -> Result<Self, Self::Rejection> {
        let autorizacao = partes
            .headers
            .get("authorization")
            .and_then(|valor| valor.to_str().ok())
            .ok_or_else(|| {
                (
                    StatusCode::UNAUTHORIZED,
                    Json(ErroApi::nova("cabecalho de autorizacao ausente")),
                )
            })?;

        let token = autorizacao.strip_prefix("Bearer ").ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(ErroApi::nova("token de autorizacao invalido")),
            )
        })?;

        let claims = estado.servico_jwt.validar(token).map_err(|_| {
            (
                StatusCode::UNAUTHORIZED,
                Json(ErroApi::nova("token invalido ou expirado")),
            )
        })?;

        Ok(UsuarioAutenticado(claims))
    }
}
