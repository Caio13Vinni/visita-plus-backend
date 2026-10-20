// =============================================================================
// Repositorio de auditoria — leitura da trilha de acoes criticas (UC12).
// Escrita centralizada no ServicoAuditoria (RN22).
// =============================================================================

use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::dominio::entidades::auditoria::{EventoAuditoria, FiltroAuditoria};

/// Contrato de leitura da trilha de auditoria (UC12).
#[async_trait]
pub trait RepositorioAuditoria: Send + Sync {
    /// Lista os eventos conforme os filtros, da mais recente para a mais
    /// antiga. Sem filtros, retorna a trilha completa.
    async fn listar(&self, filtro: &FiltroAuditoria) -> Result<Vec<EventoAuditoria>, sqlx::Error>;
}

/// Implementacao concreta do repositorio de auditoria sobre o PostgreSQL.
#[derive(Clone)]
pub struct RepositorioAuditoriaPostgres {
    pool: PgPool,
}

impl RepositorioAuditoriaPostgres {
    /// Cria o repositorio com o pool fornecido.
    pub fn novo(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl RepositorioAuditoria for RepositorioAuditoriaPostgres {
    async fn listar(&self, filtro: &FiltroAuditoria) -> Result<Vec<EventoAuditoria>, sqlx::Error> {
        let mut sql = String::from(
            "SELECT id_auditoria, id_usuario, entidade, entidade_id, acao, descricao, data_hora
             FROM auditoria WHERE 1 = 1",
        );
        let mut condicoes = Vec::new();
        let mut posicao = 1;

        if filtro.id_usuario.is_some() {
            condicoes.push(format!("id_usuario = ${posicao}"));
            posicao += 1;
        }
        if filtro.acao.is_some() {
            condicoes.push(format!("acao = ${posicao}"));
            posicao += 1;
        }
        if filtro.entidade.is_some() {
            condicoes.push(format!("entidade = ${posicao}"));
            posicao += 1;
        }
        if filtro.entidade_id.is_some() {
            condicoes.push(format!("entidade_id = ${posicao}"));
            posicao += 1;
        }
        if filtro.data_inicio.is_some() {
            condicoes.push(format!("data_hora >= ${posicao}"));
            posicao += 1;
        }
        if filtro.data_fim.is_some() {
            condicoes.push(format!("data_hora <= ${posicao}"));
        }
        for condicao in condicoes {
            sql.push_str(" AND ");
            sql.push_str(&condicao);
        }
        sql.push_str(" ORDER BY data_hora DESC");

        let mut consulta = sqlx::query_as::<_, EventoAuditoriaLinha>(&sql);
        if let Some(id_usuario) = filtro.id_usuario {
            consulta = consulta.bind(id_usuario);
        }
        if let Some(acao) = &filtro.acao {
            consulta = consulta.bind(acao);
        }
        if let Some(entidade) = &filtro.entidade {
            consulta = consulta.bind(entidade);
        }
        if let Some(entidade_id) = filtro.entidade_id {
            consulta = consulta.bind(entidade_id);
        }
        if let Some(data_inicio) = filtro.data_inicio {
            consulta = consulta.bind(data_inicio);
        }
        if let Some(data_fim) = filtro.data_fim {
            consulta = consulta.bind(data_fim);
        }

        let linhas = consulta.fetch_all(&self.pool).await?;
        Ok(linhas
            .into_iter()
            .map(EventoAuditoriaLinha::para_evento)
            .collect())
    }
}

/// Linha crua de `auditoria`, convertida para a entidade de dominio.
#[derive(sqlx::FromRow)]
struct EventoAuditoriaLinha {
    id_auditoria: Uuid,
    id_usuario: Option<Uuid>,
    entidade: String,
    entidade_id: Uuid,
    acao: String,
    descricao: Option<String>,
    data_hora: chrono::DateTime<chrono::Utc>,
}

impl EventoAuditoriaLinha {
    fn para_evento(self) -> EventoAuditoria {
        EventoAuditoria {
            id: self.id_auditoria,
            id_usuario: self.id_usuario,
            entidade: self.entidade,
            entidade_id: self.entidade_id,
            acao: self.acao,
            descricao: self.descricao,
            data_hora: self.data_hora,
        }
    }
}
