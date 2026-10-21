// =============================================================================
// Repositorio de sessoes — contrato (trait) e implementacao PostgreSQL
// (UC01/RF04). Cobre a tabela `sessao` do MER.
// =============================================================================

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::dominio::entidades::sessao::Sessao;

/// Contrato de persistencia das sessoes de acesso (UC01/RF04).
#[async_trait]
pub trait RepositorioSessao: Send + Sync {
    /// Persiste uma nova sessao ativa e devolve o registro criado.
    async fn criar(
        &self,
        id_sessao: Uuid,
        id_usuario: Uuid,
        token_jwt: &str,
        expira_em: DateTime<Utc>,
        ip_origem: Option<&str>,
        dispositivo: Option<&str>,
    ) -> Result<Sessao, sqlx::Error>;

    /// Busca a sessao pelo identificador.
    async fn buscar_por_id(&self, id_sessao: Uuid) -> Result<Option<Sessao>, sqlx::Error>;

    /// Encerra a sessao manualmente, marcando o momento do encerramento.
    async fn encerrar(&self, id_sessao: Uuid) -> Result<(), sqlx::Error>;

    /// Encerra todas as sessoes ativas do usuario (RNF07/RF03): usado na
    /// redefinicao de senha por recuperacao, para invalidar acessos.
    async fn encerrar_todas_do_usuario(&self, id_usuario: Uuid) -> Result<(), sqlx::Error>;

    /// Prorroga a expiracao da sessao (renovacao por atividade, RNF07).
    async fn estender_expiracao(
        &self,
        id_sessao: Uuid,
        nova_expiracao: DateTime<Utc>,
    ) -> Result<(), sqlx::Error>;
}

/// Implementacao concreta do repositorio de sessoes sobre o PostgreSQL.
#[derive(Clone)]
pub struct RepositorioSessaoPostgres {
    pool: PgPool,
}

impl RepositorioSessaoPostgres {
    /// Cria o repositorio com o pool fornecido.
    pub fn novo(pool: PgPool) -> Self {
        Self { pool }
    }
}

const COLUNAS_SESSAO: &str = "id_sessao, id_usuario, token_jwt, criado_em,
    expira_em, encerrada_em, ip_origem, dispositivo";

#[async_trait]
impl RepositorioSessao for RepositorioSessaoPostgres {
    async fn criar(
        &self,
        id_sessao: Uuid,
        id_usuario: Uuid,
        token_jwt: &str,
        expira_em: DateTime<Utc>,
        ip_origem: Option<&str>,
        dispositivo: Option<&str>,
    ) -> Result<Sessao, sqlx::Error> {
        let linha = sqlx::query_as::<_, SessaoLinha>(&format!(
            "INSERT INTO sessao
                (id_sessao, id_usuario, token_jwt, expira_em, ip_origem, dispositivo)
             VALUES ($1, $2, $3, $4, $5, $6)
             RETURNING {COLUNAS_SESSAO}"
        ))
        .bind(id_sessao)
        .bind(id_usuario)
        .bind(token_jwt)
        .bind(expira_em)
        .bind(ip_origem)
        .bind(dispositivo)
        .fetch_one(&self.pool)
        .await?;
        Ok(linha.para_sessao())
    }

    async fn buscar_por_id(&self, id_sessao: Uuid) -> Result<Option<Sessao>, sqlx::Error> {
        sqlx::query_as::<_, SessaoLinha>(&format!(
            "SELECT {COLUNAS_SESSAO} FROM sessao WHERE id_sessao = $1"
        ))
        .bind(id_sessao)
        .fetch_optional(&self.pool)
        .await
        .map(|linha| linha.map(SessaoLinha::para_sessao))
    }

    async fn encerrar(&self, id_sessao: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE sessao SET encerrada_em = now() WHERE id_sessao = $1")
            .bind(id_sessao)
            .execute(&self.pool)
            .await
            .map(|_| ())
    }

    async fn encerrar_todas_do_usuario(&self, id_usuario: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE sessao SET encerrada_em = now()
              WHERE id_usuario = $1 AND encerrada_em IS NULL AND expira_em > now()",
        )
        .bind(id_usuario)
        .execute(&self.pool)
        .await
        .map(|_| ())
    }

    async fn estender_expiracao(
        &self,
        id_sessao: Uuid,
        nova_expiracao: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE sessao SET expira_em = $2 WHERE id_sessao = $1")
            .bind(id_sessao)
            .bind(nova_expiracao)
            .execute(&self.pool)
            .await
            .map(|_| ())
    }
}

/// Linha crua de `sessao`, convertida para a entidade de dominio.
#[derive(sqlx::FromRow)]
struct SessaoLinha {
    id_sessao: Uuid,
    id_usuario: Uuid,
    token_jwt: String,
    criado_em: chrono::DateTime<chrono::Utc>,
    expira_em: chrono::DateTime<chrono::Utc>,
    encerrada_em: Option<chrono::DateTime<chrono::Utc>>,
    ip_origem: Option<String>,
    dispositivo: Option<String>,
}

impl SessaoLinha {
    fn para_sessao(self) -> Sessao {
        Sessao {
            id: self.id_sessao,
            id_usuario: self.id_usuario,
            token_jwt: self.token_jwt,
            criado_em: self.criado_em,
            expira_em: self.expira_em,
            encerrada_em: self.encerrada_em,
            ip_origem: self.ip_origem,
            dispositivo: self.dispositivo,
        }
    }
}
