// =============================================================================
// Repositorio de solicitacoes de retorno — contrato (trait) e implementacao
// PostgreSQL (UC08). Cobre solicitacao_retorno (RN19/RN20).
// =============================================================================

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::dominio::entidades::solicitacao_retorno::SolicitacaoRetorno;
use crate::dominio::enums::{PrioridadeVisita, StatusSolicitacao};

/// Filtros da listagem de solicitacoes de retorno.
#[derive(Debug, Clone, Default)]
pub struct FiltroSolicitacao {
    pub status: Option<StatusSolicitacao>,
    /// Filtra pelo solicitante (visao do tecnico: so os proprios pedidos).
    pub solicitado_por: Option<Uuid>,
    /// Filtra pelo responsavel pela visita original (agenda do tecnico).
    pub id_tecnico: Option<Uuid>,
    /// Filtra pela unidade da familia visitada (visao do coordenador).
    pub id_unidade: Option<Uuid>,
}

/// Contrato de persistencia da solicitacao de retorno (UC08).
#[async_trait]
pub trait RepositorioSolicitacao: Send + Sync {
    /// Persiste um novo pedido e devolve a entidade criada (RN19).
    async fn criar(
        &self,
        solicitacao: &SolicitacaoRetorno,
    ) -> Result<SolicitacaoRetorno, sqlx::Error>;

    /// Busca o pedido pelo identificador.
    async fn buscar_por_id(&self, id: Uuid) -> Result<Option<SolicitacaoRetorno>, sqlx::Error>;

    /// Busca o primeiro pedido em aberto da visita ('Pendente', 'Aprovada'
    /// ou 'Agendada' — RN19/RN20), se existir.
    async fn buscar_aberta_por_visita(
        &self,
        id_visita: Uuid,
    ) -> Result<Option<SolicitacaoRetorno>, sqlx::Error>;

    /// Lista os pedidos conforme os filtros informados.
    async fn listar(
        &self,
        filtro: &FiltroSolicitacao,
    ) -> Result<Vec<SolicitacaoRetorno>, sqlx::Error>;

    /// Registra a decisao sobre o pedido: novo status, responsavel pela
    /// decisao, motivo (rejeicao) e horario (RN19).
    async fn atualizar_decisao(
        &self,
        id: Uuid,
        status: StatusSolicitacao,
        decidido_por: Uuid,
        decisao_motivo: Option<&str>,
        decidido_em: DateTime<Utc>,
    ) -> Result<(), sqlx::Error>;
}

/// Implementacao concreta do repositorio de solicitacoes sobre o PostgreSQL.
#[derive(Clone)]
pub struct RepositorioSolicitacaoPostgres {
    pool: PgPool,
}

impl RepositorioSolicitacaoPostgres {
    /// Cria o repositorio com o pool fornecido.
    pub fn novo(pool: PgPool) -> Self {
        Self { pool }
    }
}

const COLUNAS_SOLICITACAO: &str = "id_solicitacao, id_visita, motivo, prioridade,
    solicitado_por, data_solicitacao, status, decidido_por, decisao_motivo, decidido_em";

#[async_trait]
impl RepositorioSolicitacao for RepositorioSolicitacaoPostgres {
    async fn criar(
        &self,
        solicitacao: &SolicitacaoRetorno,
    ) -> Result<SolicitacaoRetorno, sqlx::Error> {
        let linha = sqlx::query_as::<_, SolicitacaoLinha>(
            "INSERT INTO solicitacao_retorno
                (id_solicitacao, id_visita, motivo, prioridade, solicitado_por,
                 data_solicitacao, status)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             RETURNING id_solicitacao, id_visita, motivo, prioridade, solicitado_por,
                       data_solicitacao, status, decidido_por, decisao_motivo,
                       decidido_em",
        )
        .bind(solicitacao.id)
        .bind(solicitacao.id_visita)
        .bind(&solicitacao.motivo)
        .bind(solicitacao.prioridade)
        .bind(solicitacao.solicitado_por)
        .bind(solicitacao.data_solicitacao)
        .bind(solicitacao.status)
        .fetch_one(&self.pool)
        .await?;
        Ok(linha.para_solicitacao())
    }

    async fn buscar_por_id(&self, id: Uuid) -> Result<Option<SolicitacaoRetorno>, sqlx::Error> {
        sqlx::query_as::<_, SolicitacaoLinha>(&format!(
            "SELECT {COLUNAS_SOLICITACAO} FROM solicitacao_retorno WHERE id_solicitacao = $1"
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map(|linha| linha.map(SolicitacaoLinha::para_solicitacao))
    }

    async fn buscar_aberta_por_visita(
        &self,
        id_visita: Uuid,
    ) -> Result<Option<SolicitacaoRetorno>, sqlx::Error> {
        sqlx::query_as::<_, SolicitacaoLinha>(&format!(
            "SELECT {COLUNAS_SOLICITACAO} FROM solicitacao_retorno
              WHERE id_visita = $1
                AND status IN ('Pendente', 'Aprovada', 'Agendada')
              ORDER BY data_solicitacao
              LIMIT 1"
        ))
        .bind(id_visita)
        .fetch_optional(&self.pool)
        .await
        .map(|linha| linha.map(SolicitacaoLinha::para_solicitacao))
    }

    async fn listar(
        &self,
        filtro: &FiltroSolicitacao,
    ) -> Result<Vec<SolicitacaoRetorno>, sqlx::Error> {
        let mut sql = String::from(
            "SELECT s.id_solicitacao, s.id_visita, s.motivo, s.prioridade,
                    s.solicitado_por, s.data_solicitacao, s.status, s.decidido_por,
                    s.decisao_motivo, s.decidido_em
               FROM solicitacao_retorno s
               JOIN visita v ON v.id_visita = s.id_visita
               JOIN familia f ON f.id_familia = v.id_familia
              WHERE 1 = 1",
        );
        let mut condicoes = Vec::new();
        let mut posicao = 1;
        if filtro.status.is_some() {
            condicoes.push(format!("s.status = ${posicao}"));
            posicao += 1;
        }
        if filtro.solicitado_por.is_some() {
            condicoes.push(format!("s.solicitado_por = ${posicao}"));
            posicao += 1;
        }
        if filtro.id_tecnico.is_some() {
            condicoes.push(format!("v.id_tecnico = ${posicao}"));
            posicao += 1;
        }
        if filtro.id_unidade.is_some() {
            condicoes.push(format!("f.id_unidade = ${posicao}"));
        }
        for condicao in condicoes {
            sql.push_str(" AND ");
            sql.push_str(&condicao);
        }
        sql.push_str(" ORDER BY s.data_solicitacao");

        let mut consulta = sqlx::query_as::<_, SolicitacaoLinha>(&sql);
        if let Some(status) = filtro.status {
            consulta = consulta.bind(status);
        }
        if let Some(solicitado_por) = filtro.solicitado_por {
            consulta = consulta.bind(solicitado_por);
        }
        if let Some(id_tecnico) = filtro.id_tecnico {
            consulta = consulta.bind(id_tecnico);
        }
        if let Some(id_unidade) = filtro.id_unidade {
            consulta = consulta.bind(id_unidade);
        }

        let linhas = consulta.fetch_all(&self.pool).await?;
        Ok(linhas
            .into_iter()
            .map(SolicitacaoLinha::para_solicitacao)
            .collect())
    }

    async fn atualizar_decisao(
        &self,
        id: Uuid,
        status: StatusSolicitacao,
        decidido_por: Uuid,
        decisao_motivo: Option<&str>,
        decidido_em: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE solicitacao_retorno
                SET status = $1, decidido_por = $2, decisao_motivo = $3,
                    decidido_em = $4
              WHERE id_solicitacao = $5",
        )
        .bind(status)
        .bind(decidido_por)
        .bind(decisao_motivo)
        .bind(decidido_em)
        .bind(id)
        .execute(&self.pool)
        .await
        .map(|_| ())
    }
}

/// Linha crua de `solicitacao_retorno`, convertida para a entidade de dominio.
#[derive(sqlx::FromRow)]
struct SolicitacaoLinha {
    id_solicitacao: Uuid,
    id_visita: Uuid,
    motivo: String,
    prioridade: PrioridadeVisita,
    solicitado_por: Uuid,
    data_solicitacao: chrono::DateTime<chrono::Utc>,
    status: StatusSolicitacao,
    decidido_por: Option<Uuid>,
    decisao_motivo: Option<String>,
    decidido_em: Option<chrono::DateTime<chrono::Utc>>,
}

impl SolicitacaoLinha {
    fn para_solicitacao(self) -> SolicitacaoRetorno {
        SolicitacaoRetorno {
            id: self.id_solicitacao,
            id_visita: self.id_visita,
            motivo: self.motivo,
            prioridade: self.prioridade,
            solicitado_por: self.solicitado_por,
            data_solicitacao: self.data_solicitacao,
            status: self.status,
            decidido_por: self.decidido_por,
            decisao_motivo: self.decisao_motivo,
            decidido_em: self.decidido_em,
        }
    }
}
