// =============================================================================
// UC08 — Solicitacao de retorno
// -----------------------------------------------------------------------------
// Regras aplicadas:
//   - Somente o tecnico solicita retorno, para visita realizada da propria
//     agenda (RN05/RN06, RN19);
//   - Somente o coordenador aprova — ja agendando a nova visita — ou rejeita
//     com justificativa obrigatoria (RN19/RN20);
//   - A aprovacao cria a nova visita vinculada ao pedido
//     (`visita.id_origem_retorno`, RN19), herdando familia e prioridade;
//   - Pedidos em aberto ('Pendente', 'Aprovada', 'Agendada') impedem novo
//     pedido para a mesma visita (RN19/RN20);
//   - O solicitante pode cancelar o pedido enquanto pendente (RN20);
//   - Agendamento do retorno respeita data futura (RN09) e conflito de
//     horario na agenda do tecnico (RN10);
//   - Auditoria em todas as operacoes criticas (RN22).
// =============================================================================

use std::sync::Arc;
use uuid::Uuid;

use crate::casos_de_uso::notificacoes::ServicoNotificacoes;
use crate::dominio::entidades::familia::Familia;
use crate::dominio::entidades::solicitacao_retorno::{AprovacaoRetorno, SolicitacaoRetorno};
use crate::dominio::entidades::usuario::Usuario;
use crate::dominio::entidades::visita::Visita;
use crate::dominio::enums::{
    PerfilUsuario, PrioridadeVisita, StatusSolicitacao, StatusUsuario, StatusVisita,
};
use crate::dominio::erros::ErroSolicitacao;
use crate::infraestrutura::auditoria::{ServicoAuditoria, acao};
use crate::infraestrutura::repositorios::familia_repositorio::RepositorioFamilia;
use crate::infraestrutura::repositorios::solicitacao_repositorio::{
    FiltroSolicitacao, RepositorioSolicitacao,
};
use crate::infraestrutura::repositorios::usuario_repositorio::RepositorioUsuario;
use crate::infraestrutura::repositorios::visita_repositorio::RepositorioVisita;

/// Resultado da aprovacao: o pedido atualizado e a visita de retorno criada.
#[derive(Debug, Clone)]
pub struct RetornoAprovado {
    pub solicitacao: SolicitacaoRetorno,
    pub visita: Visita,
}

/// Orquestrador do caso de uso de solicitacao de retorno (UC08).
#[derive(Clone)]
pub struct CasoDeUsoSolicitacaoRetorno {
    repositorio_solicitacao: Arc<dyn RepositorioSolicitacao>,
    repositorio_visita: Arc<dyn RepositorioVisita>,
    repositorio_familia: Arc<dyn RepositorioFamilia>,
    repositorio_usuario: Arc<dyn RepositorioUsuario>,
    auditoria: ServicoAuditoria,
    notificador: Option<ServicoNotificacoes>,
}

impl CasoDeUsoSolicitacaoRetorno {
    /// Cria o caso de uso com suas dependencias.
    pub fn novo(
        repositorio_solicitacao: Arc<dyn RepositorioSolicitacao>,
        repositorio_visita: Arc<dyn RepositorioVisita>,
        repositorio_familia: Arc<dyn RepositorioFamilia>,
        repositorio_usuario: Arc<dyn RepositorioUsuario>,
        auditoria: ServicoAuditoria,
    ) -> Self {
        Self {
            repositorio_solicitacao,
            repositorio_visita,
            repositorio_familia,
            repositorio_usuario,
            auditoria,
            notificador: None,
        }
    }

    /// Habilita o envio de notificacoes genericas (UC10/RN21).
    pub fn com_notificador(mut self, notificador: ServicoNotificacoes) -> Self {
        self.notificador = Some(notificador);
        self
    }

    /// Solicita retorno para uma visita realizada (RN19).
    ///
    /// O pedido nasce 'Pendente' e aguarda decisao do coordenador.
    pub async fn solicitar_retorno(
        &self,
        id_usuario: Uuid,
        id_visita: Uuid,
        motivo: &str,
        prioridade: PrioridadeVisita,
    ) -> Result<SolicitacaoRetorno, ErroSolicitacao> {
        let logado = self.usuario_ativo(id_usuario).await?;

        if logado.perfil != PerfilUsuario::Tecnico {
            return Err(ErroSolicitacao::SemPermissao);
        }

        if motivo.trim().is_empty() {
            return Err(ErroSolicitacao::MotivoObrigatorio);
        }

        let visita = self.visita_visivel(&logado, id_visita).await?;

        if visita.status != StatusVisita::Realizada {
            return Err(ErroSolicitacao::VisitaInvalida);
        }

        if self
            .repositorio_solicitacao
            .buscar_aberta_por_visita(visita.id)
            .await?
            .is_some()
        {
            return Err(ErroSolicitacao::SolicitacaoAbertaExistente);
        }

        let solicitacao = SolicitacaoRetorno {
            id: Uuid::new_v4(),
            id_visita: visita.id,
            motivo: motivo.trim().to_string(),
            prioridade,
            solicitado_por: logado.id,
            data_solicitacao: chrono::Utc::now(),
            status: StatusSolicitacao::Pendente,
            decidido_por: None,
            decisao_motivo: None,
            decidido_em: None,
        };

        let criada = self.repositorio_solicitacao.criar(&solicitacao).await?;

        let _ = self
            .auditoria
            .registrar(
                Some(logado.id),
                "solicitacao_retorno",
                criada.id,
                acao::RETORNO_SOLICITADO,
                Some(&format!(
                    "Retorno solicitado para a visita {}. Motivo: {}",
                    criada.id_visita,
                    criada.motivo.trim()
                )),
                None,
                None,
            )
            .await;

        if let Some(notificador) = &self.notificador
            && let Ok(Some(familia)) = self
                .repositorio_familia
                .buscar_por_id(visita.id_familia)
                .await
            && let Ok(usuarios) = self
                .repositorio_usuario
                .listar(Some(familia.id_unidade))
                .await
        {
            for usuario in usuarios {
                if usuario.perfil == PerfilUsuario::Coordenador && usuario.esta_ativo() {
                    let _ = notificador.avisar_retorno_solicitado(usuario.id).await;
                }
            }
        }

        Ok(criada)
    }

    /// Aprova o pedido e agenda a visita de retorno (RN19/RN20).
    ///
    /// O pedido passa para 'Agendada' e a nova visita fica vinculada a ele
    /// por `id_origem_retorno`. Familia e prioridade sao herdadas do pedido;
    /// tecnico, data e horario sao definidos pelo coordenador.
    pub async fn aprovar_retorno(
        &self,
        id_coordenador: Uuid,
        id_solicitacao: Uuid,
        aprovacao: AprovacaoRetorno,
    ) -> Result<RetornoAprovado, ErroSolicitacao> {
        let logado = self.usuario_ativo(id_coordenador).await?;

        if logado.perfil != PerfilUsuario::Coordenador {
            return Err(ErroSolicitacao::SemPermissao);
        }

        let solicitacao = self
            .repositorio_solicitacao
            .buscar_por_id(id_solicitacao)
            .await?
            .ok_or(ErroSolicitacao::NaoEncontrado)?;

        if !solicitacao.status.pode_ser_decidida() {
            return Err(ErroSolicitacao::EstadoInvalido);
        }

        let (_visita_origem, familia) = self
            .visita_e_familia_na_unidade(&logado, solicitacao.id_visita)
            .await?;

        let tecnico = self
            .repositorio_usuario
            .buscar_por_id(aprovacao.id_tecnico)
            .await?
            .ok_or_else(|| ErroSolicitacao::DadosInvalidos("tecnico informado invalido".into()))?;

        if tecnico.perfil != PerfilUsuario::Tecnico
            || tecnico.status != StatusUsuario::Ativo
            || tecnico.id_unidade != familia.id_unidade
        {
            return Err(ErroSolicitacao::DadosInvalidos(
                "tecnico informado invalido".into(),
            ));
        }

        if !Visita::data_horario_sao_futuros(aprovacao.data_visita, aprovacao.horario) {
            return Err(ErroSolicitacao::DataRetroativa);
        }

        if self
            .repositorio_visita
            .existe_conflito_de_horario(tecnico.id, aprovacao.data_visita, aprovacao.horario, None)
            .await?
        {
            return Err(ErroSolicitacao::ConflitoDeHorario);
        }

        let visita_retorno = Visita {
            id: Uuid::new_v4(),
            id_familia: familia.id,
            id_tecnico: tecnico.id,
            id_agendado_por: logado.id,
            id_origem_retorno: Some(solicitacao.id),
            data_visita: aprovacao.data_visita,
            horario: aprovacao.horario,
            prioridade: solicitacao.prioridade,
            objetivo: format!("Retorno a familia: {}", solicitacao.motivo.trim()),
            status: StatusVisita::Agendada,
            motivo_cancelamento: None,
            criado_em: chrono::Utc::now(),
        };

        let criada = self.repositorio_visita.criar(&visita_retorno).await?;

        let agora = chrono::Utc::now();
        self.repositorio_solicitacao
            .atualizar_decisao(
                solicitacao.id,
                StatusSolicitacao::Agendada,
                logado.id,
                None,
                agora,
            )
            .await?;

        let _ = self
            .auditoria
            .registrar(
                Some(logado.id),
                "solicitacao_retorno",
                solicitacao.id,
                acao::RETORNO_AGENDADO,
                Some(&format!(
                    "Retorno aprovado e agendado na visita {} para {} as {}",
                    criada.id, criada.data_visita, criada.horario
                )),
                None,
                None,
            )
            .await;

        if let Some(notificador) = &self.notificador {
            let _ = notificador
                .avisar_retorno_aprovado(solicitacao.solicitado_por)
                .await;
        }

        let atualizada = self
            .repositorio_solicitacao
            .buscar_por_id(solicitacao.id)
            .await?
            .ok_or(ErroSolicitacao::NaoEncontrado)?;

        Ok(RetornoAprovado {
            solicitacao: atualizada,
            visita: criada,
        })
    }

    /// Rejeita o pedido, com justificativa obrigatoria (RN19).
    pub async fn rejeitar_retorno(
        &self,
        id_coordenador: Uuid,
        id_solicitacao: Uuid,
        justificativa: &str,
    ) -> Result<SolicitacaoRetorno, ErroSolicitacao> {
        let logado = self.usuario_ativo(id_coordenador).await?;

        if logado.perfil != PerfilUsuario::Coordenador {
            return Err(ErroSolicitacao::SemPermissao);
        }

        if justificativa.trim().is_empty() {
            return Err(ErroSolicitacao::JustificativaObrigatoria);
        }

        let solicitacao = self
            .repositorio_solicitacao
            .buscar_por_id(id_solicitacao)
            .await?
            .ok_or(ErroSolicitacao::NaoEncontrado)?;

        if !solicitacao.status.pode_ser_decidida() {
            return Err(ErroSolicitacao::EstadoInvalido);
        }

        self.visita_e_familia_na_unidade(&logado, solicitacao.id_visita)
            .await?;

        self.repositorio_solicitacao
            .atualizar_decisao(
                solicitacao.id,
                StatusSolicitacao::Rejeitada,
                logado.id,
                Some(justificativa.trim()),
                chrono::Utc::now(),
            )
            .await?;

        let _ = self
            .auditoria
            .registrar(
                Some(logado.id),
                "solicitacao_retorno",
                solicitacao.id,
                acao::RETORNO_REJEITADO,
                Some(&format!(
                    "Solicitacao de retorno rejeitada. Justificativa: {}",
                    justificativa.trim()
                )),
                None,
                None,
            )
            .await;

        if let Some(notificador) = &self.notificador {
            let _ = notificador
                .avisar_retorno_rejeitado(solicitacao.solicitado_por)
                .await;
        }

        self.repositorio_solicitacao
            .buscar_por_id(solicitacao.id)
            .await?
            .ok_or(ErroSolicitacao::NaoEncontrado)
    }

    /// Cancela o proprio pedido enquanto pendente (RN20).
    pub async fn cancelar_solicitacao(
        &self,
        id_usuario: Uuid,
        id_solicitacao: Uuid,
    ) -> Result<SolicitacaoRetorno, ErroSolicitacao> {
        let logado = self.usuario_ativo(id_usuario).await?;

        let solicitacao = self
            .repositorio_solicitacao
            .buscar_por_id(id_solicitacao)
            .await?
            .ok_or(ErroSolicitacao::NaoEncontrado)?;

        if solicitacao.solicitado_por != logado.id {
            return Err(ErroSolicitacao::SemPermissao);
        }

        if !solicitacao.status.pode_ser_decidida() {
            return Err(ErroSolicitacao::EstadoInvalido);
        }

        self.repositorio_solicitacao
            .atualizar_decisao(
                solicitacao.id,
                StatusSolicitacao::Cancelada,
                logado.id,
                None,
                chrono::Utc::now(),
            )
            .await?;

        let _ = self
            .auditoria
            .registrar(
                Some(logado.id),
                "solicitacao_retorno",
                solicitacao.id,
                acao::RETORNO_CANCELADO,
                Some("Solicitacao de retorno cancelada pelo solicitante"),
                None,
                None,
            )
            .await;

        self.repositorio_solicitacao
            .buscar_por_id(solicitacao.id)
            .await?
            .ok_or(ErroSolicitacao::NaoEncontrado)
    }

    /// Lista os pedidos conforme o perfil (RN05/RN06).
    ///
    /// O tecnico enxerga somente os pedidos das visitas da propria agenda;
    /// o coordenador enxerga os pedidos da propria unidade.
    pub async fn listar_solicitacoes(
        &self,
        id_usuario: Uuid,
        mut filtro: FiltroSolicitacao,
    ) -> Result<Vec<SolicitacaoRetorno>, ErroSolicitacao> {
        let logado = self.usuario_ativo(id_usuario).await?;

        match logado.perfil {
            PerfilUsuario::Coordenador => filtro.id_unidade = Some(logado.id_unidade),
            PerfilUsuario::Tecnico => filtro.id_tecnico = Some(logado.id),
        }

        Ok(self.repositorio_solicitacao.listar(&filtro).await?)
    }

    /// Carrega o usuario autenticado e exige conta ativa.
    async fn usuario_ativo(&self, id: Uuid) -> Result<Usuario, ErroSolicitacao> {
        let usuario = self
            .repositorio_usuario
            .buscar_por_id(id)
            .await?
            .ok_or(ErroSolicitacao::NaoEncontrado)?;

        if !usuario.esta_ativo() {
            return Err(ErroSolicitacao::SemPermissao);
        }
        Ok(usuario)
    }

    /// Tecnico enxerga somente a propria agenda; visita alheia devolve o
    /// mesmo erro de "nao encontrado" (RN05/RN06).
    async fn visita_visivel(
        &self,
        logado: &Usuario,
        id_visita: Uuid,
    ) -> Result<Visita, ErroSolicitacao> {
        let visita = self
            .repositorio_visita
            .buscar_por_id(id_visita)
            .await?
            .ok_or(ErroSolicitacao::NaoEncontrado)?;

        if visita.id_tecnico != logado.id {
            return Err(ErroSolicitacao::NaoEncontrado);
        }
        Ok(visita)
    }

    /// Coordenador decide apenas pedidos da propria unidade; pedido de outra
    /// unidade devolve o mesmo erro de "nao encontrado".
    async fn visita_e_familia_na_unidade(
        &self,
        logado: &Usuario,
        id_visita: Uuid,
    ) -> Result<(Visita, Familia), ErroSolicitacao> {
        let visita = self
            .repositorio_visita
            .buscar_por_id(id_visita)
            .await?
            .ok_or(ErroSolicitacao::NaoEncontrado)?;

        let familia = self
            .repositorio_familia
            .buscar_por_id(visita.id_familia)
            .await?
            .ok_or(ErroSolicitacao::NaoEncontrado)?;

        if familia.id_unidade != logado.id_unidade {
            return Err(ErroSolicitacao::NaoEncontrado);
        }
        Ok((visita, familia))
    }
}
