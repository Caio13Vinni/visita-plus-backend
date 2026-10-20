// =============================================================================
// UC06/RF17 — Encaminhamentos da visita
// -----------------------------------------------------------------------------
// Regras aplicadas:
//   - Somente o tecnico responsavel pela visita registra encaminhamentos,
//     durante o atendimento (visita 'Em andamento' — RN05/RN06);
//   - Um ou mais encaminhamentos por visita, com destino e motivo
//     obrigatorios (RF17);
//   - Listagem conforme o perfil: tecnico enxerga a propria agenda;
//     coordenador, a propria unidade;
//   - Conclusao do encaminhamento pelo tecnico responsavel ou pelo
//     coordenador da unidade, apenas enquanto 'Aberto';
//   - Auditoria nas operacoes criticas (RN22).
// =============================================================================

use std::sync::Arc;
use uuid::Uuid;

use crate::dominio::entidades::encaminhamento::Encaminhamento;
use crate::dominio::entidades::usuario::Usuario;
use crate::dominio::entidades::visita::Visita;
use crate::dominio::enums::{PerfilUsuario, StatusEncaminhamento, StatusVisita};
use crate::dominio::erros::ErroEncaminhamento;
use crate::infraestrutura::auditoria::{ServicoAuditoria, acao};
use crate::infraestrutura::repositorios::encaminhamento_repositorio::RepositorioEncaminhamento;
use crate::infraestrutura::repositorios::familia_repositorio::RepositorioFamilia;
use crate::infraestrutura::repositorios::usuario_repositorio::RepositorioUsuario;
use crate::infraestrutura::repositorios::visita_repositorio::RepositorioVisita;

/// Orquestrador do caso de uso de encaminhamentos (UC06/RF17).
#[derive(Clone)]
pub struct CasoDeUsoEncaminhamentos {
    repositorio_encaminhamento: Arc<dyn RepositorioEncaminhamento>,
    repositorio_visita: Arc<dyn RepositorioVisita>,
    repositorio_familia: Arc<dyn RepositorioFamilia>,
    repositorio_usuario: Arc<dyn RepositorioUsuario>,
    auditoria: ServicoAuditoria,
}

impl CasoDeUsoEncaminhamentos {
    /// Cria o caso de uso com suas dependencias.
    pub fn novo(
        repositorio_encaminhamento: Arc<dyn RepositorioEncaminhamento>,
        repositorio_visita: Arc<dyn RepositorioVisita>,
        repositorio_familia: Arc<dyn RepositorioFamilia>,
        repositorio_usuario: Arc<dyn RepositorioUsuario>,
        auditoria: ServicoAuditoria,
    ) -> Self {
        Self {
            repositorio_encaminhamento,
            repositorio_visita,
            repositorio_familia,
            repositorio_usuario,
            auditoria,
        }
    }

    /// Registra um encaminhamento durante o atendimento da visita (RF17).
    ///
    /// Exigencias: tecnico responsavel pela visita e visita 'Em andamento'.
    pub async fn registrar_encaminhamento(
        &self,
        id_usuario: Uuid,
        id_visita: Uuid,
        destino: &str,
        motivo: &str,
    ) -> Result<Encaminhamento, ErroEncaminhamento> {
        let logado = self.usuario_ativo(id_usuario).await?;

        if logado.perfil != PerfilUsuario::Tecnico {
            return Err(ErroEncaminhamento::SemPermissao);
        }

        if destino.trim().is_empty() {
            return Err(ErroEncaminhamento::DestinoObrigatorio);
        }

        if motivo.trim().is_empty() {
            return Err(ErroEncaminhamento::MotivoObrigatorio);
        }

        let visita = self
            .repositorio_visita
            .buscar_por_id(id_visita)
            .await?
            .ok_or(ErroEncaminhamento::NaoEncontrado)?;

        if visita.id_tecnico != logado.id {
            return Err(ErroEncaminhamento::NaoEncontrado);
        }

        if visita.status != StatusVisita::EmAndamento {
            return Err(ErroEncaminhamento::VisitaInvalida);
        }

        let encaminhamento = Encaminhamento {
            id: Uuid::new_v4(),
            id_visita: visita.id,
            destino: destino.trim().to_string(),
            motivo: motivo.trim().to_string(),
            data_encaminhamento: chrono::Utc::now(),
            status: StatusEncaminhamento::Aberto,
            registrado_por: logado.id,
            criado_em: chrono::Utc::now(),
        };

        let criado = self
            .repositorio_encaminhamento
            .criar(&encaminhamento)
            .await?;

        let _ = self
            .auditoria
            .registrar(
                Some(logado.id),
                "encaminhamento",
                criado.id,
                acao::ENCAMINHAMENTO_REGISTRADO,
                Some(&format!(
                    "Encaminhamento para {} registrado na visita {}",
                    criado.destino, criado.id_visita
                )),
                None,
                None,
            )
            .await;

        Ok(criado)
    }

    /// Lista os encaminhamentos de uma visita, conforme o perfil (RN05/RN06).
    pub async fn listar_encaminhamentos(
        &self,
        id_usuario: Uuid,
        id_visita: Uuid,
    ) -> Result<Vec<Encaminhamento>, ErroEncaminhamento> {
        let logado = self.usuario_ativo(id_usuario).await?;

        let visita = self
            .repositorio_visita
            .buscar_por_id(id_visita)
            .await?
            .ok_or(ErroEncaminhamento::NaoEncontrado)?;

        if !self.pode_consultar(&logado, &visita).await? {
            return Err(ErroEncaminhamento::NaoEncontrado);
        }

        Ok(self
            .repositorio_encaminhamento
            .listar_por_visita(visita.id)
            .await?)
    }

    /// Conclui um encaminhamento 'Aberto' (tecnico responsavel ou
    /// coordenador da unidade).
    pub async fn concluir_encaminhamento(
        &self,
        id_usuario: Uuid,
        id_encaminhamento: Uuid,
    ) -> Result<Encaminhamento, ErroEncaminhamento> {
        let logado = self.usuario_ativo(id_usuario).await?;

        let encaminhamento = self
            .repositorio_encaminhamento
            .buscar_por_id(id_encaminhamento)
            .await?
            .ok_or(ErroEncaminhamento::NaoEncontrado)?;

        if !encaminhamento.status.pode_ser_concluido() {
            return Err(ErroEncaminhamento::EstadoInvalido);
        }

        let visita = self
            .repositorio_visita
            .buscar_por_id(encaminhamento.id_visita)
            .await?
            .ok_or(ErroEncaminhamento::NaoEncontrado)?;

        let permitido = match logado.perfil {
            PerfilUsuario::Coordenador => {
                self.familia_na_unidade(&logado, visita.id_familia).await?
            }
            PerfilUsuario::Tecnico => visita.id_tecnico == logado.id,
        };

        if !permitido {
            return Err(ErroEncaminhamento::NaoEncontrado);
        }

        self.repositorio_encaminhamento
            .concluir(encaminhamento.id)
            .await?;

        let _ = self
            .auditoria
            .registrar(
                Some(logado.id),
                "encaminhamento",
                encaminhamento.id,
                acao::ENCAMINHAMENTO_CONCLUIDO,
                Some("Encaminhamento concluido"),
                None,
                None,
            )
            .await;

        self.repositorio_encaminhamento
            .buscar_por_id(encaminhamento.id)
            .await?
            .ok_or(ErroEncaminhamento::NaoEncontrado)
    }

    /// Carrega o usuario autenticado e exige conta ativa.
    async fn usuario_ativo(&self, id: Uuid) -> Result<Usuario, ErroEncaminhamento> {
        let usuario = self
            .repositorio_usuario
            .buscar_por_id(id)
            .await?
            .ok_or(ErroEncaminhamento::NaoEncontrado)?;

        if !usuario.esta_ativo() {
            return Err(ErroEncaminhamento::SemPermissao);
        }
        Ok(usuario)
    }

    /// Tecnico consulta a propria agenda; coordenador, a propria unidade.
    async fn pode_consultar(
        &self,
        logado: &Usuario,
        visita: &Visita,
    ) -> Result<bool, ErroEncaminhamento> {
        match logado.perfil {
            PerfilUsuario::Tecnico => Ok(visita.id_tecnico == logado.id),
            PerfilUsuario::Coordenador => self.familia_na_unidade(logado, visita.id_familia).await,
        }
    }

    /// Indica se a familia da visita pertence a unidade do coordenador.
    async fn familia_na_unidade(
        &self,
        logado: &Usuario,
        id_familia: Uuid,
    ) -> Result<bool, ErroEncaminhamento> {
        let familia = self.repositorio_familia.buscar_por_id(id_familia).await?;
        Ok(familia
            .map(|f| f.id_unidade == logado.id_unidade)
            .unwrap_or(false))
    }
}
