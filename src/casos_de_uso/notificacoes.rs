// =============================================================================
// UC10 — Receber notificacoes
// -----------------------------------------------------------------------------
// Regras aplicadas:
//   - Texto sempre generico: nome, endereco e situacao de familias jamais
//     aparecem na notificacao (RN21). Por isso a emissao so aceita o
//     destinatario e um motivo padronizado — nunca texto livre;
//   - A notificacao e visivel somente para o destinatario (RN05/RN06);
//   - Abrir a notificacao exige sessao valida (extrator HTTP) e marca o aviso
//     como lido; tentativa de abrir aviso alheio nega o acesso e registra a
//     tentativa (RN22);
//   - Gatilhos do prototipo: visita agendada/reagendada/cancelada (UC04) e
//     pedido de retorno criado/aprovado/rejeitado (UC08).
// =============================================================================

use std::sync::Arc;
use uuid::Uuid;

use crate::dominio::entidades::notificacao::Notificacao;
use crate::dominio::entidades::usuario::Usuario;
use crate::dominio::enums::TipoNotificacao;
use crate::dominio::erros::ErroNotificacao;
use crate::infraestrutura::auditoria::{ServicoAuditoria, acao};
use crate::infraestrutura::repositorios::notificacao_repositorio::RepositorioNotificacao;
use crate::infraestrutura::repositorios::usuario_repositorio::RepositorioUsuario;

/// Textos padronizados e genericos das notificacoes (RN21).
mod texto {
    pub const NOVA_VISITA_TITULO: &str = "Nova visita agendada";
    pub const NOVA_VISITA_MENSAGEM: &str =
        "Uma nova visita entrou na sua agenda. Abra o aplicativo para ver os detalhes.";
    pub const MUDANCA_AGENDA_TITULO: &str = "Agenda atualizada";
    pub const MUDANCA_AGENDA_MENSAGEM: &str =
        "Uma visita da sua agenda foi alterada. Abra o aplicativo para ver os detalhes.";
    pub const VISITA_CANCELADA_TITULO: &str = "Visita cancelada";
    pub const VISITA_CANCELADA_MENSAGEM: &str =
        "Uma visita da sua agenda foi cancelada. Abra o aplicativo para ver os detalhes.";
    pub const RETORNO_SOLICITADO_TITULO: &str = "Pedido de retorno";
    pub const RETORNO_SOLICITADO_MENSAGEM: &str =
        "Ha um novo pedido de retorno aguardando analise. Abra o aplicativo para ver os detalhes.";
    pub const RETORNO_APROVADO_TITULO: &str = "Retorno aprovado";
    pub const RETORNO_APROVADO_MENSAGEM: &str =
        "Um pedido de retorno foi aprovado e agendado. Abra o aplicativo para ver os detalhes.";
    pub const RETORNO_REJEITADO_TITULO: &str = "Retorno nao aprovado";
    pub const RETORNO_REJEITADO_MENSAGEM: &str =
        "Um pedido de retorno foi analisado. Abra o aplicativo para ver os detalhes.";
}

/// Emissor de notificacoes, usado pelos demais casos de uso nos gatilhos do
/// sistema. Os textos sao sempre os templates genericos acima (RN21).
#[derive(Clone)]
pub struct ServicoNotificacoes {
    repositorio: Arc<dyn RepositorioNotificacao>,
}

impl ServicoNotificacoes {
    /// Cria o servico com o repositorio fornecido.
    pub fn novo(repositorio: Arc<dyn RepositorioNotificacao>) -> Self {
        Self { repositorio }
    }

    /// Avisa o tecnico sobre uma nova visita na agenda (UC04).
    pub async fn avisar_nova_visita(&self, id_tecnico: Uuid) -> Result<(), sqlx::Error> {
        self.emitir(
            id_tecnico,
            texto::NOVA_VISITA_TITULO,
            texto::NOVA_VISITA_MENSAGEM,
            TipoNotificacao::Lembrete,
        )
        .await
    }

    /// Avisa o tecnico sobre mudanca de data, horario ou responsavel (UC04).
    pub async fn avisar_mudanca_agenda(&self, id_tecnico: Uuid) -> Result<(), sqlx::Error> {
        self.emitir(
            id_tecnico,
            texto::MUDANCA_AGENDA_TITULO,
            texto::MUDANCA_AGENDA_MENSAGEM,
            TipoNotificacao::Aviso,
        )
        .await
    }

    /// Avisa o tecnico sobre o cancelamento de uma visita (UC04).
    pub async fn avisar_cancelamento(&self, id_tecnico: Uuid) -> Result<(), sqlx::Error> {
        self.emitir(
            id_tecnico,
            texto::VISITA_CANCELADA_TITULO,
            texto::VISITA_CANCELADA_MENSAGEM,
            TipoNotificacao::Aviso,
        )
        .await
    }

    /// Avisa o coordenador sobre um novo pedido de retorno (UC08).
    pub async fn avisar_retorno_solicitado(&self, id_coordenador: Uuid) -> Result<(), sqlx::Error> {
        self.emitir(
            id_coordenador,
            texto::RETORNO_SOLICITADO_TITULO,
            texto::RETORNO_SOLICITADO_MENSAGEM,
            TipoNotificacao::Aviso,
        )
        .await
    }

    /// Avisa o tecnico solicitante sobre a aprovacao do retorno (UC08).
    pub async fn avisar_retorno_aprovado(&self, id_tecnico: Uuid) -> Result<(), sqlx::Error> {
        self.emitir(
            id_tecnico,
            texto::RETORNO_APROVADO_TITULO,
            texto::RETORNO_APROVADO_MENSAGEM,
            TipoNotificacao::Aviso,
        )
        .await
    }

    /// Avisa o tecnico solicitante sobre a rejeicao do retorno (UC08).
    pub async fn avisar_retorno_rejeitado(&self, id_tecnico: Uuid) -> Result<(), sqlx::Error> {
        self.emitir(
            id_tecnico,
            texto::RETORNO_REJEITADO_TITULO,
            texto::RETORNO_REJEITADO_MENSAGEM,
            TipoNotificacao::Aviso,
        )
        .await
    }

    /// Persiste a notificacao com os textos genericos do template.
    async fn emitir(
        &self,
        id_usuario_destino: Uuid,
        titulo: &str,
        mensagem: &str,
        tipo: TipoNotificacao,
    ) -> Result<(), sqlx::Error> {
        let notificacao = Notificacao {
            id: Uuid::new_v4(),
            id_usuario_destino,
            titulo: titulo.to_string(),
            mensagem: mensagem.to_string(),
            tipo,
            lida: false,
            lida_em: None,
            data_hora: chrono::Utc::now(),
        };
        self.repositorio.criar(&notificacao).await.map(|_| ())
    }
}

/// Orquestrador do caso de uso de notificacoes do usuario (UC10).
#[derive(Clone)]
pub struct CasoDeUsoNotificacoes {
    repositorio_notificacao: Arc<dyn RepositorioNotificacao>,
    repositorio_usuario: Arc<dyn RepositorioUsuario>,
    auditoria: ServicoAuditoria,
}

impl CasoDeUsoNotificacoes {
    /// Cria o caso de uso com suas dependencias.
    pub fn novo(
        repositorio_notificacao: Arc<dyn RepositorioNotificacao>,
        repositorio_usuario: Arc<dyn RepositorioUsuario>,
        auditoria: ServicoAuditoria,
    ) -> Self {
        Self {
            repositorio_notificacao,
            repositorio_usuario,
            auditoria,
        }
    }

    /// Lista as notificacoes do usuario autenticado, da mais recente para a
    /// mais antiga (RN05/RN06).
    pub async fn listar_notificacoes(
        &self,
        id_usuario: Uuid,
        somente_nao_lidas: bool,
    ) -> Result<Vec<Notificacao>, ErroNotificacao> {
        self.usuario_ativo(id_usuario).await?;

        Ok(self
            .repositorio_notificacao
            .listar_por_destinatario(id_usuario, somente_nao_lidas)
            .await?)
    }

    /// Abre uma notificacao: confere o destinatario, marca como lida e a
    /// devolve. Aviso de outro usuario nega o acesso e registra a tentativa
    /// (RN22); a notificacao permanece como nao lida.
    pub async fn abrir_notificacao(
        &self,
        id_usuario: Uuid,
        id_notificacao: Uuid,
    ) -> Result<Notificacao, ErroNotificacao> {
        let logado = self.usuario_ativo(id_usuario).await?;

        let notificacao = self
            .repositorio_notificacao
            .buscar_por_id(id_notificacao)
            .await?
            .ok_or(ErroNotificacao::NaoEncontrado)?;

        if notificacao.id_usuario_destino != logado.id {
            let _ = self
                .auditoria
                .registrar(
                    Some(logado.id),
                    "notificacao",
                    id_notificacao,
                    acao::NOTIFICACAO_ACESSO_NEGADO,
                    Some("Tentativa de abrir notificacao de outro usuario"),
                    None,
                    None,
                )
                .await;
            return Err(ErroNotificacao::NaoEncontrado);
        }

        if !notificacao.lida {
            self.repositorio_notificacao
                .marcar_lida(notificacao.id, chrono::Utc::now())
                .await?;
        }

        self.repositorio_notificacao
            .buscar_por_id(notificacao.id)
            .await?
            .ok_or(ErroNotificacao::NaoEncontrado)
    }

    /// Carrega o usuario autenticado e exige conta ativa.
    async fn usuario_ativo(&self, id: Uuid) -> Result<Usuario, ErroNotificacao> {
        let usuario = self
            .repositorio_usuario
            .buscar_por_id(id)
            .await?
            .ok_or(ErroNotificacao::NaoEncontrado)?;

        if !usuario.esta_ativo() {
            return Err(ErroNotificacao::SemPermissao);
        }
        Ok(usuario)
    }
}
