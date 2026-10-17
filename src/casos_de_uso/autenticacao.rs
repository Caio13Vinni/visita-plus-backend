// =============================================================================
// UC01 — Autenticar usuario
// -----------------------------------------------------------------------------
// Regras aplicadas:
//   - Validacao de e-mail e hash seguro de senha (Argon2id);
//   - Emissao de JWT com claims de ID, E-mail e Perfil;
//   - Troca obrigatoria de senha no primeiro acesso (RN05);
//   - Bloqueio de contas inativas (RN04);
//   - Auditoria de tentativas de acesso (UC12).
// =============================================================================

use std::sync::Arc;
use uuid::Uuid;

use crate::dominio::entidades::usuario::Usuario;
use crate::dominio::erros::ErroAutenticacao;
use crate::infraestrutura::auditoria::ServicoAuditoria;
use crate::infraestrutura::auditoria::acao;
use crate::infraestrutura::repositorios::usuario_repositorio::RepositorioUsuario;
use crate::infraestrutura::seguranca::jwt::ServicoJwt;
use crate::infraestrutura::seguranca::senha;

/// Credenciais informadas pelo usuario no login.
pub struct Credenciais {
    pub email: String,
    pub senha: String,
}

/// Resultado bem-sucedido do login.
#[derive(Debug, Clone)]
pub struct ResultadoAutenticacao {
    pub token: String,
    pub usuario: Usuario,
    /// Verdadeiro quando o primeiro acesso exige troca de senha (RN05).
    pub exige_troca_senha: bool,
}

/// Orquestrador do caso de uso de autenticacao.
#[derive(Clone)]
pub struct CasoDeUsoAutenticacao {
    repositorio_usuario: Arc<dyn RepositorioUsuario>,
    servico_jwt: ServicoJwt,
    auditoria: ServicoAuditoria,
}

impl CasoDeUsoAutenticacao {
    /// Cria o caso de uso com suas dependencias.
    pub fn novo(
        repositorio_usuario: Arc<dyn RepositorioUsuario>,
        servico_jwt: ServicoJwt,
        auditoria: ServicoAuditoria,
    ) -> Self {
        Self {
            repositorio_usuario,
            servico_jwt,
            auditoria,
        }
    }

    /// Executa o login do usuario, validando credenciais, status e primeiro acesso.
    pub async fn autenticar(
        &self,
        credenciais: Credenciais,
        ip_origem: Option<&str>,
        dispositivo: Option<&str>,
    ) -> Result<ResultadoAutenticacao, ErroAutenticacao> {
        // 1. Validacao de formato do e-mail
        if !Usuario::email_eh_valido(&credenciais.email) {
            return Err(ErroAutenticacao::EmailInvalido);
        }

        // 2. Busca do usuario pelo e-mail (query case-insensitive)
        let usuario = self
            .repositorio_usuario
            .buscar_por_email(&credenciais.email)
            .await?;

        let Some(usuario) = usuario else {
            // Nao revelar se o e-mail existe: resposta generica
            let _ = self
                .auditoria
                .registrar(
                    None,
                    "usuario",
                    Uuid::nil(),
                    acao::LOGIN_FALHA,
                    Some("tentativa de login com email inexistente"),
                    ip_origem,
                    dispositivo,
                )
                .await;
            return Err(ErroAutenticacao::CredenciaisInvalidas);
        };

        // 3. Bloqueio de contas inativas (RN04)
        if !usuario.esta_ativo() {
            let _ = self
                .auditoria
                .registrar(
                    Some(usuario.id),
                    "usuario",
                    usuario.id,
                    acao::LOGIN_BLOQUEADO,
                    Some("tentativa de login em conta inativa"),
                    ip_origem,
                    dispositivo,
                )
                .await;
            return Err(ErroAutenticacao::ContaInativa);
        }

        // 4. Verificacao da senha contra o hash Argon2id
        let senha_ok = senha::verificar_senha(&credenciais.senha, &usuario.senha_hash)
            .map_err(|erro| ErroAutenticacao::Tecnica(erro.to_string()))?;
        if !senha_ok {
            let _ = self
                .auditoria
                .registrar(
                    Some(usuario.id),
                    "usuario",
                    usuario.id,
                    acao::LOGIN_FALHA,
                    Some("tentativa de login com senha incorreta"),
                    ip_origem,
                    dispositivo,
                )
                .await;
            return Err(ErroAutenticacao::CredenciaisInvalidas);
        }

        // 5. Atualizacao do ultimo acesso
        let _ = self
            .repositorio_usuario
            .registrar_ultimo_acesso(usuario.id)
            .await;

        // 6. Auditoria de acesso bem-sucedido
        let _ = self
            .auditoria
            .registrar(
                Some(usuario.id),
                "usuario",
                usuario.id,
                acao::LOGIN_SUCESSO,
                Some("login realizado com sucesso"),
                ip_origem,
                dispositivo,
            )
            .await;

        // 7. Emissao do token JWT
        let token = self
            .servico_jwt
            .emitir(&usuario)
            .map_err(|erro| ErroAutenticacao::Token(erro.to_string()))?;

        Ok(ResultadoAutenticacao {
            token,
            exige_troca_senha: usuario.exige_troca_senha,
            usuario,
        })
    }

    /// Executa a troca de senha (primeiro acesso ou alteracao voluntaria).
    pub async fn trocar_senha(
        &self,
        id_usuario: Uuid,
        senha_atual: &str,
        nova_senha: &str,
        ip_origem: Option<&str>,
        dispositivo: Option<&str>,
    ) -> Result<(), ErroAutenticacao> {
        // 1. Localiza o usuario autenticado
        let usuario = self
            .repositorio_usuario
            .buscar_por_id(id_usuario)
            .await?
            .ok_or(ErroAutenticacao::NaoEncontrado)?;

        // 2. Confirma a senha atual
        let senha_ok = senha::verificar_senha(senha_atual, &usuario.senha_hash)
            .map_err(|erro| ErroAutenticacao::Tecnica(erro.to_string()))?;
        if !senha_ok {
            return Err(ErroAutenticacao::CredenciaisInvalidas);
        }

        // 3. Valida os criterios minimos da nova senha
        if !senha::senha_atende_criterios(nova_senha) {
            return Err(ErroAutenticacao::SenhaFraca);
        }

        // 4. Gera o novo hash e persiste (limpa a flag de troca obrigatoria)
        let novo_hash = senha::gerar_hash_senha(nova_senha)
            .map_err(|erro| ErroAutenticacao::Tecnica(erro.to_string()))?;
        self.repositorio_usuario
            .trocar_senha(id_usuario, &novo_hash, false)
            .await?;

        // 5. Auditoria da troca de senha
        let _ = self
            .auditoria
            .registrar(
                Some(id_usuario),
                "usuario",
                id_usuario,
                acao::TROCA_SENHA,
                Some("senha alterada com sucesso"),
                ip_origem,
                dispositivo,
            )
            .await;

        Ok(())
    }
}
