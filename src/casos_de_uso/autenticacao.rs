// =============================================================================
// UC01 — Autenticar usuario
// -----------------------------------------------------------------------------
// Regras aplicadas:
//   - Validacao de e-mail e hash seguro de senha (Argon2id);
//   - Emissao de JWT com claims de ID, Sessao, E-mail e Perfil;
//   - Troca obrigatoria de senha no primeiro acesso (RN05);
//   - Bloqueio de contas inativas (RN04);
//   - Sessao server-side com encerramento manual e expiracao por
//     inatividade renovada a cada requisicao (RF04/RNF07);
//   - Auditoria de tentativas de acesso (UC12).
// =============================================================================

use std::sync::Arc;
use uuid::Uuid;

use crate::dominio::entidades::sessao::Sessao;
use crate::dominio::entidades::usuario::Usuario;
use crate::dominio::erros::ErroAutenticacao;
use crate::infraestrutura::auditoria::ServicoAuditoria;
use crate::infraestrutura::auditoria::acao;
use crate::infraestrutura::repositorios::sessao_repositorio::RepositorioSessao;
use crate::infraestrutura::repositorios::usuario_repositorio::RepositorioUsuario;
use crate::infraestrutura::seguranca::jwt::{Claims, ServicoJwt};
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
    repositorio_sessao: Arc<dyn RepositorioSessao>,
    servico_jwt: ServicoJwt,
    auditoria: ServicoAuditoria,
}

impl CasoDeUsoAutenticacao {
    /// Cria o caso de uso com suas dependencias.
    pub fn novo(
        repositorio_usuario: Arc<dyn RepositorioUsuario>,
        repositorio_sessao: Arc<dyn RepositorioSessao>,
        servico_jwt: ServicoJwt,
        auditoria: ServicoAuditoria,
    ) -> Self {
        Self {
            repositorio_usuario,
            repositorio_sessao,
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

        // 7. Emissao do token JWT vinculado a sessao que sera criada
        let id_sessao = Uuid::new_v4();
        let token = self
            .servico_jwt
            .emitir(&usuario, id_sessao)
            .map_err(|erro| ErroAutenticacao::Token(erro.to_string()))?;

        // 8. Criacao da sessao server-side ja com o token (RF04/RNF07)
        let expira_em = chrono::Utc::now()
            + chrono::Duration::minutes(self.servico_jwt.expiracao_minutos() as i64);
        let _ = self
            .repositorio_sessao
            .criar(
                id_sessao,
                usuario.id,
                &token,
                expira_em,
                ip_origem,
                dispositivo,
            )
            .await;

        Ok(ResultadoAutenticacao {
            token,
            exige_troca_senha: usuario.exige_troca_senha,
            usuario,
        })
    }

    /// Encerra manualmente a sessao informada (RF04).
    ///
    /// So o proprio usuario encerra as proprias sessoes; a sessao ja deve
    /// estar ativa para ser encerrada.
    pub async fn encerrar_sessao(
        &self,
        id_usuario: Uuid,
        id_sessao: Uuid,
    ) -> Result<(), ErroAutenticacao> {
        let sessao = self
            .repositorio_sessao
            .buscar_por_id(id_sessao)
            .await?
            .ok_or(ErroAutenticacao::SessaoInvalida)?;

        if sessao.id_usuario != id_usuario || !sessao.esta_ativa_em(chrono::Utc::now()) {
            return Err(ErroAutenticacao::SessaoInvalida);
        }

        self.repositorio_sessao.encerrar(id_sessao).await?;

        let _ = self
            .auditoria
            .registrar(
                Some(id_usuario),
                "sessao",
                id_sessao,
                acao::SESSAO_ENCERRADA,
                Some("sessao encerrada manualmente pelo usuario"),
                None,
                None,
            )
            .await;

        Ok(())
    }

    /// Valida a sessao vinculada aos claims e renova a expiracao por
    /// atividade (janela deslizante de inatividade, RNF07).
    ///
    /// Usado pelo extrator de autenticacao a cada requisicao protegida:
    /// a sessao precisa existir, nao estar encerrada e estar dentro da
    /// validade; com menos da metade da janela restante, a expiracao e
    /// prorrogada para agora + janela.
    pub async fn validar_sessao(&self, claims: &Claims) -> Result<Sessao, ErroAutenticacao> {
        let agora = chrono::Utc::now();
        let sessao = self
            .repositorio_sessao
            .buscar_por_id(claims.sid)
            .await?
            .ok_or(ErroAutenticacao::SessaoInvalida)?;

        if sessao.id_usuario != claims.sub || !sessao.esta_ativa_em(agora) {
            return Err(ErroAutenticacao::SessaoInvalida);
        }

        let janela = chrono::Duration::minutes(self.servico_jwt.expiracao_minutos() as i64);
        if sessao.expira_em - agora < janela / 2 {
            let nova_expiracao = agora + janela;
            self.repositorio_sessao
                .estender_expiracao(sessao.id, nova_expiracao)
                .await?;
            let mut renovada = sessao;
            renovada.expira_em = nova_expiracao;
            return Ok(renovada);
        }

        Ok(sessao)
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
