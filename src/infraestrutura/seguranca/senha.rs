// =============================================================================
// Hash seguro de senha usando Argon2id (OWASP-recomendado)
// =============================================================================

use argon2::Argon2;
use argon2::password_hash::{
    PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng,
};
use thiserror::Error;

/// Erros da operacao de hash de senha.
#[derive(Debug, Error)]
pub enum ErroSenha {
    #[error("falha ao gerar o hash da senha: {0}")]
    Geracao(String),
    #[error("formato de hash invalido: {0}")]
    FormatoInvalido(String),
}

/// Gera o hash Argon2id de uma senha em texto puro.
pub fn gerar_hash_senha(senha: &str) -> Result<String, ErroSenha> {
    let sal = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(senha.as_bytes(), &sal)
        .map(|hash| hash.to_string())
        .map_err(|erro| ErroSenha::Geracao(erro.to_string()))
}

/// Verifica se a senha em texto puro corresponde ao hash armazenado.
pub fn verificar_senha(senha: &str, hash: &str) -> Result<bool, ErroSenha> {
    let hash_parseado =
        PasswordHash::new(hash).map_err(|erro| ErroSenha::FormatoInvalido(erro.to_string()))?;
    Ok(Argon2::default()
        .verify_password(senha.as_bytes(), &hash_parseado)
        .is_ok())
}

/// Valida os criterios minimos de uma nova senha (RN de primeiro acesso).
pub fn senha_atende_criterios(senha: &str) -> bool {
    senha.chars().count() >= 8
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn hash_de_senha_e_verificado_com_sucesso() {
        let hash = gerar_hash_senha("senha_super_secreta").expect("deve gerar hash");
        assert!(verificar_senha("senha_super_secreta", &hash).expect("deve verificar"));
    }

    #[test]
    fn senha_incorreta_nao_e_verificada() {
        let hash = gerar_hash_senha("senha_correta").expect("deve gerar hash");
        assert!(!verificar_senha("senha_errada", &hash).expect("deve verificar"));
    }

    #[test]
    fn cada_hash_gerado_e_unico() {
        let h1 = gerar_hash_senha("mesma_senha").unwrap();
        let h2 = gerar_hash_senha("mesma_senha").unwrap();
        assert_ne!(h1, h2, "sais aleatorios devem produzir hashes distintos");
    }

    #[test]
    fn criterios_minimos_de_senha() {
        assert!(senha_atende_criterios("12345678"));
        assert!(!senha_atende_criterios("1234567"));
    }
}
