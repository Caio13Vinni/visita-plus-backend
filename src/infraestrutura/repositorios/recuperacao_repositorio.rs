// =============================================================================
// Repositorio de recuperacao de acesso — contrato (trait) e implementacao
// PostgreSQL (UC01/RF03). Cobre a tabela `recuperacao_acesso` do MER.
// =============================================================================

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::dominio::entidades::recuperacao_acesso::RecuperacaoAcesso;

/// Contrato de persistencia da recuperacao de acesso (UC01/RF03).
#[async_trait]
pub trait RepositorioRecuperacao: Send + Sync {
    /// Persiste um novo codigo de recuperacao e devolve o registro criado.
    async fn criar(
        &self,
        id_usuario: Uuid,
        codigo: &str,
        expira_em: DateTime<Utc>,
    ) -> Result<RecuperacaoAcesso, sqlx::Error>;

    /// Busca o codigo ativo (nao usado e nao expirado) mais recente do
    /// usuario para o codigo informado.
    async fn buscar_ativa_por_codigo(
        &self,
        id_usuario: Uuid,
        codigo: &str,
    ) -> Result<Option<RecuperacaoAcesso>, sqlx::Error>;

    /// Marca o codigo como usado, impedindo o reuso (RF03).
    async fn marcar_como_usada(&self, id_recuperacao: Uuid) -> Result<(), sqlx::Error>;

    /// Invalida todos os codigos pendentes do usuario (nova solicitacao).
    async fn invalidar_pendentes_do_usuario(&self, id_usuario: Uuid) -> Result<(), sqlx::Error>;
}

/// Implementacao concreta do repositorio de recuperacao sobre o PostgreSQL.
#[derive(Clone)]
pub struct RepositorioRecuperacaoPostgres {
    pool: PgPool,
}

impl RepositorioRecuperacaoPostgres {
    /// Cria o repositorio com o pool fornecido.
    pub fn novo(pool: PgPool) -> Self {
        Self { pool }
    }
}

const COLUNAS_RECUPERACAO: &str = "id_recuperacao, id_usuario, codigo,
    expira_em, usado_em, criado_em";

#[async_trait]
impl RepositorioRecuperacao for RepositorioRecuperacaoPostgres {
    async fn criar(
        &self,
        id_usuario: Uuid,
        codigo: &str,
        expira_em: DateTime<Utc>,
    ) -> Result<RecuperacaoAcesso, sqlx::Error> {
        let linha = sqlx::query_as::<_, RecuperacaoLinha>(
            "INSERT INTO recuperacao_acesso (id_usuario, codigo, expira_em)
             VALUES ($1, $2, $3)
             RETURNING id_recuperacao, id_usuario, codigo, expira_em, usado_em, criado_em",
        )
        .bind(id_usuario)
        .bind(codigo)
        .bind(expira_em)
        .fetch_one(&self.pool)
        .await?;
        Ok(linha.para_recuperacao())
    }

    async fn buscar_ativa_por_codigo(
        &self,
        id_usuario: Uuid,
        codigo: &str,
    ) -> Result<Option<RecuperacaoAcesso>, sqlx::Error> {
        sqlx::query_as::<_, RecuperacaoLinha>(&format!(
            "SELECT {COLUNAS_RECUPERACAO} FROM recuperacao_acesso
              WHERE id_usuario = $1 AND codigo = $2
                AND usado_em IS NULL AND expira_em > now()
              ORDER BY criado_em DESC
              LIMIT 1"
        ))
        .bind(id_usuario)
        .bind(codigo)
        .fetch_optional(&self.pool)
        .await
        .map(|linha| linha.map(RecuperacaoLinha::para_recuperacao))
    }

    async fn marcar_como_usada(&self, id_recuperacao: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE recuperacao_acesso SET usado_em = now() WHERE id_recuperacao = $1")
            .bind(id_recuperacao)
            .execute(&self.pool)
            .await
            .map(|_| ())
    }

    async fn invalidar_pendentes_do_usuario(&self, id_usuario: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE recuperacao_acesso SET usado_em = now()
              WHERE id_usuario = $1 AND usado_em IS NULL",
        )
        .bind(id_usuario)
        .execute(&self.pool)
        .await
        .map(|_| ())
    }
}

/// Linha crua de `recuperacao_acesso`, convertida para a entidade de dominio.
#[derive(sqlx::FromRow)]
struct RecuperacaoLinha {
    id_recuperacao: Uuid,
    id_usuario: Uuid,
    codigo: String,
    expira_em: chrono::DateTime<chrono::Utc>,
    usado_em: Option<chrono::DateTime<chrono::Utc>>,
    criado_em: chrono::DateTime<chrono::Utc>,
}

impl RecuperacaoLinha {
    fn para_recuperacao(self) -> RecuperacaoAcesso {
        RecuperacaoAcesso {
            id: self.id_recuperacao,
            id_usuario: self.id_usuario,
            codigo: self.codigo,
            expira_em: self.expira_em,
            usado_em: self.usado_em,
            criado_em: self.criado_em,
        }
    }
}
