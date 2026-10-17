// =============================================================================
// Entidade Familia — cadastro estritamente ficticio (UC03 / RN24)
// Mapeada para a tabela `familia` do MER.
// =============================================================================

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::dominio::enums::StatusFamilia;

/// Representa uma familia acompanhada pela unidade.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Familia {
    pub id: Uuid,
    pub id_unidade: Uuid,
    /// Codigo unico de identificacao da familia (RN23).
    pub codigo: String,
    pub nome_referencia: String,
    pub contato: String,
    pub endereco: String,
    pub status: StatusFamilia,
    pub criado_por: Option<Uuid>,
    pub criado_em: DateTime<Utc>,
}

impl Familia {
    /// Valida os dados minimos de um cadastro de familia.
    pub fn dados_sao_validos(familia: &CadastroFamilia) -> bool {
        !familia.codigo.trim().is_empty()
            && !familia.nome_referencia.trim().is_empty()
            && !familia.contato.trim().is_empty()
            && !familia.endereco.trim().is_empty()
    }
}

/// Dados de entrada para criacao/edicao de familia (UC03).
#[derive(Debug, Clone)]
pub struct CadastroFamilia {
    pub codigo: String,
    pub nome_referencia: String,
    pub contato: String,
    pub endereco: String,
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn cadastro_familia_valido() {
        let cadastro = CadastroFamilia {
            codigo: "FAM-0001".into(),
            nome_referencia: "Maria Silva".into(),
            contato: "(11) 99999-9999".into(),
            endereco: "Rua A, 123".into(),
        };
        assert!(Familia::dados_sao_validos(&cadastro));
    }

    #[test]
    fn cadastro_familia_com_dados_vazios_e_invalido() {
        let cadastro = CadastroFamilia {
            codigo: "  ".into(),
            nome_referencia: "".into(),
            contato: "".into(),
            endereco: "".into(),
        };
        assert!(!Familia::dados_sao_validos(&cadastro));
    }
}
