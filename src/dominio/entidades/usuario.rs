// =============================================================================
// Entidade Usuario — profissional (Coordenador ou Tecnico de Referencia)
// Mapeada para a tabela `usuario` do MER.
// =============================================================================

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::dominio::enums::{PerfilUsuario, StatusUsuario};

/// Representa um profissional registrado no sistema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Usuario {
    pub id: Uuid,
    pub id_unidade: Uuid,
    pub nome: String,
    pub email: String,
    /// Hash da senha (Argon2id).
    pub senha_hash: String,
    pub perfil: PerfilUsuario,
    pub status: StatusUsuario,
    /// Indica troca obrigatoria de senha no primeiro acesso (RN05).
    pub exige_troca_senha: bool,
    pub ultimo_acesso_em: Option<DateTime<Utc>>,
}

impl Usuario {
    /// Verifica se um e-mail possui formato aceitavel no dominio.
    pub fn email_eh_valido(email: &str) -> bool {
        let partes: Vec<&str> = email.split('@').collect();
        if partes.len() != 2 {
            return false;
        }
        let (usuario, dominio) = (partes[0], partes[1]);
        !usuario.is_empty()
            && usuario.len() <= 64
            && dominio.contains('.')
            && dominio.split('.').all(|parte| !parte.is_empty())
    }

    /// Indica se a conta esta ativa para acesso (RN04).
    pub fn esta_ativo(&self) -> bool {
        self.status == StatusUsuario::Ativo
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn email_valido_e_aceito() {
        assert!(Usuario::email_eh_valido("tecnico@cras.gov.br"));
    }

    #[test]
    fn email_sem_arroba_e_rejeitado() {
        assert!(!Usuario::email_eh_valido("tecnico.cras.gov.br"));
    }

    #[test]
    fn email_sem_dominio_e_rejeitado() {
        assert!(!Usuario::email_eh_valido("tecnico@"));
        assert!(!Usuario::email_eh_valido("tecnico@cras"));
    }

    #[test]
    fn conta_inativa_nao_esta_ativa() {
        let usuario = Usuario {
            id: uuid::Uuid::new_v4(),
            id_unidade: uuid::Uuid::new_v4(),
            nome: "Teste".into(),
            email: "teste@teste.com".into(),
            senha_hash: "hash".into(),
            perfil: PerfilUsuario::Tecnico,
            status: StatusUsuario::Inativo,
            exige_troca_senha: true,
            ultimo_acesso_em: None,
        };
        assert!(!usuario.esta_ativo());
    }
}