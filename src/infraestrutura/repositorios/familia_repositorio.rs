// =============================================================================
// Repositorio de familias — contrato (trait) e implementacao PostgreSQL
// =============================================================================

use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::dominio::entidades::familia::Familia;
use crate::dominio::enums::StatusFamilia;

/// Contrato de persistencia da entidade Familia (UC03).
#[async_trait]
pub trait RepositorioFamilia: Send + Sync {
    /// Persiste uma nova familia e devolve a entidade criada.
    async fn criar(&self, familia: &Familia) -> Result<Familia, sqlx::Error>;

    /// Busca uma familia pelo identificador.
    async fn buscar_por_id(&self, id: Uuid) -> Result<Option<Familia>, sqlx::Error>;

    /// Busca uma familia pelo codigo unico.
    async fn buscar_por_codigo(&self, codigo: &str) -> Result<Option<Familia>, sqlx::Error>;

    /// Lista as familias, opcionalmente filtrando por unidade e status.
    async fn listar(
        &self,
        id_unidade: Option<Uuid>,
        status: Option<StatusFamilia>,
    ) -> Result<Vec<Familia>, sqlx::Error>;

    /// Atualiza os dados cadastrais de uma familia.
    async fn atualizar(&self, familia: &Familia) -> Result<(), sqlx::Error>;

    /// Altera o status da familia (ativa/inativa).
    async fn alterar_status(&self, id: Uuid, status: StatusFamilia) -> Result<(), sqlx::Error>;
}

/// Implementacao concreta do repositorio de familias sobre o PostgreSQL.
#[derive(Clone)]
pub struct RepositorioFamiliaPostgres {
    pool: PgPool,
}

impl RepositorioFamiliaPostgres {
    /// Cria o repositorio com o pool fornecido.
    pub fn novo(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl RepositorioFamilia for RepositorioFamiliaPostgres {
    async fn criar(&self, familia: &Familia) -> Result<Familia, sqlx::Error> {
        let linha = sqlx::query_as::<_, FamiliaLinha>(
            "INSERT INTO familia
                (id_unidade, codigo, nome_referencia, contato, endereco, status, criado_por)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             RETURNING id_familia, id_unidade, codigo, nome_referencia, contato,
                       endereco, status, criado_por, criado_em",
        )
        .bind(familia.id_unidade)
        .bind(&familia.codigo)
        .bind(&familia.nome_referencia)
        .bind(&familia.contato)
        .bind(&familia.endereco)
        .bind(familia.status)
        .bind(familia.criado_por)
        .fetch_one(&self.pool)
        .await?;
        Ok(linha.para_familia())
    }

    async fn buscar_por_id(&self, id: Uuid) -> Result<Option<Familia>, sqlx::Error> {
        sqlx::query_as::<_, FamiliaLinha>(
            "SELECT id_familia, id_unidade, codigo, nome_referencia, contato,
                    endereco, status, criado_por, criado_em
               FROM familia
              WHERE id_familia = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map(|linha| linha.map(FamiliaLinha::para_familia))
    }

    async fn buscar_por_codigo(&self, codigo: &str) -> Result<Option<Familia>, sqlx::Error> {
        sqlx::query_as::<_, FamiliaLinha>(
            "SELECT id_familia, id_unidade, codigo, nome_referencia, contato,
                    endereco, status, criado_por, criado_em
               FROM familia
              WHERE codigo = $1",
        )
        .bind(codigo)
        .fetch_optional(&self.pool)
        .await
        .map(|linha| linha.map(FamiliaLinha::para_familia))
    }

    async fn listar(
        &self,
        id_unidade: Option<Uuid>,
        status: Option<StatusFamilia>,
    ) -> Result<Vec<Familia>, sqlx::Error> {
        let mut sql = String::from(
            "SELECT id_familia, id_unidade, codigo, nome_referencia, contato,
                    endereco, status, criado_por, criado_em
               FROM familia
              WHERE 1 = 1",
        );
        if id_unidade.is_some() {
            sql.push_str(" AND id_unidade = $1");
        }
        if status.is_some() {
            sql.push_str(" AND status = $2");
        }
        sql.push_str(" ORDER BY nome_referencia");

        let mut consulta = sqlx::query_as::<_, FamiliaLinha>(&sql);
        if let Some(unidade) = id_unidade {
            consulta = consulta.bind(unidade);
        }
        if let Some(status_filtro) = status {
            consulta = consulta.bind(status_filtro);
        }

        let linhas = consulta.fetch_all(&self.pool).await?;
        Ok(linhas.into_iter().map(FamiliaLinha::para_familia).collect())
    }

    async fn atualizar(&self, familia: &Familia) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE familia
                SET codigo = $1, nome_referencia = $2, contato = $3, endereco = $4
              WHERE id_familia = $5",
        )
        .bind(&familia.codigo)
        .bind(&familia.nome_referencia)
        .bind(&familia.contato)
        .bind(&familia.endereco)
        .bind(familia.id)
        .execute(&self.pool)
        .await
        .map(|_| ())
    }

    async fn alterar_status(&self, id: Uuid, status: StatusFamilia) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE familia SET status = $1 WHERE id_familia = $2")
            .bind(status)
            .bind(id)
            .execute(&self.pool)
            .await
            .map(|_| ())
    }
}

/// Linha crua retornada pelo PostgreSQL, convertida para a entidade de dominio.
#[derive(sqlx::FromRow)]
struct FamiliaLinha {
    id_familia: Uuid,
    id_unidade: Uuid,
    codigo: String,
    nome_referencia: String,
    contato: String,
    endereco: String,
    status: StatusFamilia,
    criado_por: Option<Uuid>,
    criado_em: chrono::DateTime<chrono::Utc>,
}

impl FamiliaLinha {
    fn para_familia(self) -> Familia {
        Familia {
            id: self.id_familia,
            id_unidade: self.id_unidade,
            codigo: self.codigo,
            nome_referencia: self.nome_referencia,
            contato: self.contato,
            endereco: self.endereco,
            status: self.status,
            criado_por: self.criado_por,
            criado_em: self.criado_em,
        }
    }
}
