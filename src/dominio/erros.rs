// =============================================================================
// Erros de dominio e de autenticacao
// =============================================================================

use thiserror::Error;

/// Erros da camada de autenticacao do UC01 (autenticar usuario).
#[derive(Debug, Error)]
pub enum ErroAutenticacao {
    /// Credenciais invalidas (e-mail ou senha incorretos).
    #[error("credenciais invalidas")]
    CredenciaisInvalidas,
    /// Conta inativa — acesso bloqueado (RN04).
    #[error("conta inativa: contate o coordenador da unidade")]
    ContaInativa,
    /// E-mail com formato invalido.
    #[error("formato de email invalido")]
    EmailInvalido,
    /// Nova senha nao atende aos criterios minimos.
    #[error("a nova senha deve ter no minimo 8 caracteres")]
    SenhaFraca,
    /// Usuario/registro nao encontrado.
    #[error("registro nao encontrado")]
    NaoEncontrado,
    /// Falha ao emitir/validar o token JWT.
    #[error("falha na emissao ou validacao do token: {0}")]
    Token(String),
    /// Falha tecnica na camada de banco ou seguranca.
    #[error("falha tecnica: {0}")]
    Tecnica(String),
}

impl From<sqlx::Error> for ErroAutenticacao {
    fn from(erro: sqlx::Error) -> Self {
        ErroAutenticacao::Tecnica(erro.to_string())
    }
}