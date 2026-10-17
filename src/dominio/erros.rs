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

/// Erros dos casos de uso de cadastro (UC02 profissionais / UC03 familias).
#[derive(Debug, Error)]
pub enum ErroCadastro {
    /// Usuario autenticado nao possui perfil para a operacao (RN02).
    #[error("perfil sem permissao para esta operacao")]
    SemPermissao,
    /// E-mail ja cadastrado nao pode ser duplicado (UC02).
    #[error("ja existe um usuario com este email")]
    EmailJaExistente,
    /// Codigo de familia ja cadastrado (UC03 / RN23).
    #[error("ja existe uma familia com este codigo")]
    CodigoJaExistente,
    /// Nao e possivel inativar tecnico com visitas futuras ativas.
    #[error("nao e possivel inativar o tecnico: existem visitas futuras ativas")]
    TecnicoComVisitasFuturas,
    /// Registro nao encontrado.
    #[error("registro nao encontrado")]
    NaoEncontrado,
    /// Dados de entrada invalidos.
    #[error("{0}")]
    DadosInvalidos(String),
    /// Falha tecnica na camada de banco ou seguranca.
    #[error("falha tecnica: {0}")]
    Tecnica(String),
}

impl From<sqlx::Error> for ErroCadastro {
    fn from(erro: sqlx::Error) -> Self {
        ErroCadastro::Tecnica(erro.to_string())
    }
}

/// Identifica se um erro SQL corresponde a violacao de constraint UNIQUE.
pub fn erro_sql_e_violacao_de_unicidade(erro: &sqlx::Error) -> bool {
    if let sqlx::Error::Database(detalhe) = erro {
        let codigo = detalhe
            .code()
            .map(|codigo| codigo.to_string())
            .unwrap_or_default();
        codigo == "23505"
    } else {
        false
    }
}
