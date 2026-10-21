// =============================================================================
// Helpers compartilhados pelos testes de integracao
// (subdiretorio: nao compilado como suite de testes propria)
// =============================================================================

use sqlx::PgPool;
use uuid::Uuid;

use visita_plus::dominio::enums::{PerfilUsuario, StatusUsuario};
use visita_plus::infraestrutura::seguranca::senha;

/// URL do banco central a partir do ambiente.
pub fn url_do_banco() -> String {
    dotenvy::dotenv().ok();
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://visita:visita_forte_2026@localhost:5432/visita_plus".into())
}

/// Abre o pool e aplica as migracoes do MER.
pub async fn preparar_banco() -> PgPool {
    let pool = PgPool::connect(&url_do_banco())
        .await
        .expect("falha ao conectar no PostgreSQL (docker compose up -d postgres)");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("falha ao aplicar as migracoes");
    pool
}

/// Sufixo unico para isolar dados entre execucoes.
pub fn sufixo_unico() -> String {
    Uuid::new_v4().simple().to_string()[..12].to_string()
}

/// Dados retornados apos criar um usuario de teste.
///
/// Nem todo alvo de teste le todos os campos/usa todos os helpers de
/// `comum` (o modulo e compilado por suite), logo o dead_code e tolerado.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct UsuarioDeTeste {
    pub id: Uuid,
    pub id_unidade: Uuid,
    pub email: String,
    pub senha: String,
}

/// Cria uma unidade CRAS de teste.
pub async fn criar_unidade(pool: &PgPool) -> Uuid {
    let token = sufixo_unico();
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO unidade (nome, tipo, cidade)
         VALUES ($1, 'CRAS', 'Cidade Teste')
         RETURNING id_unidade",
    )
    .bind(format!("Unidade Teste {token}"))
    .fetch_one(pool)
    .await
    .expect("falha ao criar unidade")
}

/// Cria um usuario de teste com hash Argon2id real.
pub async fn criar_usuario(
    pool: &PgPool,
    perfil: PerfilUsuario,
    status: StatusUsuario,
    exige_troca_senha: bool,
) -> UsuarioDeTeste {
    let token = sufixo_unico();
    let email = format!("usuario_{token}@teste.com");
    criar_usuario_com(
        pool,
        &email,
        "SenhaForte#123",
        perfil,
        status,
        exige_troca_senha,
    )
    .await
}

/// Cria um usuario de teste com e-mail e senha controlados.
pub async fn criar_usuario_com(
    pool: &PgPool,
    email: &str,
    senha: &str,
    perfil: PerfilUsuario,
    status: StatusUsuario,
    exige_troca_senha: bool,
) -> UsuarioDeTeste {
    let id_unidade = criar_unidade(pool).await;
    let senha_hash = senha::gerar_hash_senha(senha).expect("falha ao gerar hash de teste");

    let id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO usuario
            (id_unidade, nome, email, senha_hash, perfil, status, exige_troca_senha)
         VALUES ($1, 'Usuario Teste', $2, $3, $4, $5, $6)
         RETURNING id_usuario",
    )
    .bind(id_unidade)
    .bind(email)
    .bind(senha_hash)
    .bind(perfil)
    .bind(status)
    .bind(exige_troca_senha)
    .fetch_one(pool)
    .await
    .expect("falha ao criar usuario");

    UsuarioDeTeste {
        id,
        id_unidade,
        email: email.to_string(),
        senha: senha.to_string(),
    }
}

/// Retorna a quantidade de eventos de auditoria de uma acao para um usuario.
#[allow(dead_code)]
pub async fn contar_auditoria(pool: &PgPool, id_usuario: Uuid, acao: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM auditoria WHERE id_usuario = $1 AND acao = $2")
        .bind(id_usuario)
        .bind(acao)
        .fetch_one(pool)
        .await
        .expect("falha ao consultar auditoria")
}

/// Cria um usuario de teste vinculado a uma unidade especifica.
#[allow(dead_code)]
pub async fn criar_usuario_na_unidade(
    pool: &PgPool,
    id_unidade: Uuid,
    perfil: PerfilUsuario,
    status: StatusUsuario,
    exige_troca_senha: bool,
) -> UsuarioDeTeste {
    let token = sufixo_unico();
    let email = format!("usuario_unidade_{token}@teste.com");
    let senha_hash =
        senha::gerar_hash_senha("SenhaForte#123").expect("falha ao gerar hash de teste");

    let id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO usuario
            (id_unidade, nome, email, senha_hash, perfil, status, exige_troca_senha)
         VALUES ($1, 'Usuario Teste', $2, $3, $4, $5, $6)
         RETURNING id_usuario",
    )
    .bind(id_unidade)
    .bind(&email)
    .bind(senha_hash)
    .bind(perfil)
    .bind(status)
    .bind(exige_troca_senha)
    .fetch_one(pool)
    .await
    .expect("falha ao criar usuario");

    UsuarioDeTeste {
        id,
        id_unidade,
        email,
        senha: "SenhaForte#123".to_string(),
    }
}

/// Cria uma familia ficticia de teste na unidade informada (RN24).
#[allow(dead_code)]
pub async fn criar_familia(pool: &PgPool, id_unidade: Uuid, codigo: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO familia
            (id_unidade, codigo, nome_referencia, contato, endereco)
         VALUES ($1, $2, 'Familia Teste', '(11) 98888-7777', 'Rua de Teste, 100')
         RETURNING id_familia",
    )
    .bind(id_unidade)
    .bind(codigo)
    .fetch_one(pool)
    .await
    .expect("falha ao criar familia")
}

/// Cria uma sessao ativa para o usuario — usado por testes que forjam
/// tokens JWT diretamente (o extrator de autenticacao exige sessao ativa,
/// RF04).
#[allow(dead_code)]
pub async fn criar_sessao_ativa(pool: &PgPool, id_sessao: Uuid, id_usuario: Uuid) {
    // Token com valor unico por chamada: a coluna `token_jwt` tem
    // restricao UNIQUE e os testes rodam em paralelo.
    let token = format!("token_de_teste_{id_sessao}");
    sqlx::query(
        "INSERT INTO sessao (id_sessao, id_usuario, token_jwt, expira_em)
         VALUES ($1, $2, $3, now() + interval '1 hour')",
    )
    .bind(id_sessao)
    .bind(id_usuario)
    .bind(token)
    .execute(pool)
    .await
    .expect("falha ao criar sessao de teste");
}
