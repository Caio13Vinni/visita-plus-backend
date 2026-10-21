// =============================================================================
// Banco local SQLite — espelho offline dos dados de campo e fila de
// sincronizacao pendente (infraestrutura de sync offline/online).
//
// Simula o armazenamento do aplicativo mobile: a agenda do tecnico e
// refletida para consulta sem conexao e os registros feitos offline
// ficam em `fila_sincronizacao` ate serem enviados ao banco central.
// =============================================================================

use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::dominio::enums::{PrioridadeVisita, StatusFamilia, StatusVisita};

/// Visita refletida no banco local para consulta offline.
#[derive(Debug, Clone)]
pub struct VisitaLocal {
    pub id: Uuid,
    pub id_familia: Uuid,
    pub data_visita: NaiveDate,
    pub horario: NaiveTime,
    pub prioridade: PrioridadeVisita,
    pub objetivo: String,
    pub status: StatusVisita,
}

/// Familia refletida no banco local (somente dados necessarios a visita).
#[derive(Debug, Clone)]
pub struct FamiliaLocal {
    pub id: Uuid,
    pub codigo: String,
    pub nome_referencia: String,
    pub endereco: String,
    pub status: StatusFamilia,
}

/// Pendencia de envio lida da fila local.
#[derive(Debug, Clone)]
pub struct PendenciaLocal {
    pub id: i64,
    pub tipo: String,
    /// JSON com os dados especificos do tipo de pendencia.
    pub payload: String,
}

/// Acesso ao banco local SQLite (simula o dispositivo mobile).
#[derive(Debug, Clone)]
pub struct BancoLocal {
    pool: SqlitePool,
}

impl BancoLocal {
    /// Abre (ou cria) o arquivo SQLite do endereco informado, sem conectar
    /// ainda — a conexao e estabelecida sob demanda.
    pub fn novo(endereco: impl AsRef<str>) -> Self {
        let opcoes = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(endereco.as_ref())
            .create_if_missing(true);
        let pool = sqlx::sqlite::SqlitePoolOptions::new().connect_lazy_with(opcoes);
        Self { pool }
    }

    /// Garante o esquema local (idempotente).
    pub async fn garantir_esquema(&self) -> Result<(), sqlx::Error> {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS familia_local (
                id TEXT PRIMARY KEY,
                codigo TEXT NOT NULL,
                nome_referencia TEXT NOT NULL,
                endereco TEXT NOT NULL,
                status TEXT NOT NULL
            )",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS visita_local (
                id TEXT PRIMARY KEY,
                id_familia TEXT NOT NULL,
                data_visita TEXT NOT NULL,
                horario TEXT NOT NULL,
                prioridade TEXT NOT NULL,
                objetivo TEXT NOT NULL,
                status TEXT NOT NULL
            )",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS fila_sincronizacao (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                tipo TEXT NOT NULL,
                payload TEXT NOT NULL,
                criado_em TEXT NOT NULL,
                sincronizado_em TEXT,
                erro TEXT
            )",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS meta_sincronizacao (
                chave TEXT PRIMARY KEY,
                valor TEXT NOT NULL
            )",
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Substitui o espelho local pela agenda recebida do banco central.
    pub async fn substituir_agenda(
        &self,
        familias: &[FamiliaLocal],
        visitas: &[VisitaLocal],
    ) -> Result<(), sqlx::Error> {
        let mut transacao = self.pool.begin().await?;

        sqlx::query("DELETE FROM visita_local")
            .execute(&mut *transacao)
            .await?;
        sqlx::query("DELETE FROM familia_local")
            .execute(&mut *transacao)
            .await?;

        for familia in familias {
            sqlx::query(
                "INSERT INTO familia_local (id, codigo, nome_referencia, endereco, status)
                 VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(familia.id.to_string())
            .bind(&familia.codigo)
            .bind(&familia.nome_referencia)
            .bind(&familia.endereco)
            .bind(familia.status.como_str())
            .execute(&mut *transacao)
            .await?;
        }

        for visita in visitas {
            sqlx::query(
                "INSERT INTO visita_local
                    (id, id_familia, data_visita, horario, prioridade, objetivo, status)
                 VALUES ($1, $2, $3, $4, $5, $6, $7)",
            )
            .bind(visita.id.to_string())
            .bind(visita.id_familia.to_string())
            .bind(visita.data_visita.to_string())
            .bind(visita.horario.to_string())
            .bind(visita.prioridade.como_str())
            .bind(&visita.objetivo)
            .bind(visita.status.como_str())
            .execute(&mut *transacao)
            .await?;
        }

        transacao.commit().await
    }

    /// Enfileira uma pendencia de envio (registro feito offline).
    pub async fn enfileirar(&self, tipo: &str, payload: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO fila_sincronizacao (tipo, payload, criado_em)
             VALUES ($1, $2, $3)",
        )
        .bind(tipo)
        .bind(payload)
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Lista as pendencias ainda nao sincronizadas, em ordem de criacao.
    pub async fn listar_pendentes(&self) -> Result<Vec<PendenciaLocal>, sqlx::Error> {
        let linhas: Vec<(i64, String, String)> = sqlx::query_as(
            "SELECT id, tipo, payload FROM fila_sincronizacao
              WHERE sincronizado_em IS NULL ORDER BY id",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(linhas
            .into_iter()
            .map(|(id, tipo, payload)| PendenciaLocal { id, tipo, payload })
            .collect())
    }

    /// Marca a pendencia como enviada com sucesso.
    pub async fn marcar_sincronizado(&self, id: i64) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE fila_sincronizacao SET sincronizado_em = $2 WHERE id = $1")
            .bind(id)
            .bind(Utc::now().to_rfc3339())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Marca a pendencia com erro de envio, preservando-a na fila para
    /// inspecao (nao sera reenviada automaticamente nesta versao).
    pub async fn marcar_erro(&self, id: i64, mensagem: &str) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE fila_sincronizacao SET erro = $2 WHERE id = $1")
            .bind(id)
            .bind(mensagem)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Total de visitas no espelho local (usado em testes e diagnostico).
    pub async fn contar_visitas(&self) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar("SELECT count(*) FROM visita_local")
            .fetch_one(&self.pool)
            .await
    }

    /// Momento da ultima sincronizacao registrada no meta local.
    pub async fn registrar_ultima_sincronizacao(&self) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO meta_sincronizacao (chave, valor) VALUES ('ultima_sincronizacao', $1)
             ON CONFLICT(chave) DO UPDATE SET valor = $1",
        )
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Expoe o horario da ultima sincronizacao, quando houver.
    pub async fn ultima_sincronizacao(&self) -> Result<Option<DateTime<Utc>>, sqlx::Error> {
        let valor: Option<String> = sqlx::query_scalar(
            "SELECT valor FROM meta_sincronizacao WHERE chave = 'ultima_sincronizacao'",
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(valor
            .and_then(|texto| DateTime::parse_from_rfc3339(&texto).ok())
            .map(|data| data.with_timezone(&Utc)))
    }
}
