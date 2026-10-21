// =============================================================================
// UC01/RF03 — Recuperacao de acesso por codigo
// -----------------------------------------------------------------------------
// Regras aplicadas:
//   - Codigo numerico de 6 digitos com validade de 15 minutos;
//   - Resposta publica generica, sem revelar a existencia da conta (RF03);
//   - Uso unico do codigo e invalidacao das solicitacoes anteriores;
//   - Redefinicao de senha com os mesmos criterios minimos do UC01;
//   - Auditoria das operacoes criticas (RN22).
// =============================================================================

use std::sync::Arc;
use uuid::Uuid;

use chrono::{Duration, Utc};
use rand::Rng;

use crate::dominio::entidades::usuario::Usuario;
use crate::dominio::erros::ErroRecuperacao;
use crate::infraestrutura::auditoria::ServicoAuditoria;
use crate::infraestrutura::auditoria::acao;
use crate::infraestrutura::repositorios::recuperacao_repositorio::RepositorioRecuperacao;
use crate::infraestrutura::repositorios::sessao_repositorio::RepositorioSessao;
use crate::infraestrutura::repositorios::usuario_repositorio::RepositorioUsuario;
use crate::infraestrutura::seguranca::senha;

/// Mensagem publica generica devolvida em qualquer cenario de solicitacao,
/// sem revelar se a conta existe (RF03).
pub const MENSAGEM_RECUPERACAO_GENERICA: &str =
    "Se o e-mail estiver cadastrado, um código de recuperação será disponibilizado.";

/// Validade do codigo de recuperacao, em minutos.
const VALIDADE_CODIGO_MINUTOS: i64 = 15;

/// Resultado da solicitacao de recuperacao de acesso.
#[derive(Debug, Clone)]
pub struct SolicitacaoRecuperacao {
    /// Mensagem publica generica (identica para qualquer cenario).
    pub mensagem: &'static str,
    /// Codigo gerado — disponibilizado apenas no resultado do caso de uso
    /// para testes e logs, pois o prototipo academico nao possui envio de
    /// e-mail (SMTP). A API nunca o expoe na resposta HTTP.
    pub codigo: Option<String>,
}

/// Orquestrador do caso de uso de recuperacao de acesso (RF03).
#[derive(Clone)]
pub struct CasoDeUsoRecuperacaoAcesso {
    repositorio_recuperacao: Arc<dyn RepositorioRecuperacao>,
    repositorio_usuario: Arc<dyn RepositorioUsuario>,
    repositorio_sessao: Arc<dyn RepositorioSessao>,
    auditoria: ServicoAuditoria,
}

impl CasoDeUsoRecuperacaoAcesso {
    /// Cria o caso de uso com suas dependencias.
    pub fn novo(
        repositorio_recuperacao: Arc<dyn RepositorioRecuperacao>,
        repositorio_usuario: Arc<dyn RepositorioUsuario>,
        repositorio_sessao: Arc<dyn RepositorioSessao>,
        auditoria: ServicoAuditoria,
    ) -> Self {
        Self {
            repositorio_recuperacao,
            repositorio_usuario,
            repositorio_sessao,
            auditoria,
        }
    }

    /// Solicita um codigo de recuperacao para o e-mail informado (RF03).
    ///
    /// A resposta publica e sempre a mesma, sem revelar a existencia da
    /// conta; apenas contas ativas recebem codigo.
    pub async fn solicitar(&self, email: &str) -> Result<SolicitacaoRecuperacao, ErroRecuperacao> {
        if !Usuario::email_eh_valido(email) {
            return Err(ErroRecuperacao::EmailInvalido);
        }

        let usuario = self.repositorio_usuario.buscar_por_email(email).await?;

        let Some(usuario) = usuario else {
            // E-mail inexistente: mesma resposta publica, sem criar codigo.
            self.auditar_negacao("solicitacao de recuperacao para e-mail inexistente")
                .await;
            return Ok(SolicitacaoRecuperacao {
                mensagem: MENSAGEM_RECUPERACAO_GENERICA,
                codigo: None,
            });
        };

        if !usuario.esta_ativo() {
            // Conta inativa: mesma resposta publica, sem criar codigo.
            self.auditar_negacao("solicitacao de recuperacao para conta inativa")
                .await;
            return Ok(SolicitacaoRecuperacao {
                mensagem: MENSAGEM_RECUPERACAO_GENERICA,
                codigo: None,
            });
        }

        // Conta ativa: invalida codigos pendentes e gera um novo codigo.
        self.repositorio_recuperacao
            .invalidar_pendentes_do_usuario(usuario.id)
            .await?;

        let codigo = gerar_codigo_numerico();
        let expira_em = Utc::now() + Duration::minutes(VALIDADE_CODIGO_MINUTOS);
        let recuperacao = self
            .repositorio_recuperacao
            .criar(usuario.id, &codigo, expira_em)
            .await?;

        let _ = self
            .auditoria
            .registrar(
                Some(usuario.id),
                "recuperacao_acesso",
                recuperacao.id,
                acao::RECUPERACAO_SOLICITADA,
                Some("codigo de recuperacao de acesso solicitado"),
                None,
                None,
            )
            .await;

        Ok(SolicitacaoRecuperacao {
            mensagem: MENSAGEM_RECUPERACAO_GENERICA,
            codigo: Some(recuperacao.codigo),
        })
    }

    /// Redefine a senha do usuario a partir do codigo de recuperacao (RF03).
    pub async fn redefinir(
        &self,
        email: &str,
        codigo: &str,
        nova_senha: &str,
    ) -> Result<(), ErroRecuperacao> {
        if !Usuario::email_eh_valido(email) {
            return Err(ErroRecuperacao::EmailInvalido);
        }

        // Sem usuario ativo para o e-mail: resposta generica, sem revelar
        // se a conta existe (RF03).
        let usuario = self
            .repositorio_usuario
            .buscar_por_email(email)
            .await?
            .filter(|usuario| usuario.esta_ativo());
        let Some(usuario) = usuario else {
            return Err(ErroRecuperacao::CodigoInvalidoOuExpirado);
        };

        let recuperacao = self
            .repositorio_recuperacao
            .buscar_ativa_por_codigo(usuario.id, codigo)
            .await?;

        let Some(recuperacao) = recuperacao else {
            let _ = self
                .auditoria
                .registrar(
                    Some(usuario.id),
                    "usuario",
                    usuario.id,
                    acao::RECUPERACAO_FALHA,
                    Some("tentativa de redefinicao com codigo invalido ou expirado"),
                    None,
                    None,
                )
                .await;
            return Err(ErroRecuperacao::CodigoInvalidoOuExpirado);
        };

        if !senha::senha_atende_criterios(nova_senha) {
            return Err(ErroRecuperacao::SenhaFraca);
        }

        let novo_hash = senha::gerar_hash_senha(nova_senha)
            .map_err(|erro| ErroRecuperacao::Tecnica(erro.to_string()))?;

        // Uso unico do codigo (RF03), seguido da troca efetiva da senha.
        self.repositorio_recuperacao
            .marcar_como_usada(recuperacao.id)
            .await?;
        self.repositorio_usuario
            .trocar_senha(usuario.id, &novo_hash, false)
            .await?;

        // Redefinicao por recuperacao encerra as sessoes ativas do usuario
        // (RF04): acessos anteriores nao devem continuar validos apos a
        // troca da senha fora de uma sessao autenticada.
        self.repositorio_sessao
            .encerrar_todas_do_usuario(usuario.id)
            .await?;

        let _ = self
            .auditoria
            .registrar(
                Some(usuario.id),
                "recuperacao_acesso",
                recuperacao.id,
                acao::RECUPERACAO_CONCLUIDA,
                Some("senha redefinida com codigo de recuperacao"),
                None,
                None,
            )
            .await;

        Ok(())
    }

    /// Audita solicitacao negada (e-mail inexistente ou conta inativa),
    /// sem vincular a um usuario e sem revelar a existencia da conta.
    async fn auditar_negacao(&self, descricao: &str) {
        let _ = self
            .auditoria
            .registrar(
                None,
                "usuario",
                Uuid::nil(),
                acao::RECUPERACAO_NEGADA,
                Some(descricao),
                None,
                None,
            )
            .await;
    }
}

/// Gera um codigo numerico aleatorio de 6 digitos (RF03).
fn gerar_codigo_numerico() -> String {
    let numero: u32 = rand::thread_rng().gen_range(0..1_000_000);
    format!("{numero:06}")
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn codigo_gerado_possui_6_digitos_numericos() {
        for _ in 0..100 {
            let codigo = gerar_codigo_numerico();
            assert_eq!(codigo.len(), 6, "codigo deve ter 6 digitos");
            assert!(
                codigo.chars().all(|caractere| caractere.is_ascii_digit()),
                "codigo deve ser numerico: {codigo}"
            );
        }
    }

    #[test]
    fn codigos_gerados_variam() {
        let codigos: std::collections::HashSet<String> =
            (0..50).map(|_| gerar_codigo_numerico()).collect();
        assert!(codigos.len() > 1, "codigos gerados devem variar");
    }
}
