// =============================================================================
// Repositorio de visitas — contrato (trait) e implementacao PostgreSQL
// UC02 usa `existem_visitas_futuras_ativas`; UC04 usa o contrato completo
// (agendar, reagendar, cancelar, listar e validar conflito de horario).
// =============================================================================

use async_trait::async_trait;
use chrono::{NaiveDate, NaiveTime};
use sqlx::PgPool;
use uuid::Uuid;

use crate::dominio::entidades::visita::Visita;
use crate::dominio::enums::StatusVisita;

/// Filtros da listagem de visitas (agenda do tecnico e visao do coordenador).
#[derive(Debug, Clone, Default)]
pub struct FiltroVisita {
    pub id_tecnico: Option<Uuid>,
    pub id_familia: Option<Uuid>,
    /// Filtra pela unidade da familia visitada (visao do coordenador).
    pub id_unidade: Option<Uuid>,
    pub status: Option<StatusVisita>,
    pub data_inicio: Option<NaiveDate>,
    pub data_fim: Option<NaiveDate>,
}

/// Contrato de persistencia da entidade Visita (UC04).
#[async_trait]
pub trait RepositorioVisita: Send + Sync {
    /// Verifica se o tecnico possui visitas futuras em status ativo ('Agendada').
    /// Usado pelo UC02 para impedir a desativacao de tecnico com compromissos.
    async fn existem_visitas_futuras_ativas(&self, id_tecnico: Uuid) -> Result<bool, sqlx::Error>;

    /// Persiste um novo agendamento e devolve a entidade criada.
    async fn criar(&self, visita: &Visita) -> Result<Visita, sqlx::Error>;

    /// Busca um agendamento pelo identificador.
    async fn buscar_por_id(&self, id: Uuid) -> Result<Option<Visita>, sqlx::Error>;

    /// Lista as visitas conforme os filtros informados.
    async fn listar(&self, filtro: &FiltroVisita) -> Result<Vec<Visita>, sqlx::Error>;

    /// Verifica conflito de horario na agenda do tecnico (RN10).
    /// `exceto_visita` ignora a propria visita em caso de reagendamento.
    async fn existe_conflito_de_horario(
        &self,
        id_tecnico: Uuid,
        data: NaiveDate,
        horario: NaiveTime,
        exceto_visita: Option<Uuid>,
    ) -> Result<bool, sqlx::Error>;

    /// Aplica o reagendamento (nova data, horario e, opcional, tecnico).
    async fn reagendar(
        &self,
        id: Uuid,
        data: NaiveDate,
        horario: NaiveTime,
        id_tecnico: Uuid,
    ) -> Result<(), sqlx::Error>;

    /// Marca o agendamento como cancelado, com motivo obrigatorio (RN18).
    async fn cancelar(&self, id: Uuid, motivo: &str) -> Result<(), sqlx::Error>;
}

/// Implementacao concreta do repositorio de visitas sobre o PostgreSQL.
#[derive(Clone)]
pub struct RepositorioVisitaPostgres {
    pool: PgPool,
}

impl RepositorioVisitaPostgres {
    /// Cria o repositorio com o pool fornecido.
    pub fn novo(pool: PgPool) -> Self {
        Self { pool }
    }
}

const COLUNAS_VISITA: &str = "id_visita, id_familia, id_tecnico, id_agendado_por,
    id_origem_retorno, data_visita, horario, prioridade, objetivo, status,
    motivo_cancelamento, criado_em";

#[async_trait]
impl RepositorioVisita for RepositorioVisitaPostgres {
    async fn existem_visitas_futuras_ativas(&self, id_tecnico: Uuid) -> Result<bool, sqlx::Error> {
        let existe: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT 1 FROM visita
                 WHERE id_tecnico = $1
                   AND status = 'Agendada'
                   AND (data_visita > CURRENT_DATE
                        OR (data_visita = CURRENT_DATE AND horario >= CURRENT_TIME))
             )",
        )
        .bind(id_tecnico)
        .fetch_one(&self.pool)
        .await?;
        Ok(existe)
    }

    async fn criar(&self, visita: &Visita) -> Result<Visita, sqlx::Error> {
        let linha = sqlx::query_as::<_, VisitaLinha>(
            "INSERT INTO visita
                (id_familia, id_tecnico, id_agendado_por, id_origem_retorno,
                 data_visita, horario, prioridade, objetivo, status, motivo_cancelamento)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
             RETURNING id_visita, id_familia, id_tecnico, id_agendado_por,
                       id_origem_retorno, data_visita, horario, prioridade,
                       objetivo, status, motivo_cancelamento, criado_em",
        )
        .bind(visita.id_familia)
        .bind(visita.id_tecnico)
        .bind(visita.id_agendado_por)
        .bind(visita.id_origem_retorno)
        .bind(visita.data_visita)
        .bind(visita.horario)
        .bind(visita.prioridade)
        .bind(&visita.objetivo)
        .bind(visita.status)
        .bind(&visita.motivo_cancelamento)
        .fetch_one(&self.pool)
        .await?;
        Ok(linha.para_visita())
    }

    async fn buscar_por_id(&self, id: Uuid) -> Result<Option<Visita>, sqlx::Error> {
        sqlx::query_as::<_, VisitaLinha>(&format!(
            "SELECT {COLUNAS_VISITA} FROM visita WHERE id_visita = $1"
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map(|linha| linha.map(VisitaLinha::para_visita))
    }

    async fn listar(&self, filtro: &FiltroVisita) -> Result<Vec<Visita>, sqlx::Error> {
        let mut sql = String::from(
            "SELECT v.id_visita, v.id_familia, v.id_tecnico, v.id_agendado_por,
                    v.id_origem_retorno, v.data_visita, v.horario, v.prioridade,
                    v.objetivo, v.status, v.motivo_cancelamento, v.criado_em
               FROM visita v
               JOIN familia f ON f.id_familia = v.id_familia
              WHERE 1 = 1",
        );
        let mut condicoes = Vec::new();
        let mut posicao = 1;
        if filtro.id_tecnico.is_some() {
            condicoes.push(format!("v.id_tecnico = ${posicao}"));
            posicao += 1;
        }
        if filtro.id_familia.is_some() {
            condicoes.push(format!("v.id_familia = ${posicao}"));
            posicao += 1;
        }
        if filtro.id_unidade.is_some() {
            condicoes.push(format!("f.id_unidade = ${posicao}"));
            posicao += 1;
        }
        if filtro.status.is_some() {
            condicoes.push(format!("v.status = ${posicao}"));
            posicao += 1;
        }
        if filtro.data_inicio.is_some() {
            condicoes.push(format!("v.data_visita >= ${posicao}"));
            posicao += 1;
        }
        if filtro.data_fim.is_some() {
            condicoes.push(format!("v.data_visita <= ${posicao}"));
        }
        for condicao in condicoes {
            sql.push_str(" AND ");
            sql.push_str(&condicao);
        }
        sql.push_str(" ORDER BY v.data_visita, v.horario");

        let mut consulta = sqlx::query_as::<_, VisitaLinha>(&sql);
        if let Some(id_tecnico) = filtro.id_tecnico {
            consulta = consulta.bind(id_tecnico);
        }
        if let Some(id_familia) = filtro.id_familia {
            consulta = consulta.bind(id_familia);
        }
        if let Some(id_unidade) = filtro.id_unidade {
            consulta = consulta.bind(id_unidade);
        }
        if let Some(status) = filtro.status {
            consulta = consulta.bind(status);
        }
        if let Some(data_inicio) = filtro.data_inicio {
            consulta = consulta.bind(data_inicio);
        }
        if let Some(data_fim) = filtro.data_fim {
            consulta = consulta.bind(data_fim);
        }

        let linhas = consulta.fetch_all(&self.pool).await?;
        Ok(linhas.into_iter().map(VisitaLinha::para_visita).collect())
    }

    async fn existe_conflito_de_horario(
        &self,
        id_tecnico: Uuid,
        data: NaiveDate,
        horario: NaiveTime,
        exceto_visita: Option<Uuid>,
    ) -> Result<bool, sqlx::Error> {
        let existe: bool = if let Some(id) = exceto_visita {
            sqlx::query_scalar(
                "SELECT EXISTS (
                    SELECT 1 FROM visita
                     WHERE id_tecnico = $1
                       AND status = 'Agendada'
                       AND data_visita = $2
                       AND horario = $3
                       AND id_visita <> $4
                 )",
            )
            .bind(id_tecnico)
            .bind(data)
            .bind(horario)
            .bind(id)
            .fetch_one(&self.pool)
            .await?
        } else {
            sqlx::query_scalar(
                "SELECT EXISTS (
                    SELECT 1 FROM visita
                     WHERE id_tecnico = $1
                       AND status = 'Agendada'
                       AND data_visita = $2
                       AND horario = $3
                 )",
            )
            .bind(id_tecnico)
            .bind(data)
            .bind(horario)
            .fetch_one(&self.pool)
            .await?
        };
        Ok(existe)
    }

    async fn reagendar(
        &self,
        id: Uuid,
        data: NaiveDate,
        horario: NaiveTime,
        id_tecnico: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE visita SET data_visita = $1, horario = $2, id_tecnico = $3
              WHERE id_visita = $4",
        )
        .bind(data)
        .bind(horario)
        .bind(id_tecnico)
        .bind(id)
        .execute(&self.pool)
        .await
        .map(|_| ())
    }

    async fn cancelar(&self, id: Uuid, motivo: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE visita SET status = 'Cancelada', motivo_cancelamento = $1
              WHERE id_visita = $2",
        )
        .bind(motivo)
        .bind(id)
        .execute(&self.pool)
        .await
        .map(|_| ())
    }
}

/// Linha crua retornada pelo PostgreSQL, convertida para a entidade de dominio.
#[derive(sqlx::FromRow)]
struct VisitaLinha {
    id_visita: Uuid,
    id_familia: Uuid,
    id_tecnico: Uuid,
    id_agendado_por: Uuid,
    id_origem_retorno: Option<Uuid>,
    data_visita: NaiveDate,
    horario: NaiveTime,
    prioridade: crate::dominio::enums::PrioridadeVisita,
    objetivo: String,
    status: StatusVisita,
    motivo_cancelamento: Option<String>,
    criado_em: chrono::DateTime<chrono::Utc>,
}

impl VisitaLinha {
    fn para_visita(self) -> Visita {
        Visita {
            id: self.id_visita,
            id_familia: self.id_familia,
            id_tecnico: self.id_tecnico,
            id_agendado_por: self.id_agendado_por,
            id_origem_retorno: self.id_origem_retorno,
            data_visita: self.data_visita,
            horario: self.horario,
            prioridade: self.prioridade,
            objetivo: self.objetivo,
            status: self.status,
            motivo_cancelamento: self.motivo_cancelamento,
            criado_em: self.criado_em,
        }
    }
}
