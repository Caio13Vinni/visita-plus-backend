// =============================================================================
// Repositorio de usuarios — contrato (trait) e implementacao PostgreSQL
// =============================================================================

use async_trait::async_trait;
use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::dominio::entidades::usuario::Usuario;
use crate::dominio::enums::{PerfilUsuario, StatusUsuario};

/// Contrato de persistencia da entidade Usuario.
#[async_trait]
pub trait RepositorioUsuario: Send + Sync {
    /// Busca um usuario pelo e-mail (case-insensitive).
    async fn buscar_por_email(&self, email: &str) -> Result<Option<Usuario>, sqlx::Error>;

    /// Busca um usuario pelo identificador.
    async fn buscar_por_id(&self, id: Uuid) -> Result<Option<Usuario>, sqlx::Error>;

    /// Registra o horario do ultimo acesso bem-sucedido.
    async fn registrar_ultimo_acesso(&self, id: Uuid) -> Result<(), sqlx::Error>;

    /// Atualiza o hash da senha e, opcionalmente, limpa a flag de troca obrigatoria.
    async fn trocar_senha(
        &self,
        id: Uuid,
        novo_hash: &str,
        exige_troca: bool,
    ) -> Result<(), sqlx::Error>;
}

/// Implementacao concreta do repositorio de usuarios sobre o PostgreSQL.
#[derive(Clone)]
pub struct RepositorioUsuarioPostgres {
    pool: PgPool,
}

impl RepositorioUsuarioPostgres {
    /// Cria o repositorio com o pool fornecido.
    pub fn novo(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl RepositorioUsuario for RepositorioUsuarioPostgres {
    async fn buscar_por_email(&self, email: &str) -> Result<Option<Usuario>, sqlx::Error> {
        sqlx::query_as::<_, UsuarioLinha>(
            "SELECT id_usuario, id_unidade, nome, email, senha_hash, perfil, status,
                    exige_troca_senha, ultimo_acesso_em
               FROM usuario
              WHERE lower(email) = lower($1)",
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await
        .map(|linha| linha.map(UsuarioLinha::para_usuario))
    }

    async fn buscar_por_id(&self, id: Uuid) -> Result<Option<Usuario>, sqlx::Error> {
        sqlx::query_as::<_, UsuarioLinha>(
            "SELECT id_usuario, id_unidade, nome, email, senha_hash, perfil, status,
                    exige_troca_senha, ultimo_acesso_em
               FROM usuario
              WHERE id_usuario = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map(|linha| linha.map(UsuarioLinha::para_usuario))
    }

    async fn registrar_ultimo_acesso(&self, id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE usuario SET ultimo_acesso_em = $1 WHERE id_usuario = $2")
            .bind(Utc::now())
            .bind(id)
            .execute(&self.pool)
            .await
            .map(|_| ())
    }

    async fn trocar_senha(
        &self,
        id: Uuid,
        novo_hash: &str,
        exige_troca: bool,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE usuario SET senha_hash = $1, exige_troca_senha = $2 WHERE id_usuario = $3",
        )
        .bind(novo_hash)
        .bind(exige_troca)
        .bind(id)
        .execute(&self.pool)
        .await
        .map(|_| ())
    }
}

/// Linha crua retornada pelo PostgreSQL, convertida para a entidade de dominio.
#[derive(sqlx::FromRow)]
struct UsuarioLinha {
    id_usuario: Uuid,
    id_unidade: Uuid,
    nome: String,
    email: String,
    senha_hash: String,
    perfil: PerfilUsuario,
    status: StatusUsuario,
    exige_troca_senha: bool,
    ultimo_acesso_em: Option<chrono::DateTime<chrono::Utc>>,
}

impl UsuarioLinha {
    fn para_usuario(self) -> Usuario {
        Usuario {
            id: self.id_usuario,
            id_unidade: self.id_unidade,
            nome: self.nome,
            email: self.email,
            senha_hash: self.senha_hash,
            perfil: self.perfil,
            status: self.status,
            exige_troca_senha: self.exige_troca_senha,
            ultimo_acesso_em: self.ultimo_acesso_em,
        }
    }
}