// =============================================================================
// UC11 — Consultar indicadores
// -----------------------------------------------------------------------------
// Regras aplicadas:
//   - Somente o coordenador consulta indicadores, e apenas da propria
//     unidade (RN05/RN06);
//   - Quantidades operacionais agregadas, sem dados sensiveis (RN23);
//   - Periodo de consulta validado; periodo sem dados devolve zeros, sem
//     indicar erro;
//   - Consulta somente leitura.
// =============================================================================

use std::sync::Arc;
use uuid::Uuid;

use crate::dominio::entidades::indicadores::IndicadoresOperacionais;
use crate::dominio::entidades::usuario::Usuario;
use crate::dominio::enums::PerfilUsuario;
use crate::dominio::erros::ErroIndicadores;
use crate::infraestrutura::repositorios::indicadores_repositorio::RepositorioIndicadores;
use crate::infraestrutura::repositorios::usuario_repositorio::RepositorioUsuario;

/// Orquestrador do caso de uso de indicadores operacionais (UC11).
#[derive(Clone)]
pub struct CasoDeUsoIndicadores {
    repositorio_indicadores: Arc<dyn RepositorioIndicadores>,
    repositorio_usuario: Arc<dyn RepositorioUsuario>,
}

impl CasoDeUsoIndicadores {
    /// Cria o caso de uso com suas dependencias.
    pub fn novo(
        repositorio_indicadores: Arc<dyn RepositorioIndicadores>,
        repositorio_usuario: Arc<dyn RepositorioUsuario>,
    ) -> Self {
        Self {
            repositorio_indicadores,
            repositorio_usuario,
        }
    }

    /// Calcula os indicadores da unidade do coordenador no periodo.
    ///
    /// O periodo sem dados devolve quantidades zero (comportamento previsto
    /// no fluxo do UC11), nao erro.
    pub async fn consultar_indicadores(
        &self,
        id_usuario: Uuid,
        data_inicio: chrono::NaiveDate,
        data_fim: chrono::NaiveDate,
    ) -> Result<IndicadoresOperacionais, ErroIndicadores> {
        let logado = self.usuario_ativo(id_usuario).await?;

        if logado.perfil != PerfilUsuario::Coordenador {
            return Err(ErroIndicadores::SemPermissao);
        }

        if data_inicio > data_fim {
            return Err(ErroIndicadores::DadosInvalidos(
                "a data inicial do periodo nao pode ser posterior a data final".into(),
            ));
        }

        Ok(self
            .repositorio_indicadores
            .calcular(logado.id_unidade, data_inicio, data_fim)
            .await?)
    }

    /// Carrega o usuario autenticado e exige conta ativa.
    async fn usuario_ativo(&self, id: Uuid) -> Result<Usuario, ErroIndicadores> {
        let usuario = self
            .repositorio_usuario
            .buscar_por_id(id)
            .await?
            .ok_or(ErroIndicadores::SemPermissao)?;

        if !usuario.esta_ativo() {
            return Err(ErroIndicadores::SemPermissao);
        }
        Ok(usuario)
    }
}
