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
}
