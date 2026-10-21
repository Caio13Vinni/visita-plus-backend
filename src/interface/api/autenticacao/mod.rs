// =============================================================================
// UC01 — Interface HTTP de autenticacao
// POST /api/v1/auth/login                        -> autenticar usuario
// POST /api/v1/auth/trocar-senha                 -> troca de senha (primeiro acesso)
// POST /api/v1/auth/recuperar-senha              -> solicita codigo (RF03, anonimo)
// POST /api/v1/auth/recuperar-senha/redefinir    -> redefine a senha (RF03, anonimo)
// =============================================================================

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::casos_de_uso::autenticacao::Credenciais;
use crate::dominio::entidades::usuario::Usuario;
use crate::dominio::enums::{PerfilUsuario, StatusUsuario};
use crate::dominio::erros::{ErroAutenticacao, ErroRecuperacao};
use crate::infraestrutura::seguranca::jwt::{Claims, ServicoJwt};
use crate::interface::api::modelos::ErroApi;
use crate::interface::estado::EstadoAplicacao;

/// Constroi o sub-roteador do modulo de autenticacao.
pub fn construir_rotas() -> Router<Arc<EstadoAplicacao>> {
    Router::new()
        .route("/login", post(login))
        .route("/trocar-senha", post(trocar_senha))
        .route("/recuperar-senha", post(solicitar_recuperacao))
        .route("/recuperar-senha/redefinir", post(redefinir_senha))
}

// -----------------------------------------------------------------------------
// DTOs de entrada e saida
// -----------------------------------------------------------------------------

/// Corpo da requisicao de login.
#[derive(Debug, Deserialize)]
pub struct RequisicaoLogin {
    pub email: String,
    pub senha: String,
}

/// Corpo da requisicao de troca de senha.
#[derive(Debug, Deserialize)]
pub struct RequisicaoTrocaSenha {
    pub senha_atual: String,
    pub nova_senha: String,
}

/// Corpo da requisicao de solicitacao de recuperacao de senha (RF03).
#[derive(Debug, Deserialize)]
pub struct RequisicaoRecuperarSenha {
    pub email: String,
}

/// Corpo da requisicao de redefinicao de senha via codigo (RF03).
#[derive(Debug, Deserialize)]
pub struct RequisicaoRedefinirSenha {
    pub email: String,
    pub codigo: String,
    pub nova_senha: String,
}

/// Resposta publica generica da solicitacao de recuperacao (RF03).
/// Nao expoe o codigo nem revela a existencia da conta.
#[derive(Debug, Serialize)]
pub struct RespostaRecuperarSenha {
    pub mensagem: String,
}

/// Resposta do login bem-sucedido.
#[derive(Debug, Serialize)]
pub struct RespostaLogin {
    pub token: String,
    pub usuario: RespostaUsuario,
    pub exige_troca_senha: bool,
}

/// Dados publicos do usuario autenticado.
#[derive(Debug, Serialize)]
pub struct RespostaUsuario {
    pub id: Uuid,
    pub nome: String,
    pub email: String,
    pub perfil: PerfilUsuario,
    pub status: StatusUsuario,
}

impl From<&Usuario> for RespostaUsuario {
    fn from(usuario: &Usuario) -> Self {
        Self {
            id: usuario.id,
            nome: usuario.nome.clone(),
            email: usuario.email.clone(),
            perfil: usuario.perfil,
            status: usuario.status,
        }
    }
}

// -----------------------------------------------------------------------------
// Manipuladores
// -----------------------------------------------------------------------------

/// POST /auth/login — autentica o usuario e devolve o token JWT.
async fn login(
    State(estado): State<Arc<EstadoAplicacao>>,
    cabecalhos: HeaderMap,
    Json(corpo): Json<RequisicaoLogin>,
) -> Result<Json<RespostaLogin>, (StatusCode, Json<ErroApi>)> {
    let dispositivo = cabecalhos
        .get("user-agent")
        .and_then(|valor| valor.to_str().ok())
        .map(str::to_string);
    let ip = cabecalhos
        .get("x-forwarded-for")
        .and_then(|valor| valor.to_str().ok())
        .map(str::to_string);

    let resultado = estado
        .autenticacao
        .autenticar(
            Credenciais {
                email: corpo.email,
                senha: corpo.senha,
            },
            ip.as_deref(),
            dispositivo.as_deref(),
        )
        .await
        .map_err(mapear_erro_autenticacao)?;

    Ok(Json(RespostaLogin {
        token: resultado.token,
        usuario: RespostaUsuario::from(&resultado.usuario),
        exige_troca_senha: resultado.exige_troca_senha,
    }))
}

/// POST /auth/trocar-senha — altera a senha do usuario autenticado.
async fn trocar_senha(
    State(estado): State<Arc<EstadoAplicacao>>,
    cabecalhos: HeaderMap,
    Json(corpo): Json<RequisicaoTrocaSenha>,
) -> Result<StatusCode, (StatusCode, Json<ErroApi>)> {
    let claims = extrair_claims_do_bearer(&estado.servico_jwt, &cabecalhos)?;

    estado
        .autenticacao
        .trocar_senha(
            claims.sub,
            &corpo.senha_atual,
            &corpo.nova_senha,
            None,
            None,
        )
        .await
        .map_err(mapear_erro_autenticacao)?;

    Ok(StatusCode::NO_CONTENT)
}

/// POST /auth/recuperar-senha — solicita o codigo de recuperacao (RF03).
///
/// Fluxo anonimo: a resposta publica e generica e nao revela se a conta
/// existe. O prototipo academico nao possui envio de e-mail (SMTP); o
/// codigo gerado fica apenas no log do servidor.
async fn solicitar_recuperacao(
    State(estado): State<Arc<EstadoAplicacao>>,
    Json(corpo): Json<RequisicaoRecuperarSenha>,
) -> Result<Json<RespostaRecuperarSenha>, (StatusCode, Json<ErroApi>)> {
    let solicitacao = estado
        .recuperacao
        .solicitar(&corpo.email)
        .await
        .map_err(mapear_erro_recuperacao)?;

    if let Some(codigo) = &solicitacao.codigo {
        tracing::info!(
            email = %corpo.email,
            codigo = %codigo,
            "codigo de recuperacao de acesso gerado (prototipo sem envio de e-mail)"
        );
    }

    Ok(Json(RespostaRecuperarSenha {
        mensagem: solicitacao.mensagem.to_string(),
    }))
}

/// POST /auth/recuperar-senha/redefinir — redefine a senha com o codigo
/// informado (RF03). Fluxo anonimo.
async fn redefinir_senha(
    State(estado): State<Arc<EstadoAplicacao>>,
    Json(corpo): Json<RequisicaoRedefinirSenha>,
) -> Result<StatusCode, (StatusCode, Json<ErroApi>)> {
    estado
        .recuperacao
        .redefinir(&corpo.email, &corpo.codigo, &corpo.nova_senha)
        .await
        .map_err(mapear_erro_recuperacao)?;

    Ok(StatusCode::OK)
}

// -----------------------------------------------------------------------------
// Utilitarios
// -----------------------------------------------------------------------------

/// Extrai o token Bearer do cabecalho de autorizacao e valida a assinatura.
fn extrair_claims_do_bearer(
    servico_jwt: &ServicoJwt,
    cabecalhos: &HeaderMap,
) -> Result<Claims, (StatusCode, Json<ErroApi>)> {
    let autorizacao = cabecalhos
        .get("authorization")
        .and_then(|valor| valor.to_str().ok())
        .ok_or_else(|| erro_http(StatusCode::UNAUTHORIZED, "cabecalho de autorizacao ausente"))?;

    let token = autorizacao
        .strip_prefix("Bearer ")
        .ok_or_else(|| erro_http(StatusCode::UNAUTHORIZED, "token de autorizacao invalido"))?;

    servico_jwt
        .validar(token)
        .map_err(|_| erro_http(StatusCode::UNAUTHORIZED, "token invalido ou expirado"))
}

/// Converte erros do dominio de autenticacao em respostas HTTP.
fn mapear_erro_autenticacao(erro: ErroAutenticacao) -> (StatusCode, Json<ErroApi>) {
    match erro {
        ErroAutenticacao::CredenciaisInvalidas
        | ErroAutenticacao::EmailInvalido
        | ErroAutenticacao::SenhaFraca => erro_http(StatusCode::BAD_REQUEST, erro.to_string()),
        ErroAutenticacao::ContaInativa => erro_http(StatusCode::FORBIDDEN, erro.to_string()),
        ErroAutenticacao::NaoEncontrado => erro_http(StatusCode::NOT_FOUND, erro.to_string()),
        ErroAutenticacao::Token(mensagem) => erro_http(StatusCode::UNAUTHORIZED, mensagem),
        ErroAutenticacao::Tecnica(mensagem) => {
            erro_http(StatusCode::INTERNAL_SERVER_ERROR, mensagem)
        }
    }
}

/// Converte erros de recuperacao de acesso em respostas HTTP (RF03).
/// 'CodigoInvalidoOuExpirado' usa a mesma mensagem generica, sem revelar
/// se a conta existe ou qual codigo falhou.
fn mapear_erro_recuperacao(erro: ErroRecuperacao) -> (StatusCode, Json<ErroApi>) {
    match erro {
        ErroRecuperacao::EmailInvalido
        | ErroRecuperacao::CodigoInvalidoOuExpirado
        | ErroRecuperacao::SenhaFraca => erro_http(StatusCode::BAD_REQUEST, erro.to_string()),
        ErroRecuperacao::Tecnica(mensagem) => {
            erro_http(StatusCode::INTERNAL_SERVER_ERROR, mensagem)
        }
    }
}

/// Monta uma resposta de erro padrao da API.
fn erro_http(codigo: StatusCode, mensagem: impl Into<String>) -> (StatusCode, Json<ErroApi>) {
    (codigo, Json(ErroApi::nova(mensagem)))
}
