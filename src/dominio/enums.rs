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
    }
}