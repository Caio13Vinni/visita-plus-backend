// =============================================================================
// Servico JWT — emissao e validacao de tokens de acesso
// Claims: ID (sub), Email e Perfil (Coordenador ou Tecnico)
// =============================================================================

use chrono::{Duration, Utc};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::dominio::entidades::usuario::Usuario;
use crate::dominio::enums::PerfilUsuario;

/// Erros do servico de tokens.
#[derive(Debug, Error)]
pub enum ErroJwt {
    #[error("falha ao emitir o token: {0}")]
    Emissao(String),
    #[error("token invalido ou expirado: {0}")]
    Validacao(String),
}

/// Payload do token JWT (claims).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Identificador do usuario autenticado.
    pub sub: Uuid,
    /// E-mail do usuario.
    pub email: String,
    /// Perfil de acesso (Coordenador ou Tecnico).
    pub perfil: PerfilUsuario,
    /// Momento de emissao (epoch segundos).
    pub iat: usize,
    /// Momento de expiracao (epoch segundos).
    pub exp: usize,
}

/// Servico responsavel por emitir e validar tokens JWT.
#[derive(Debug, Clone)]
pub struct ServicoJwt {
    segredo: String,
    expiracao_minutos: u64,
}

impl ServicoJwt {
    /// Cria o servico a partir do segredo e do tempo de expiracao.
    pub fn novo(segredo: impl Into<String>, expiracao_minutos: u64) -> Self {
        Self {
            segredo: segredo.into(),
            expiracao_minutos,
        }
    }

    /// Emite um token JWT assinado para o usuario autenticado.
    pub fn emitir(&self, usuario: &Usuario) -> Result<String, ErroJwt> {
        let agora = Utc::now();
        let claims = Claims {
            sub: usuario.id,
            email: usuario.email.clone(),
            perfil: usuario.perfil,
            iat: agora.timestamp() as usize,
            exp: (agora + Duration::minutes(self.expiracao_minutos as i64)).timestamp() as usize,
        };

        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(self.segredo.as_bytes()),
        )
        .map_err(|erro| ErroJwt::Emissao(erro.to_string()))
    }

    /// Valida a assinatura e a expiracao, devolvendo os claims.
    pub fn validar(&self, token: &str) -> Result<Claims, ErroJwt> {
        let dados = decode::<Claims>(
            token,
            &DecodingKey::from_secret(self.segredo.as_bytes()),
            &Validation::default(),
        )
        .map_err(|erro| ErroJwt::Validacao(erro.to_string()))?;
        Ok(dados.claims)
    }

    /// Recupera a string legivel do perfil a partir dos claims.
    pub fn perfil_como_str(perfil: PerfilUsuario) -> &'static str {
        perfil.como_str()
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::dominio::enums::StatusUsuario;

    fn usuario_exemplo() -> Usuario {
        Usuario {
            id: Uuid::new_v4(),
            id_unidade: Uuid::new_v4(),
            nome: "Tecnico Exemplo".into(),
            email: "tecnico@exemplo.gov.br".into(),
            senha_hash: "hash".into(),
            perfil: PerfilUsuario::Tecnico,
            status: StatusUsuario::Ativo,
            exige_troca_senha: false,
            ultimo_acesso_em: None,
        }
    }

    #[test]
    fn token_emitido_contem_claims_de_id_email_e_perfil() {
        let servico = ServicoJwt::novo("segredo_de_teste", 60);
        let usuario = usuario_exemplo();

        let token = servico.emitir(&usuario).expect("deve emitir token");
        let claims = servico.validar(&token).expect("deve validar token");

        assert_eq!(claims.sub, usuario.id);
        assert_eq!(claims.email, usuario.email);
        assert_eq!(claims.perfil, PerfilUsuario::Tecnico);
        assert!(claims.exp > claims.iat);
    }

    #[test]
    fn token_invalido_e_rejeitado() {
        let servico = ServicoJwt::novo("segredo_de_teste", 60);
        assert!(servico.validar("token.falso.invalido").is_err());
    }

    #[test]
    fn assinatura_de_outro_segredo_e_rejeitada() {
        let emissor = ServicoJwt::novo("segredo_a", 60);
        let validador = ServicoJwt::novo("segredo_b", 60);
        let token = emissor.emitir(&usuario_exemplo()).expect("deve emitir");
        assert!(validador.validar(&token).is_err());
    }
}
