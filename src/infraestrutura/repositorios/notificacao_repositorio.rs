// =============================================================================
// Repositorio de notificacoes — contrato (trait) e implementacao PostgreSQL
// (UC10). Cobre a tabela `notificacao` (RN21).
// =============================================================================

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::dominio::entidades::notificacao::Notificacao;
use crate::dominio::enums::TipoNotificacao;

/// Contrato de persistencia da notificacao (UC10).
#[async_trait]
pub trait RepositorioNotificacao: Send + Sync {
    /// Persiste uma nova notificacao e devolve a entidade criada.
    async fn criar(&self, notificacao: &Notificacao) -> Result<Notificacao, sqlx::Error>;

    /// Busca a notificacao pelo identificador.
    async fn buscar_por_id(&self, id: Uuid) -> Result<Option<Notificacao>, sqlx::Error>;

    /// Lista as notificacoes de um destinatario, da mais recente para a mais
    /// antiga. `somente_nao_lidas` restringe o resultado as pendentes.
    async fn listar_por_destinatario(
        &self,
        id_usuario_destino: Uuid,
        somente_nao_lidas: bool,
    ) -> Result<Vec<Notificacao>, sqlx::Error>;

    /// Marca a notificacao como lida, registrando o horario.
    async fn marcar_lida(&self, id: Uuid, lida_em: DateTime<Utc>) -> Result<(), sqlx::Error>;
}

/// Implementacao concreta do repositorio de notificacoes sobre o PostgreSQL.
#[derive(Clone)]
pub struct RepositorioNotificacaoPostgres {
    pool: PgPool,
}

impl RepositorioNotificacaoPostgres {
    /// Cria o repositorio com o pool fornecido.
    pub fn novo(pool: PgPool) -> Self {
        Self { pool }
    }
}

const COLUNAS_NOTIFICACAO: &str = "id_notificacao, id_usuario_destino, titulo, mensagem,
    tipo, lida, lida_em, data_hora";

#[async_trait]
impl RepositorioNotificacao for RepositorioNotificacaoPostgres {
    async fn criar(&self, notificacao: &Notificacao) -> Result<Notificacao, sqlx::Error> {
        let linha = sqlx::query_as::<_, NotificacaoLinha>(
            "INSERT INTO notificacao
                (id_notificacao, id_usuario_destino, titulo, mensagem, tipo, lida, lida_em)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             RETURNING id_notificacao, id_usuario_destino, titulo, mensagem, tipo, lida,
                       lida_em, data_hora",
        )
        .bind(notificacao.id)
        .bind(notificacao.id_usuario_destino)
        .bind(&notificacao.titulo)
        .bind(&notificacao.mensagem)
        .bind(notificacao.tipo)
        .bind(notificacao.lida)
        .bind(notificacao.lida_em)
        .fetch_one(&self.pool)
        .await?;
        Ok(linha.para_notificacao())
    }

    async fn buscar_por_id(&self, id: Uuid) -> Result<Option<Notificacao>, sqlx::Error> {
        sqlx::query_as::<_, NotificacaoLinha>(&format!(
            "SELECT {COLUNAS_NOTIFICACAO} FROM notificacao WHERE id_notificacao = $1"
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map(|linha| linha.map(NotificacaoLinha::para_notificacao))
    }

    async fn listar_por_destinatario(
        &self,
        id_usuario_destino: Uuid,
        somente_nao_lidas: bool,
    ) -> Result<Vec<Notificacao>, sqlx::Error> {
        let linhas = if somente_nao_lidas {
            sqlx::query_as::<_, NotificacaoLinha>(&format!(
                "SELECT {COLUNAS_NOTIFICACAO} FROM notificacao
                  WHERE id_usuario_destino = $1 AND lida = FALSE
                  ORDER BY data_hora DESC"
            ))
            .bind(id_usuario_destino)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query_as::<_, NotificacaoLinha>(&format!(
                "SELECT {COLUNAS_NOTIFICACAO} FROM notificacao
                  WHERE id_usuario_destino = $1
                  ORDER BY data_hora DESC"
            ))
            .bind(id_usuario_destino)
            .fetch_all(&self.pool)
            .await?
        };

        Ok(linhas
            .into_iter()
            .map(NotificacaoLinha::para_notificacao)
            .collect())
    }

    async fn marcar_lida(&self, id: Uuid, lida_em: DateTime<Utc>) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE notificacao SET lida = TRUE, lida_em = $2 WHERE id_notificacao = $1")
            .bind(id)
            .bind(lida_em)
            .execute(&self.pool)
            .await
            .map(|_| ())
    }
}

/// Linha crua de `notificacao`, convertida para a entidade de dominio.
#[derive(sqlx::FromRow)]
struct NotificacaoLinha {
    id_notificacao: Uuid,
    id_usuario_destino: Uuid,
    titulo: String,
    mensagem: String,
    tipo: TipoNotificacao,
    lida: bool,
    lida_em: Option<DateTime<Utc>>,
    data_hora: DateTime<Utc>,
}

impl NotificacaoLinha {
    fn para_notificacao(self) -> Notificacao {
        Notificacao {
            id: self.id_notificacao,
            id_usuario_destino: self.id_usuario_destino,
            titulo: self.titulo,
            mensagem: self.mensagem,
            tipo: self.tipo,
            lida: self.lida,
            lida_em: self.lida_em,
            data_hora: self.data_hora,
        }
    }
}
