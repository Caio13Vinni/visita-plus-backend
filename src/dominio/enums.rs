// =============================================================================
// Enums de dominio — estados controlados da documentacao academica
// =============================================================================

/// Perfil de acesso dos usuarios do sistema (RN02, RN03).
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type, serde::Serialize, serde::Deserialize)]
#[sqlx(type_name = "perfil_usuario", rename_all = "PascalCase")]
pub enum PerfilUsuario {
    /// Coordenador: gerencia profissionais, familias, agenda geral e relatorios.
    Coordenador,
    /// Tecnico de Referencia: executa visitas domiciliares.
    Tecnico,
}

impl PerfilUsuario {
    /// Retorna o nome legivel do perfil (usado em JWT e relatorios).
    pub fn como_str(self) -> &'static str {
        match self {
            PerfilUsuario::Coordenador => "Coordenador",
            PerfilUsuario::Tecnico => "Tecnico",
        }
    }
}

/// Status da conta de usuario (RN04: bloqueio de contas inativas).
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type, serde::Serialize, serde::Deserialize)]
#[sqlx(type_name = "status_usuario", rename_all = "PascalCase")]
pub enum StatusUsuario {
    /// Conta ativa e apta a acessar o sistema.
    Ativo,
    /// Conta inativa: acesso bloqueado (RN04).
    Inativo,
}

impl StatusUsuario {
    /// Retorna o nome legivel do status.
    pub fn como_str(self) -> &'static str {
        match self {
            StatusUsuario::Ativo => "Ativo",
            StatusUsuario::Inativo => "Inativo",
        }
    }
}

/// Status do cadastro de familia (UC03).
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type, serde::Serialize, serde::Deserialize)]
#[sqlx(type_name = "status_familia", rename_all = "PascalCase")]
pub enum StatusFamilia {
    /// Familia ativa e assistida pela unidade.
    Ativa,
    /// Familia inativa (sem novas visitas).
    Inativa,
}

impl StatusFamilia {
    /// Retorna o nome legivel do status.
    pub fn como_str(self) -> &'static str {
        match self {
            StatusFamilia::Ativa => "Ativa",
            StatusFamilia::Inativa => "Inativa",
        }
    }
}

/// Prioridade da visita na agenda do tecnico (UC04).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, sqlx::Type, serde::Serialize, serde::Deserialize,
)]
#[sqlx(type_name = "prioridade_visita", rename_all = "PascalCase")]
pub enum PrioridadeVisita {
    /// Baixa: acompanhamento rotineiro.
    Baixa,
    /// Normal: prioridade padrao da maioria das visitas.
    #[default]
    Normal,
    /// Alta: demanda prioritária na agenda.
    Alta,
}

impl PrioridadeVisita {
    /// Retorna o nome legivel da prioridade.
    pub fn como_str(self) -> &'static str {
        match self {
            PrioridadeVisita::Baixa => "Baixa",
            PrioridadeVisita::Normal => "Normal",
            PrioridadeVisita::Alta => "Alta",
        }
    }
}

/// Status do agendamento da visita (UC04/UC05).
/// Os nomes espelham os valores exatos do ENUM do banco (MER).
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type, serde::Serialize, serde::Deserialize)]
#[sqlx(type_name = "status_visita", rename_all = "PascalCase")]
pub enum StatusVisita {
    /// Agendada: compromisso futuro na agenda do tecnico.
    Agendada,
    /// Em andamento: visita iniciada pelo tecnico (UC05).
    #[sqlx(rename = "Em andamento")]
    #[serde(rename = "Em andamento")]
    EmAndamento,
    /// Realizada: visita executada e registrada (UC05).
    Realizada,
    /// Nao realizada: visita tentada sem sucesso, com motivo (RN14).
    #[sqlx(rename = "Não realizada")]
    #[serde(rename = "Não realizada")]
    NaoRealizada,
    /// Cancelada: agendamento desfeito antes da execucao (RN18).
    Cancelada,
}

impl StatusVisita {
    /// Retorna o nome legivel do status (valor do banco).
    pub fn como_str(self) -> &'static str {
        match self {
            StatusVisita::Agendada => "Agendada",
            StatusVisita::EmAndamento => "Em andamento",
            StatusVisita::Realizada => "Realizada",
            StatusVisita::NaoRealizada => "Não realizada",
            StatusVisita::Cancelada => "Cancelada",
        }
    }

    /// Indica se o agendamento ainda pode ser alterado/cancelado (RN18).
    pub fn pode_ser_alterada(self) -> bool {
        self == StatusVisita::Agendada
    }
}

/// Status do registro da visita (UC05 / RN12).
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type, serde::Serialize, serde::Deserialize)]
#[sqlx(type_name = "status_registro", rename_all = "PascalCase")]
pub enum StatusRegistro {
    /// Rascunho: registro em edicao pelo tecnico (RN12).
    Rascunho,
    /// Finalizado: registro fechado e imutavel (RN16).
    Finalizado,
}

impl StatusRegistro {
    /// Retorna o nome legivel do status.
    pub fn como_str(self) -> &'static str {
        match self {
            StatusRegistro::Rascunho => "Rascunho",
            StatusRegistro::Finalizado => "Finalizado",
        }
    }
}

/// Resultado da tentativa de visita (UC05 / RN13, RN14).
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type, serde::Serialize, serde::Deserialize)]
#[sqlx(type_name = "resultado_registro", rename_all = "PascalCase")]
pub enum ResultadoRegistro {
    /// Visita realizada com sucesso.
    Realizada,
    /// Visita tentada, mas nao realizada (exige motivo — RN14).
    #[sqlx(rename = "Não realizada")]
    #[serde(rename = "Não realizada")]
    NaoRealizada,
}

impl ResultadoRegistro {
    /// Retorna o nome legivel do resultado (valor do banco).
    pub fn como_str(self) -> &'static str {
        match self {
            ResultadoRegistro::Realizada => "Realizada",
            ResultadoRegistro::NaoRealizada => "Não realizada",
        }
    }
}

/// Status do pedido de retorno (UC08 / RN19, RN20).
/// Os nomes espelham os valores exatos do ENUM do banco (MER).
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type, serde::Serialize, serde::Deserialize)]
#[sqlx(type_name = "status_solicitacao", rename_all = "PascalCase")]
pub enum StatusSolicitacao {
    /// Pendente: aguardando decisao do coordenador (RN19).
    Pendente,
    /// Aprovada: decisao favoravel ainda sem visita vinculada (RN20).
    Aprovada,
    /// Rejeitada: recusada pelo coordenador, com justificativa (RN19).
    Rejeitada,
    /// Cancelada: retirada pelo solicitante antes da decisao (RN20).
    Cancelada,
    /// Agendada: aprovada e ja com nova visita programada (RN19/RN20).
    Agendada,
}

impl StatusSolicitacao {
    /// Retorna o nome legivel do status (valor do banco).
    pub fn como_str(self) -> &'static str {
        match self {
            StatusSolicitacao::Pendente => "Pendente",
            StatusSolicitacao::Aprovada => "Aprovada",
            StatusSolicitacao::Rejeitada => "Rejeitada",
            StatusSolicitacao::Cancelada => "Cancelada",
            StatusSolicitacao::Agendada => "Agendada",
        }
    }

    /// Indica se o pedido ainda aguarda decisao do coordenador (RN19).
    pub fn pode_ser_decidida(self) -> bool {
        self == StatusSolicitacao::Pendente
    }

    /// Indica se o pedido permanece em aberto (impede novo pedido para a
    /// mesma visita — RN19/RN20).
    pub fn esta_aberta(self) -> bool {
        matches!(
            self,
            StatusSolicitacao::Pendente | StatusSolicitacao::Aprovada | StatusSolicitacao::Agendada
        )
    }
}

/// Tipo da notificacao (UC10 / RN21).
/// Os nomes espelham os valores exatos do ENUM do banco (MER).
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type, serde::Serialize, serde::Deserialize)]
#[sqlx(type_name = "tipo_notificacao", rename_all = "PascalCase")]
pub enum TipoNotificacao {
    /// Lembrete de agenda (ex.: nova visita agendada).
    Lembrete,
    /// Aviso de mudanca (ex.: reagendamento, cancelamento, retorno).
    Aviso,
    /// Mensagem generica do sistema.
    Sistema,
}

impl TipoNotificacao {
    /// Retorna o nome legivel do tipo (valor do banco).
    pub fn como_str(self) -> &'static str {
        match self {
            TipoNotificacao::Lembrete => "Lembrete",
            TipoNotificacao::Aviso => "Aviso",
            TipoNotificacao::Sistema => "Sistema",
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn perfil_seraliza_como_pascal_case_do_banco() {
        assert_eq!(PerfilUsuario::Coordenador.como_str(), "Coordenador");
        assert_eq!(PerfilUsuario::Tecnico.como_str(), "Tecnico");
    }

    #[test]
    fn status_seraliza_como_pascal_case_do_banco() {
        assert_eq!(StatusUsuario::Ativo.como_str(), "Ativo");
        assert_eq!(StatusUsuario::Inativo.como_str(), "Inativo");
        assert_eq!(StatusFamilia::Ativa.como_str(), "Ativa");
        assert_eq!(StatusFamilia::Inativa.como_str(), "Inativa");
    }

    #[test]
    fn prioridade_seraliza_como_pascal_case_do_banco() {
        assert_eq!(PrioridadeVisita::Baixa.como_str(), "Baixa");
        assert_eq!(PrioridadeVisita::Normal.como_str(), "Normal");
        assert_eq!(PrioridadeVisita::Alta.como_str(), "Alta");
    }

    #[test]
    fn status_visita_espelha_valores_acentuados_do_banco() {
        assert_eq!(StatusVisita::EmAndamento.como_str(), "Em andamento");
        assert_eq!(StatusVisita::NaoRealizada.como_str(), "Não realizada");
        assert_eq!(StatusVisita::Agendada.como_str(), "Agendada");
        assert_eq!(StatusVisita::Realizada.como_str(), "Realizada");
        assert_eq!(StatusVisita::Cancelada.como_str(), "Cancelada");
    }

    #[test]
    fn somente_visita_agendada_pode_ser_alterada() {
        assert!(StatusVisita::Agendada.pode_ser_alterada());
        assert!(!StatusVisita::EmAndamento.pode_ser_alterada());
        assert!(!StatusVisita::Realizada.pode_ser_alterada());
        assert!(!StatusVisita::NaoRealizada.pode_ser_alterada());
        assert!(!StatusVisita::Cancelada.pode_ser_alterada());
    }

    #[test]
    fn resultado_registro_espelha_valores_acentuados_do_banco() {
        assert_eq!(ResultadoRegistro::Realizada.como_str(), "Realizada");
        assert_eq!(ResultadoRegistro::NaoRealizada.como_str(), "Não realizada");
        assert_eq!(StatusRegistro::Rascunho.como_str(), "Rascunho");
        assert_eq!(StatusRegistro::Finalizado.como_str(), "Finalizado");
    }

    #[test]
    fn status_solicitacao_espelha_valores_do_banco() {
        assert_eq!(StatusSolicitacao::Pendente.como_str(), "Pendente");
        assert_eq!(StatusSolicitacao::Aprovada.como_str(), "Aprovada");
        assert_eq!(StatusSolicitacao::Rejeitada.como_str(), "Rejeitada");
        assert_eq!(StatusSolicitacao::Cancelada.como_str(), "Cancelada");
        assert_eq!(StatusSolicitacao::Agendada.como_str(), "Agendada");
    }

    #[test]
    fn somente_pendente_pode_ser_decidida() {
        assert!(StatusSolicitacao::Pendente.pode_ser_decidida());
        assert!(!StatusSolicitacao::Aprovada.pode_ser_decidida());
        assert!(!StatusSolicitacao::Rejeitada.pode_ser_decidida());
        assert!(!StatusSolicitacao::Cancelada.pode_ser_decidida());
        assert!(!StatusSolicitacao::Agendada.pode_ser_decidida());
    }

    #[test]
    fn status_abertos_bloqueiam_novo_pedido_para_a_mesma_visita() {
        assert!(StatusSolicitacao::Pendente.esta_aberta());
        assert!(StatusSolicitacao::Aprovada.esta_aberta());
        assert!(StatusSolicitacao::Agendada.esta_aberta());
        assert!(!StatusSolicitacao::Rejeitada.esta_aberta());
        assert!(!StatusSolicitacao::Cancelada.esta_aberta());
    }

    #[test]
    fn tipo_notificacao_espelha_valores_do_banco() {
        assert_eq!(TipoNotificacao::Lembrete.como_str(), "Lembrete");
        assert_eq!(TipoNotificacao::Aviso.como_str(), "Aviso");
        assert_eq!(TipoNotificacao::Sistema.como_str(), "Sistema");
    }
}
