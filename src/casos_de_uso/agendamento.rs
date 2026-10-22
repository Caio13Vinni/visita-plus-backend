// =============================================================================
// UC04 — Agendar visitas
// -----------------------------------------------------------------------------
// Regras aplicadas:
//   - Coordenador agenda para qualquer tecnico da unidade; tecnico apenas
//     na propria agenda (RN05, RN06);
//   - Agendamento retroativo e proibido (RN09);
//   - Conflito de horario na agenda do tecnico e validado antes de salvar
//     (RN10);
//   - Cancelamentos e reagendamentos preservam historico via auditoria
//     (RN18, RN22).
// =============================================================================

use std::sync::Arc;
use uuid::Uuid;

use crate::casos_de_uso::notificacoes::ServicoNotificacoes;
use crate::dominio::entidades::usuario::Usuario;
use crate::dominio::entidades::visita::{
    CadastroVisita, DetalheVisita, ReagendamentoVisita, Visita,
};
use crate::dominio::enums::{PerfilUsuario, StatusFamilia, StatusUsuario, StatusVisita};
use crate::dominio::erros::ErroAgendamento;
use crate::infraestrutura::auditoria::{ServicoAuditoria, acao};
use crate::infraestrutura::repositorios::familia_repositorio::RepositorioFamilia;
use crate::infraestrutura::repositorios::usuario_repositorio::RepositorioUsuario;
use crate::infraestrutura::repositorios::visita_repositorio::{FiltroVisita, RepositorioVisita};

/// Orquestrador do caso de uso de agendamento de visitas.
#[derive(Clone)]
pub struct CasoDeUsoAgendamento {
    repositorio_visita: Arc<dyn RepositorioVisita>,
    repositorio_familia: Arc<dyn RepositorioFamilia>,
    repositorio_usuario: Arc<dyn RepositorioUsuario>,
    auditoria: ServicoAuditoria,
    notificador: Option<ServicoNotificacoes>,
}

impl CasoDeUsoAgendamento {
    /// Cria o caso de uso com suas dependencias.
    pub fn novo(
        repositorio_visita: Arc<dyn RepositorioVisita>,
        repositorio_familia: Arc<dyn RepositorioFamilia>,
        repositorio_usuario: Arc<dyn RepositorioUsuario>,
        auditoria: ServicoAuditoria,
    ) -> Self {
        Self {
            repositorio_visita,
            repositorio_familia,
            repositorio_usuario,
            auditoria,
            notificador: None,
        }
    }

    /// Habilita o envio de notificacoes genericas aos tecnicos (UC10/RN21).
    pub fn com_notificador(mut self, notificador: ServicoNotificacoes) -> Self {
        self.notificador = Some(notificador);
        self
    }

    /// Agenda uma nova visita na agenda de um tecnico.
    pub async fn agendar_visita(
        &self,
        id_agendado_por: Uuid,
        cadastro: CadastroVisita,
    ) -> Result<Visita, ErroAgendamento> {
        let agendador = self.usuario_ativo(id_agendado_por).await?;

        if !Visita::dados_sao_validos(&cadastro) {
            return Err(ErroAgendamento::DadosInvalidos(
                "o objetivo da visita e obrigatorio".into(),
            ));
        }

        let familia = self
            .repositorio_familia
            .buscar_por_id(cadastro.id_familia)
            .await?
            .ok_or(ErroAgendamento::NaoEncontrado)?;

        if familia.status != StatusFamilia::Ativa {
            return Err(ErroAgendamento::FamiliaInativa);
        }

        let tecnico = self
            .repositorio_usuario
            .buscar_por_id(cadastro.id_tecnico)
            .await?
            .ok_or(ErroAgendamento::TecnicoInvalido)?;

        self.validar_responsavel(&agendador, &tecnico, familia.id_unidade)?;

        if !Visita::data_horario_sao_futuros(cadastro.data_visita, cadastro.horario) {
            return Err(ErroAgendamento::DataRetroativa);
        }

        if self
            .repositorio_visita
            .existe_conflito_de_horario(tecnico.id, cadastro.data_visita, cadastro.horario, None)
            .await?
        {
            return Err(ErroAgendamento::ConflitoDeHorario);
        }

        let visita = Visita {
            id: Uuid::new_v4(),
            id_familia: familia.id,
            id_tecnico: tecnico.id,
            id_agendado_por: agendador.id,
            id_origem_retorno: None,
            data_visita: cadastro.data_visita,
            horario: cadastro.horario,
            prioridade: cadastro.prioridade,
            objetivo: cadastro.objetivo.trim().to_string(),
            status: StatusVisita::Agendada,
            motivo_cancelamento: None,
            criado_em: chrono::Utc::now(),
        };

        let criada = self.repositorio_visita.criar(&visita).await?;

        let _ = self
            .auditoria
            .registrar(
                Some(agendador.id),
                "visita",
                criada.id,
                acao::VISITA_AGENDADA,
                Some(&format!(
                    "Visita agendada para {} as {} com prioridade {}",
                    criada.data_visita,
                    criada.horario,
                    criada.prioridade.como_str()
                )),
                None,
                None,
            )
            .await;

        if let Some(notificador) = &self.notificador {
            let _ = notificador.avisar_nova_visita(criada.id_tecnico).await;
        }

        Ok(criada)
    }

    /// Cancela um agendamento ainda nao executado, com motivo (RN18).
    pub async fn cancelar_visita(
        &self,
        id_usuario: Uuid,
        id_visita: Uuid,
        motivo: &str,
    ) -> Result<Visita, ErroAgendamento> {
        let logado = self.usuario_ativo(id_usuario).await?;

        if motivo.trim().is_empty() {
            return Err(ErroAgendamento::MotivoObrigatorio);
        }

        let visita = self
            .repositorio_visita
            .buscar_por_id(id_visita)
            .await?
            .ok_or(ErroAgendamento::NaoEncontrado)?;

        if !visita.status.pode_ser_alterada() {
            return Err(ErroAgendamento::VisitaNaoPodeSerAlterada);
        }

        if !self.pode_gerenciar(&logado, &visita).await? {
            return Err(ErroAgendamento::SemPermissao);
        }

        self.repositorio_visita
            .cancelar(id_visita, motivo.trim())
            .await?;

        let _ = self
            .auditoria
            .registrar(
                Some(logado.id),
                "visita",
                id_visita,
                acao::VISITA_CANCELADA,
                Some(&format!("Visita cancelada. Motivo: {}", motivo.trim())),
                None,
                None,
            )
            .await;

        if let Some(notificador) = &self.notificador {
            let _ = notificador.avisar_cancelamento(visita.id_tecnico).await;
        }

        self.repositorio_visita
            .buscar_por_id(id_visita)
            .await?
            .ok_or(ErroAgendamento::NaoEncontrado)
    }

    /// Reagenda uma visita (nova data, horario e/ou tecnico) — RN18.
    pub async fn reagendar_visita(
        &self,
        id_usuario: Uuid,
        id_visita: Uuid,
        reagendamento: ReagendamentoVisita,
    ) -> Result<Visita, ErroAgendamento> {
        let logado = self.usuario_ativo(id_usuario).await?;

        let visita = self
            .repositorio_visita
            .buscar_por_id(id_visita)
            .await?
            .ok_or(ErroAgendamento::NaoEncontrado)?;

        if !visita.status.pode_ser_alterada() {
            return Err(ErroAgendamento::VisitaNaoPodeSerAlterada);
        }

        if !self.pode_gerenciar(&logado, &visita).await? {
            return Err(ErroAgendamento::SemPermissao);
        }

        let novo_tecnico = match reagendamento.id_tecnico {
            Some(id) => {
                let tecnico = self
                    .repositorio_usuario
                    .buscar_por_id(id)
                    .await?
                    .ok_or(ErroAgendamento::TecnicoInvalido)?;

                let familia = self
                    .repositorio_familia
                    .buscar_por_id(visita.id_familia)
                    .await?
                    .ok_or(ErroAgendamento::NaoEncontrado)?;

                if logado.perfil != PerfilUsuario::Coordenador {
                    return Err(ErroAgendamento::SemPermissao);
                }
                if tecnico.perfil != PerfilUsuario::Tecnico
                    || tecnico.status != StatusUsuario::Ativo
                    || tecnico.id_unidade != familia.id_unidade
                {
                    return Err(ErroAgendamento::TecnicoInvalido);
                }
                tecnico.id
            }
            None => visita.id_tecnico,
        };

        if !Visita::data_horario_sao_futuros(reagendamento.data_visita, reagendamento.horario) {
            return Err(ErroAgendamento::DataRetroativa);
        }

        if self
            .repositorio_visita
            .existe_conflito_de_horario(
                novo_tecnico,
                reagendamento.data_visita,
                reagendamento.horario,
                Some(id_visita),
            )
            .await?
        {
            return Err(ErroAgendamento::ConflitoDeHorario);
        }

        self.repositorio_visita
            .reagendar(
                id_visita,
                reagendamento.data_visita,
                reagendamento.horario,
                novo_tecnico,
            )
            .await?;

        let _ = self
            .auditoria
            .registrar(
                Some(logado.id),
                "visita",
                id_visita,
                acao::VISITA_REAGENDADA,
                Some(&format!(
                    "Visita reagendada de {} {} para {} {}. Tecnico alterado: {}",
                    visita.data_visita,
                    visita.horario,
                    reagendamento.data_visita,
                    reagendamento.horario,
                    if novo_tecnico == visita.id_tecnico {
                        "nao"
                    } else {
                        "sim"
                    }
                )),
                None,
                None,
            )
            .await;

        if let Some(notificador) = &self.notificador {
            let _ = notificador.avisar_mudanca_agenda(novo_tecnico).await;
        }

        self.repositorio_visita
            .buscar_por_id(id_visita)
            .await?
            .ok_or(ErroAgendamento::NaoEncontrado)
    }

    /// Detalha um agendamento, respeitando a visibilidade do perfil (RN05/RN06).
    ///
    /// Para nao revelar a existencia de agendas alheias, o tecnico recebe
    /// o mesmo erro de "nao encontrado" ao tentar abrir visita de outro.
    pub async fn detalhar_visita(
        &self,
        id_usuario: Uuid,
        id_visita: Uuid,
    ) -> Result<DetalheVisita, ErroAgendamento> {
        let logado = self.usuario_ativo(id_usuario).await?;

        let visita = self
            .repositorio_visita
            .buscar_por_id(id_visita)
            .await?
            .ok_or(ErroAgendamento::NaoEncontrado)?;

        if !self.pode_gerenciar(&logado, &visita).await? {
            return Err(ErroAgendamento::NaoEncontrado);
        }

        let familia = self
            .repositorio_familia
            .buscar_por_id(visita.id_familia)
            .await?
            .ok_or(ErroAgendamento::NaoEncontrado)?;

        Ok(DetalheVisita { visita, familia })
    }

    /// Lista as visitas conforme o perfil (RN05/RN06).
    ///
    /// O tecnico enxerga somente a propria agenda; o coordenador, apenas as
    /// visitas da propria unidade, filtrando livremente por tecnico, familia,
    /// status e periodo dentro dessa visao.
    pub async fn listar_visitas(
        &self,
        id_usuario: Uuid,
        mut filtro: FiltroVisita,
    ) -> Result<Vec<Visita>, ErroAgendamento> {
        let logado = self.usuario_ativo(id_usuario).await?;

        match logado.perfil {
            PerfilUsuario::Coordenador => filtro.id_unidade = Some(logado.id_unidade),
            PerfilUsuario::Tecnico => filtro.id_tecnico = Some(logado.id),
        }

        Ok(self.repositorio_visita.listar(&filtro).await?)
    }

    /// Carrega o usuario autenticado e exige conta ativa.
    async fn usuario_ativo(&self, id: Uuid) -> Result<Usuario, ErroAgendamento> {
        let usuario = self
            .repositorio_usuario
            .buscar_por_id(id)
            .await?
            .ok_or(ErroAgendamento::NaoEncontrado)?;

        if !usuario.esta_ativo() {
            return Err(ErroAgendamento::SemPermissao);
        }
        Ok(usuario)
    }

    /// Valida se o agendador pode delegar ao tecnico na unidade da familia.
    fn validar_responsavel(
        &self,
        agendador: &Usuario,
        tecnico: &Usuario,
        id_unidade_familia: Uuid,
    ) -> Result<(), ErroAgendamento> {
        match agendador.perfil {
            PerfilUsuario::Coordenador => {
                if agendador.id_unidade != id_unidade_familia
                    || tecnico.id_unidade != id_unidade_familia
                {
                    return Err(ErroAgendamento::TecnicoInvalido);
                }
                if tecnico.perfil != PerfilUsuario::Tecnico || !tecnico.esta_ativo() {
                    return Err(ErroAgendamento::TecnicoInvalido);
                }
                Ok(())
            }
            PerfilUsuario::Tecnico => {
                if tecnico.id != agendador.id {
                    return Err(ErroAgendamento::SemPermissao);
                }
                if tecnico.id_unidade != id_unidade_familia {
                    return Err(ErroAgendamento::TecnicoInvalido);
                }
                Ok(())
            }
        }
    }

    /// Coordenador gerencia as agendas da propria unidade; tecnico so a propria.
    async fn pode_gerenciar(
        &self,
        logado: &Usuario,
        visita: &Visita,
    ) -> Result<bool, ErroAgendamento> {
        match logado.perfil {
            PerfilUsuario::Coordenador => {
                let familia = self
                    .repositorio_familia
                    .buscar_por_id(visita.id_familia)
                    .await?
                    .ok_or(ErroAgendamento::NaoEncontrado)?;
                Ok(familia.id_unidade == logado.id_unidade)
            }
            PerfilUsuario::Tecnico => Ok(visita.id_tecnico == logado.id),
        }
    }
}
