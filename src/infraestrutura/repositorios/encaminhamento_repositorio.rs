// =============================================================================
// Repositorio de encaminhamentos — contrato (trait) e implementacao
// PostgreSQL (UC06/RF17). Cobre a tabela `encaminhamento` do MER.
// =============================================================================

use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::dominio::entidades::encaminhamento::Encaminhamento;
use crate::dominio::enums::StatusEncaminhamento;

/// Contrato de persistencia do encaminhamento (UC06/RF17).
#[async_trait]
pub trait RepositorioEncaminhamento: Send + Sync {
    /// Persiste um novo encaminhamento e devolve a entidade criada.
    async fn criar(&self, encaminhamento: &Encaminhamento) -> Result<Encaminhamento, sqlx::Error>;

    /// Busca o encaminhamento pelo identificador.
    async fn buscar_por_id(&self, id: Uuid) -> Result<Option<Encaminhamento>, sqlx::Error>;

    /// Lista os encaminhamentos de uma visita, em ordem cronologica.
    async fn listar_por_visita(&self, id_visita: Uuid) -> Result<Vec<Encaminhamento>, sqlx::Error>;

    /// Marca o encaminhamento como concluido.
    async fn concluir(&self, id: Uuid) -> Result<(), sqlx::Error>;
}

/// Implementacao concreta do repositorio de encaminhamentos sobre o PostgreSQL.
#[derive(Clone)]
pub struct RepositorioEncaminhamentoPostgres {
    pool: PgPool,
}

impl RepositorioEncaminhamentoPostgres {
    /// Cria o repositorio com o pool fornecido.
    pub fn novo(pool: PgPool) -> Self {
        Self { pool }
    }
}

const COLUNAS_ENCAMINHAMENTO: &str = "id_encaminhamento, id_visita, destino, motivo,
    data_encaminhamento, status, registrado_por, criado_em";

#[async_trait]
impl RepositorioEncaminhamento for RepositorioEncaminhamentoPostgres {
    async fn criar(&self, encaminhamento: &Encaminhamento) -> Result<Encaminhamento, sqlx::Error> {
        let linha = sqlx::query_as::<_, EncaminhamentoLinha>(
            "INSERT INTO encaminhamento
                (id_encaminhamento, id_visita, destino, motivo, data_encaminhamento,
                 status, registrado_por)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             RETURNING id_encaminhamento, id_visita, destino, motivo, data_encaminhamento,
                       status, registrado_por, criado_em",
        )
        .bind(encaminhamento.id)
        .bind(encaminhamento.id_visita)
        .bind(&encaminhamento.destino)
        .bind(&encaminhamento.motivo)
        .bind(encaminhamento.data_encaminhamento)
        .bind(encaminhamento.status)
        .bind(encaminhamento.registrado_por)
        .fetch_one(&self.pool)
        .await?;
        Ok(linha.para_encaminhamento())
    }

    async fn buscar_por_id(&self, id: Uuid) -> Result<Option<Encaminhamento>, sqlx::Error> {
        sqlx::query_as::<_, EncaminhamentoLinha>(&format!(
            "SELECT {COLUNAS_ENCAMINHAMENTO} FROM encaminhamento WHERE id_encaminhamento = $1"
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map(|linha| linha.map(EncaminhamentoLinha::para_encaminhamento))
    }

    async fn listar_por_visita(&self, id_visita: Uuid) -> Result<Vec<Encaminhamento>, sqlx::Error> {
        let linhas = sqlx::query_as::<_, EncaminhamentoLinha>(&format!(
            "SELECT {COLUNAS_ENCAMINHAMENTO} FROM encaminhamento
              WHERE id_visita = $1
              ORDER BY data_encaminhamento"
        ))
        .bind(id_visita)
        .fetch_all(&self.pool)
        .await?;
        Ok(linhas
            .into_iter()
            .map(EncaminhamentoLinha::para_encaminhamento)
            .collect())
    }

    async fn concluir(&self, id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE encaminhamento SET status = 'Concluído' WHERE id_encaminhamento = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map(|_| ())
    }
}

/// Linha crua de `encaminhamento`, convertida para a entidade de dominio.
#[derive(sqlx::FromRow)]
struct EncaminhamentoLinha {
    id_encaminhamento: Uuid,
    id_visita: Uuid,
    destino: String,
    motivo: String,
    data_encaminhamento: chrono::DateTime<chrono::Utc>,
    status: StatusEncaminhamento,
    registrado_por: Uuid,
    criado_em: chrono::DateTime<chrono::Utc>,
}

impl EncaminhamentoLinha {
    fn para_encaminhamento(self) -> Encaminhamento {
        Encaminhamento {
            id: self.id_encaminhamento,
            id_visita: self.id_visita,
            destino: self.destino,
            motivo: self.motivo,
            data_encaminhamento: self.data_encaminhamento,
            status: self.status,
            registrado_por: self.registrado_por,
            criado_em: self.criado_em,
        }
    }
}
