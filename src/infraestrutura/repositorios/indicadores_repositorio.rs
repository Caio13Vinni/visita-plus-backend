// =============================================================================
// Repositorio de indicadores — leitura agregada para o UC11 (RN23).
// Nenhum dado sensivel: apenas contagens por status.
// =============================================================================

use async_trait::async_trait;
use chrono::NaiveDate;
use sqlx::PgPool;
use uuid::Uuid;

use crate::dominio::entidades::indicadores::IndicadoresOperacionais;

/// Contrato de leitura dos indicadores operacionais (UC11).
#[async_trait]
pub trait RepositorioIndicadores: Send + Sync {
    /// Calcula as quantidades da unidade no periodo: visitas por status
    /// (por data da visita) e retornos pendentes (por data do pedido).
    async fn calcular(
        &self,
        id_unidade: Uuid,
        data_inicio: NaiveDate,
        data_fim: NaiveDate,
    ) -> Result<IndicadoresOperacionais, sqlx::Error>;
}

/// Implementacao concreta do repositorio de indicadores sobre o PostgreSQL.
#[derive(Clone)]
pub struct RepositorioIndicadoresPostgres {
    pool: PgPool,
}

impl RepositorioIndicadoresPostgres {
    /// Cria o repositorio com o pool fornecido.
    pub fn novo(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl RepositorioIndicadores for RepositorioIndicadoresPostgres {
    async fn calcular(
        &self,
        id_unidade: Uuid,
        data_inicio: NaiveDate,
        data_fim: NaiveDate,
    ) -> Result<IndicadoresOperacionais, sqlx::Error> {
        let visitas = sqlx::query_as::<_, ContagemVisitas>(
            "SELECT count(*) FILTER (WHERE v.status = 'Agendada')       AS agendadas,
                    count(*) FILTER (WHERE v.status = 'Em andamento')   AS em_andamento,
                    count(*) FILTER (WHERE v.status = 'Realizada')      AS realizadas,
                    count(*) FILTER (WHERE v.status = 'Não realizada')  AS nao_realizadas,
                    count(*) FILTER (WHERE v.status = 'Cancelada')      AS canceladas
               FROM visita v
               JOIN familia f ON f.id_familia = v.id_familia
              WHERE f.id_unidade = $1
                AND v.data_visita BETWEEN $2 AND $3",
        )
        .bind(id_unidade)
        .bind(data_inicio)
        .bind(data_fim)
        .fetch_one(&self.pool)
        .await?;

        let retornos_pendentes: i64 = sqlx::query_scalar(
            "SELECT count(*)
               FROM solicitacao_retorno s
               JOIN visita v ON v.id_visita = s.id_visita
               JOIN familia f ON f.id_familia = v.id_familia
              WHERE f.id_unidade = $1
                AND s.status = 'Pendente'
                AND s.data_solicitacao::date BETWEEN $2 AND $3",
        )
        .bind(id_unidade)
        .bind(data_inicio)
        .bind(data_fim)
        .fetch_one(&self.pool)
        .await?;

        Ok(IndicadoresOperacionais {
            periodo_inicio: data_inicio,
            periodo_fim: data_fim,
            visitas_agendadas: visitas.agendadas,
            visitas_em_andamento: visitas.em_andamento,
            visitas_realizadas: visitas.realizadas,
            visitas_nao_realizadas: visitas.nao_realizadas,
            visitas_canceladas: visitas.canceladas,
            retornos_pendentes,
        })
    }
}

/// Linha agregada da contagem de visitas por status.
#[derive(sqlx::FromRow)]
struct ContagemVisitas {
    agendadas: i64,
    em_andamento: i64,
    realizadas: i64,
    nao_realizadas: i64,
    canceladas: i64,
}
