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

/// Erros do caso de uso de agendamento de visitas (UC04).
#[derive(Debug, Error)]
pub enum ErroAgendamento {
    /// Usuario autenticado nao possui perfil para a operacao (RN02).
    #[error("perfil sem permissao para esta operacao")]
    SemPermissao,
    /// Visita, familia ou tecnico nao encontrado.
    #[error("registro nao encontrado")]
    NaoEncontrado,
    /// Familia inativa nao recebe novas visitas.
    #[error("familia inativa nao pode receber novas visitas")]
    FamiliaInativa,
    /// Tecnico informado nao pertence a unidade, nao e Tecnico ou esta inativo.
    #[error("tecnico invalido para esta agenda")]
    TecnicoInvalido,
    /// Agendamento retroativo e proibido (RN09).
    #[error("nao e permitido agendar visita em data ou horario retroativo")]
    DataRetroativa,
    /// Conflito de horario na agenda do tecnico (RN10).
    #[error("o tecnico ja possui visita agendada nesta data e horario")]
    ConflitoDeHorario,
    /// Visita nao esta 'Agendada' e nao pode ser alterada/cancelada (RN18).
    #[error("apenas visitas com status 'Agendada' podem ser alteradas ou canceladas")]
    VisitaNaoPodeSerAlterada,
    /// Motivo obrigatorio para cancelamento (RN18).
    #[error("o motivo do cancelamento e obrigatorio")]
    MotivoObrigatorio,
    /// Dados de entrada invalidos.
    #[error("{0}")]
    DadosInvalidos(String),
    /// Falha tecnica na camada de banco ou seguranca.
    #[error("falha tecnica: {0}")]
    Tecnica(String),
}

impl From<sqlx::Error> for ErroAgendamento {
    fn from(erro: sqlx::Error) -> Self {
        ErroAgendamento::Tecnica(erro.to_string())
    }
}

/// Erros do caso de uso de registro da visita (UC05).
#[derive(Debug, Error)]
pub enum ErroRegistro {
    /// Usuario autenticado nao possui permissao (RN05/RN06).
    #[error("perfil sem permissao para esta operacao")]
    SemPermissao,
    /// Registro ou visita nao encontrado.
    #[error("registro nao encontrado")]
    NaoEncontrado,
    /// Visita nao esta 'Agendada' e nao pode ser iniciada.
    #[error("apenas visitas com status 'Agendada' podem ser iniciadas")]
    VisitaNaoAgendada,
    /// Visita nao esta 'Em andamento' e nao aceita edicao do registro.
    #[error("apenas visitas 'Em andamento' aceitam edicao do registro")]
    VisitaNaoEmAndamento,
    /// Registro ja foi finalizado e e imutavel (RN16).
    #[error("registro finalizado e imutavel: corrija via complemento (RN16)")]
    RegistroNaoRascunho,
    /// Complemento so e permitido em registro finalizado (RN17).
    #[error("complemento exige um registro finalizado")]
    RegistroNaoFinalizado,
    /// Dados de entrada invalidos (RN13/RN14).
    #[error("{0}")]
    DadosInvalidos(String),
    /// Falha tecnica na camada de banco ou seguranca.
    #[error("falha tecnica: {0}")]
    Tecnica(String),
}

impl From<sqlx::Error> for ErroRegistro {
    fn from(erro: sqlx::Error) -> Self {
        ErroRegistro::Tecnica(erro.to_string())
    }
}

/// Erros do caso de uso de solicitacao de retorno (UC08).
#[derive(Debug, Error)]
pub enum ErroSolicitacao {
    /// Usuario autenticado nao possui permissao para a operacao (RN02/RN19).
    #[error("perfil sem permissao para esta operacao")]
    SemPermissao,
    /// Solicitacao ou visita nao encontrada (ou fora da visibilidade).
    #[error("registro nao encontrado")]
    NaoEncontrado,
    /// Visita informada nao admite solicitacao de retorno.
    #[error("a visita informada nao admite solicitacao de retorno")]
    VisitaInvalida,
    /// Motivo obrigatorio na solicitacao (RN19).
    #[error("o motivo da solicitacao de retorno e obrigatorio")]
    MotivoObrigatorio,
    /// Justificativa obrigatoria na rejeicao do pedido (RN19).
    #[error("a justificativa da rejeicao e obrigatoria")]
    JustificativaObrigatoria,
    /// Ja existe pedido em aberto para a visita (RN19/RN20).
    #[error("ja existe uma solicitacao de retorno em aberto para esta visita")]
    SolicitacaoAbertaExistente,
    /// Pedido ja decidido ou cancelado, fora do estado esperado (RN19/RN20).
    #[error("a solicitacao nao esta no estado esperado para esta operacao")]
    EstadoInvalido,
    /// Agendamento retroativo da visita de retorno (RN09).
    #[error("a data da visita de retorno nao pode ser retroativa")]
    DataRetroativa,
    /// Conflito de horario na agenda do tecnico na aprovacao (RN10).
    #[error("ja existe visita agendada para o tecnico neste horario")]
    ConflitoDeHorario,
    /// Dados de entrada invalidos.
    #[error("{0}")]
    DadosInvalidos(String),
    /// Falha tecnica na camada de banco ou seguranca.
    #[error("falha tecnica: {0}")]
    Tecnica(String),
}

impl From<sqlx::Error> for ErroSolicitacao {
    fn from(erro: sqlx::Error) -> Self {
        ErroSolicitacao::Tecnica(erro.to_string())
    }
}

/// Erros do caso de uso de notificacoes (UC10).
#[derive(Debug, Error)]
pub enum ErroNotificacao {
    /// Usuario autenticado nao possui permissao para a operacao (RN02).
    #[error("perfil sem permissao para esta operacao")]
    SemPermissao,
    /// Notificacao nao encontrada (ou destinada a outro usuario).
    #[error("notificacao nao encontrada")]
    NaoEncontrado,
    /// Dados de entrada invalidos.
    #[error("{0}")]
    DadosInvalidos(String),
    /// Falha tecnica na camada de banco ou seguranca.
    #[error("falha tecnica: {0}")]
    Tecnica(String),
}

impl From<sqlx::Error> for ErroNotificacao {
    fn from(erro: sqlx::Error) -> Self {
        ErroNotificacao::Tecnica(erro.to_string())
    }
}

/// Erros do caso de uso de indicadores operacionais (UC11).
#[derive(Debug, Error)]
pub enum ErroIndicadores {
    /// Apenas coordenadores consultam indicadores (UC11).
    #[error("perfil sem permissao para esta operacao")]
    SemPermissao,
    /// Dados de entrada invalidos.
    #[error("{0}")]
    DadosInvalidos(String),
    /// Falha tecnica na camada de banco ou seguranca.
    #[error("falha tecnica: {0}")]
    Tecnica(String),
}

impl From<sqlx::Error> for ErroIndicadores {
    fn from(erro: sqlx::Error) -> Self {
        ErroIndicadores::Tecnica(erro.to_string())
    }
}

/// Erros do caso de uso de consulta a auditoria (UC12).
#[derive(Debug, Error)]
pub enum ErroAuditoria {
    /// Apenas coordenadores consultam a auditoria (UC12).
    #[error("perfil sem permissao para esta operacao")]
    SemPermissao,
    /// Dados de entrada invalidos.
    #[error("{0}")]
    DadosInvalidos(String),
    /// Falha tecnica na camada de banco ou seguranca.
    #[error("falha tecnica: {0}")]
    Tecnica(String),
}

impl From<sqlx::Error> for ErroAuditoria {
    fn from(erro: sqlx::Error) -> Self {
        ErroAuditoria::Tecnica(erro.to_string())
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
