// =============================================================================
// UC09 — Consultar historico
// -----------------------------------------------------------------------------
// Regras aplicadas:
//   - Consulta somente leitura, conforme o perfil e o vinculo (RN05/RN06):
//     tecnico enxerga somente a propria agenda; coordenador, a propria
//     unidade;
//   - Lista em ordem da mais recente para a mais antiga, com filtro por
//     periodo, status e familia;
//   - O detalhe exibe o registro, os complementos e as mudancas com autor e
//     data (RN16 a RN18); as mudancas vêm da trilha de auditoria (RN22);
//   - Tentativa de acesso fora da visibilidade nega o acesso e registra a
//     tentativa (RN22).
// =============================================================================

use std::sync::Arc;
use uuid::Uuid;

use crate::dominio::entidades::historico::DetalheHistorico;
use crate::dominio::entidades::usuario::Usuario;
use crate::dominio::entidades::visita::Visita;
use crate::dominio::enums::PerfilUsuario;
use crate::dominio::erros::ErroHistorico;
use crate::infraestrutura::auditoria::{ServicoAuditoria, acao};
use crate::infraestrutura::repositorios::familia_repositorio::RepositorioFamilia;
use crate::infraestrutura::repositorios::registro_repositorio::RepositorioRegistro;
use crate::infraestrutura::repositorios::usuario_repositorio::RepositorioUsuario;
use crate::infraestrutura::repositorios::visita_repositorio::{FiltroVisita, RepositorioVisita};

/// Orquestrador do caso de uso de consulta ao historico (UC09).
#[derive(Clone)]
pub struct CasoDeUsoHistorico {
    repositorio_visita: Arc<dyn RepositorioVisita>,
    repositorio_familia: Arc<dyn RepositorioFamilia>,
    repositorio_registro: Arc<dyn RepositorioRegistro>,
    repositorio_usuario: Arc<dyn RepositorioUsuario>,
    auditoria: ServicoAuditoria,
}

impl CasoDeUsoHistorico {
    /// Cria o caso de uso com suas dependencias.
    pub fn novo(
        repositorio_visita: Arc<dyn RepositorioVisita>,
        repositorio_familia: Arc<dyn RepositorioFamilia>,
        repositorio_registro: Arc<dyn RepositorioRegistro>,
        repositorio_usuario: Arc<dyn RepositorioUsuario>,
        auditoria: ServicoAuditoria,
    ) -> Self {
        Self {
            repositorio_visita,
            repositorio_familia,
            repositorio_registro,
            repositorio_usuario,
            auditoria,
        }
    }

    /// Lista o historico de visitas, da mais recente para a mais antiga,
    /// conforme o perfil e o vinculo (RN05/RN06).
    ///
    /// Lista vazia nao e erro: o historico pode simplesmente nao conter
    /// itens para os filtros informados.
    pub async fn consultar_historico(
        &self,
        id_usuario: Uuid,
        mut filtro: FiltroVisita,
    ) -> Result<Vec<Visita>, ErroHistorico> {
        let logado = self.usuario_ativo(id_usuario).await?;

        match logado.perfil {
            PerfilUsuario::Coordenador => filtro.id_unidade = Some(logado.id_unidade),
            PerfilUsuario::Tecnico => filtro.id_tecnico = Some(logado.id),
        }

        let mut visitas = self.repositorio_visita.listar(&filtro).await?;
        visitas.sort_by(|a, b| {
            b.data_visita
                .cmp(&a.data_visita)
                .then(b.horario.cmp(&a.horario))
        });
        Ok(visitas)
    }

    /// Detalha um item do historico: visita, registro, complementos e
    /// mudancas auditadas, com autor e data (RN16 a RN18, RN22).
    pub async fn detalhar_historico(
        &self,
        id_usuario: Uuid,
        id_visita: Uuid,
    ) -> Result<DetalheHistorico, ErroHistorico> {
        let logado = self.usuario_ativo(id_usuario).await?;

        let visita = self
            .repositorio_visita
            .buscar_por_id(id_visita)
            .await?
            .ok_or(ErroHistorico::NaoEncontrado)?;

        if !self.pode_consultar(&logado, &visita).await? {
            let _ = self
                .auditoria
                .registrar(
                    Some(logado.id),
                    "visita",
                    id_visita,
                    acao::HISTORICO_ACESSO_NEGADO,
                    Some("Tentativa de consulta de historico fora da visibilidade do perfil"),
                    None,
                    None,
                )
                .await;
            return Err(ErroHistorico::NaoEncontrado);
        }

        let registro = self
            .repositorio_registro
            .buscar_por_visita(visita.id)
            .await?;

        let complementos = match &registro {
            Some(r) => self.repositorio_registro.listar_complementos(r.id).await?,
            None => Vec::new(),
        };

        // Mudancas preservadas na auditoria: agendamento, reagendamentos,
        // cancelamentos e trocas de responsavel da visita (RN18), além do
        // inicio e da finalizacao do registro (RN22) — tudo com autor e data.
        let mut mudancas = self
            .auditoria
            .listar_por_entidade("visita", visita.id)
            .await?;
        if let Some(r) = &registro {
            mudancas.extend(
                self.auditoria
                    .listar_por_entidade("registro_visita", r.id)
                    .await?,
            );
        }
        mudancas.sort_by_key(|evento| evento.data_hora);

        Ok(DetalheHistorico {
            visita,
            registro,
            complementos,
            mudancas,
        })
    }

    /// Carrega o usuario autenticado e exige conta ativa.
    async fn usuario_ativo(&self, id: Uuid) -> Result<Usuario, ErroHistorico> {
        let usuario = self
            .repositorio_usuario
            .buscar_por_id(id)
            .await?
            .ok_or(ErroHistorico::NaoEncontrado)?;

        if !usuario.esta_ativo() {
            return Err(ErroHistorico::SemPermissao);
        }
        Ok(usuario)
    }

    /// Tecnico consulta somente a propria agenda; coordenador, visitas de
    /// familias da propria unidade (RN05/RN06).
    async fn pode_consultar(
        &self,
        logado: &Usuario,
        visita: &Visita,
    ) -> Result<bool, ErroHistorico> {
        match logado.perfil {
            PerfilUsuario::Coordenador => {
                let familia = self
                    .repositorio_familia
                    .buscar_por_id(visita.id_familia)
                    .await?;
                Ok(familia
                    .map(|f| f.id_unidade == logado.id_unidade)
                    .unwrap_or(false))
            }
            PerfilUsuario::Tecnico => Ok(visita.id_tecnico == logado.id),
        }
    }
}
