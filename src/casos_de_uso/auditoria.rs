// =============================================================================
// UC12 — Consultar auditoria
// -----------------------------------------------------------------------------
// Regras aplicadas:
//   - Somente o coordenador consulta a trilha (UC12); tentativa de acesso por
//     outro perfil nega o acesso e registra a tentativa (RN22);
//   - Lista as acoes criticas da mais recente para a mais antiga, com filtro
//     por periodo, usuario, acao ou item;
//   - Consulta somente leitura: o registro nao e alterado.
// =============================================================================

use std::sync::Arc;
use uuid::Uuid;

use crate::dominio::entidades::auditoria::{EventoAuditoria, FiltroAuditoria};
use crate::dominio::entidades::usuario::Usuario;
use crate::dominio::enums::PerfilUsuario;
use crate::dominio::erros::ErroAuditoria;
use crate::infraestrutura::auditoria::{ServicoAuditoria, acao};
use crate::infraestrutura::repositorios::auditoria_repositorio::RepositorioAuditoria;
use crate::infraestrutura::repositorios::usuario_repositorio::RepositorioUsuario;

/// Orquestrador do caso de uso de consulta a auditoria (UC12).
#[derive(Clone)]
pub struct CasoDeUsoAuditoria {
    repositorio_auditoria: Arc<dyn RepositorioAuditoria>,
    repositorio_usuario: Arc<dyn RepositorioUsuario>,
    auditoria: ServicoAuditoria,
}

impl CasoDeUsoAuditoria {
    /// Cria o caso de uso com suas dependencias.
    pub fn novo(
        repositorio_auditoria: Arc<dyn RepositorioAuditoria>,
        repositorio_usuario: Arc<dyn RepositorioUsuario>,
        auditoria: ServicoAuditoria,
    ) -> Self {
        Self {
            repositorio_auditoria,
            repositorio_usuario,
            auditoria,
        }
    }

    /// Lista os eventos da trilha conforme os filtros, da mais recente para
    /// a mais antiga. Somente leitura.
    pub async fn consultar_auditoria(
        &self,
        id_usuario: Uuid,
        filtro: FiltroAuditoria,
    ) -> Result<Vec<EventoAuditoria>, ErroAuditoria> {
        let logado = self.usuario_ativo(id_usuario).await?;

        if logado.perfil != PerfilUsuario::Coordenador {
            let _ = self
                .auditoria
                .registrar(
                    Some(logado.id),
                    "auditoria",
                    logado.id,
                    acao::AUDITORIA_ACESSO_NEGADO,
                    Some("Tentativa de consulta a auditoria por perfil nao coordenador"),
                    None,
                    None,
                )
                .await;
            return Err(ErroAuditoria::SemPermissao);
        }

        if let (Some(inicio), Some(fim)) = (filtro.data_inicio, filtro.data_fim)
            && inicio > fim
        {
            return Err(ErroAuditoria::DadosInvalidos(
                "a data inicial do periodo nao pode ser posterior a data final".into(),
            ));
        }

        Ok(self.repositorio_auditoria.listar(&filtro).await?)
    }

    /// Carrega o usuario autenticado e exige conta ativa.
    async fn usuario_ativo(&self, id: Uuid) -> Result<Usuario, ErroAuditoria> {
        let usuario = self
            .repositorio_usuario
            .buscar_por_id(id)
            .await?
            .ok_or(ErroAuditoria::SemPermissao)?;

        if !usuario.esta_ativo() {
            return Err(ErroAuditoria::SemPermissao);
        }
        Ok(usuario)
    }
}
