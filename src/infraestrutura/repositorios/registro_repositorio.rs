// =============================================================================
// Repositorio de registros de visita — contrato (trait) e implementacao
// PostgreSQL (UC05). Cobre registro_visita e complemento_registro (RN16/RN17).
// =============================================================================

use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::dominio::entidades::registro_visita::{ComplementoRegistro, RegistroVisita};
use crate::dominio::enums::{ResultadoRegistro, StatusRegistro};

/// Contrato de persistencia do registro da visita (UC05).
#[async_trait]
pub trait RepositorioRegistro: Send + Sync {
    /// Cria o registro em rascunho quando a visita e iniciada (RN12).
    async fn criar(
        &self,
        id_visita: Uuid,
        registrado_por: Uuid,
    ) -> Result<RegistroVisita, sqlx::Error>;

    /// Busca o registro pelo identificador.
    async fn buscar_por_id(&self, id: Uuid) -> Result<Option<RegistroVisita>, sqlx::Error>;

    /// Busca o registro pela visita (relacao 1 para 0..1 do MER).
    async fn buscar_por_visita(
        &self,
        id_visita: Uuid,
    ) -> Result<Option<RegistroVisita>, sqlx::Error>;

    /// Atualiza os dados editaveis do rascunho (RN12).
    async fn salvar_rascunho(
        &self,
        id: Uuid,
        resultado: Option<ResultadoRegistro>,
        descricao: Option<&str>,
        desfecho: Option<&str>,
        necessita_retorno: bool,
        motivo_nao_realizada: Option<&str>,
    ) -> Result<(), sqlx::Error>;

    /// Marca o registro como finalizado, com horario de termino (RN13/RN14).
    #[allow(clippy::too_many_arguments)]
    async fn finalizar(
        &self,
        id: Uuid,
        resultado: ResultadoRegistro,
        descricao: Option<&str>,
        desfecho: Option<&str>,
        necessita_retorno: bool,
        motivo_nao_realizada: Option<&str>,
        data_hora_fim: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), sqlx::Error>;

    /// Insere um complemento imutavel em registro finalizado (RN16/RN17).
    async fn criar_complemento(
        &self,
        id_registro: Uuid,
        descricao: &str,
        motivo: &str,
        registrado_por: Uuid,
    ) -> Result<ComplementoRegistro, sqlx::Error>;

    /// Lista os complementos de um registro (historico RN16/RN17).
    async fn listar_complementos(
        &self,
        id_registro: Uuid,
    ) -> Result<Vec<ComplementoRegistro>, sqlx::Error>;
}

/// Implementacao concreta do repositorio de registros sobre o PostgreSQL.
#[derive(Clone)]
pub struct RepositorioRegistroPostgres {
    pool: PgPool,
}

impl RepositorioRegistroPostgres {
    /// Cria o repositorio com o pool fornecido.
    pub fn novo(pool: PgPool) -> Self {
        Self { pool }
    }
}

const COLUNAS_REGISTRO: &str = "id_registro, id_visita, status, resultado, descricao,
    desfecho, necessita_retorno, motivo_nao_realizada, data_hora_inicio,
    data_hora_fim, registrado_por, criado_em";

#[async_trait]
impl RepositorioRegistro for RepositorioRegistroPostgres {
    async fn criar(
        &self,
        id_visita: Uuid,
        registrado_por: Uuid,
    ) -> Result<RegistroVisita, sqlx::Error> {
        let linha = sqlx::query_as::<_, RegistroLinha>(
            "INSERT INTO registro_visita
                (id_visita, status, registrado_por, data_hora_inicio)
             VALUES ($1, 'Rascunho', $2, now())
             RETURNING id_registro, id_visita, status, resultado, descricao,
                       desfecho, necessita_retorno, motivo_nao_realizada,
                       data_hora_inicio, data_hora_fim, registrado_por, criado_em",
        )
        .bind(id_visita)
        .bind(registrado_por)
        .fetch_one(&self.pool)
        .await?;
        Ok(linha.para_registro())
    }

    async fn buscar_por_id(&self, id: Uuid) -> Result<Option<RegistroVisita>, sqlx::Error> {
        sqlx::query_as::<_, RegistroLinha>(&format!(
            "SELECT {COLUNAS_REGISTRO} FROM registro_visita WHERE id_registro = $1"
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map(|linha| linha.map(RegistroLinha::para_registro))
    }

    async fn buscar_por_visita(
        &self,
        id_visita: Uuid,
    ) -> Result<Option<RegistroVisita>, sqlx::Error> {
        sqlx::query_as::<_, RegistroLinha>(&format!(
            "SELECT {COLUNAS_REGISTRO} FROM registro_visita WHERE id_visita = $1"
        ))
        .bind(id_visita)
        .fetch_optional(&self.pool)
        .await
        .map(|linha| linha.map(RegistroLinha::para_registro))
    }

    async fn salvar_rascunho(
        &self,
        id: Uuid,
        resultado: Option<ResultadoRegistro>,
        descricao: Option<&str>,
        desfecho: Option<&str>,
        necessita_retorno: bool,
        motivo_nao_realizada: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE registro_visita
                SET resultado = $1, descricao = $2, desfecho = $3,
                    necessita_retorno = $4, motivo_nao_realizada = $5
              WHERE id_registro = $6",
        )
        .bind(resultado)
        .bind(descricao)
        .bind(desfecho)
        .bind(necessita_retorno)
        .bind(motivo_nao_realizada)
        .bind(id)
        .execute(&self.pool)
        .await
        .map(|_| ())
    }

    #[allow(clippy::too_many_arguments)]
    async fn finalizar(
        &self,
        id: Uuid,
        resultado: ResultadoRegistro,
        descricao: Option<&str>,
        desfecho: Option<&str>,
        necessita_retorno: bool,
        motivo_nao_realizada: Option<&str>,
        data_hora_fim: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE registro_visita
                SET status = 'Finalizado', resultado = $1, descricao = $2,
                    desfecho = $3, necessita_retorno = $4, motivo_nao_realizada = $5,
                    data_hora_fim = $6
              WHERE id_registro = $7",
        )
        .bind(resultado)
        .bind(descricao)
        .bind(desfecho)
        .bind(necessita_retorno)
        .bind(motivo_nao_realizada)
        .bind(data_hora_fim)
        .bind(id)
        .execute(&self.pool)
        .await
        .map(|_| ())
    }

    async fn criar_complemento(
        &self,
        id_registro: Uuid,
        descricao: &str,
        motivo: &str,
        registrado_por: Uuid,
    ) -> Result<ComplementoRegistro, sqlx::Error> {
        let linha = sqlx::query_as::<_, ComplementoLinha>(
            "INSERT INTO complemento_registro
                (id_registro, descricao, motivo, registrado_por)
             VALUES ($1, $2, $3, $4)
             RETURNING id_complemento, id_registro, descricao, motivo,
                       registrado_por, criado_em",
        )
        .bind(id_registro)
        .bind(descricao)
        .bind(motivo)
        .bind(registrado_por)
        .fetch_one(&self.pool)
        .await?;
        Ok(linha.para_complemento())
    }

    async fn listar_complementos(
        &self,
        id_registro: Uuid,
    ) -> Result<Vec<ComplementoRegistro>, sqlx::Error> {
        let linhas = sqlx::query_as::<_, ComplementoLinha>(
            "SELECT id_complemento, id_registro, descricao, motivo,
                    registrado_por, criado_em
               FROM complemento_registro
              WHERE id_registro = $1
              ORDER BY criado_em",
        )
        .bind(id_registro)
        .fetch_all(&self.pool)
        .await?;
        Ok(linhas
            .into_iter()
            .map(ComplementoLinha::para_complemento)
            .collect())
    }
}

/// Linha crua de `registro_visita`, convertida para a entidade de dominio.
#[derive(sqlx::FromRow)]
struct RegistroLinha {
    id_registro: Uuid,
    id_visita: Uuid,
    status: StatusRegistro,
    resultado: Option<ResultadoRegistro>,
    descricao: Option<String>,
    desfecho: Option<String>,
    necessita_retorno: bool,
    motivo_nao_realizada: Option<String>,
    data_hora_inicio: Option<chrono::DateTime<chrono::Utc>>,
    data_hora_fim: Option<chrono::DateTime<chrono::Utc>>,
    registrado_por: Uuid,
    criado_em: chrono::DateTime<chrono::Utc>,
}

impl RegistroLinha {
    fn para_registro(self) -> RegistroVisita {
        RegistroVisita {
            id: self.id_registro,
            id_visita: self.id_visita,
            status: self.status,
            resultado: self.resultado,
            descricao: self.descricao,
            desfecho: self.desfecho,
            necessita_retorno: self.necessita_retorno,
            motivo_nao_realizada: self.motivo_nao_realizada,
            data_hora_inicio: self.data_hora_inicio,
            data_hora_fim: self.data_hora_fim,
            registrado_por: self.registrado_por,
            criado_em: self.criado_em,
        }
    }
}

/// Linha crua de `complemento_registro`, convertida para a entidade de dominio.
#[derive(sqlx::FromRow)]
struct ComplementoLinha {
    id_complemento: Uuid,
    id_registro: Uuid,
    descricao: String,
    motivo: String,
    registrado_por: Uuid,
    criado_em: chrono::DateTime<chrono::Utc>,
}

impl ComplementoLinha {
    fn para_complemento(self) -> ComplementoRegistro {
        ComplementoRegistro {
            id: self.id_complemento,
            id_registro: self.id_registro,
            descricao: self.descricao,
            motivo: self.motivo,
            registrado_por: self.registrado_por,
            criado_em: self.criado_em,
        }
    }
}
