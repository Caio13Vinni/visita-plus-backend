// =============================================================================
// Testes de integração — Integridade do esquema do MER no PostgreSQL
// -----------------------------------------------------------------------------
// Estes testes exigem o PostgreSQL do docker-compose em execucao:
//     docker compose up -d postgres
//     DATABASE_URL=postgres://visita:visita_forte_2026@localhost:5432/visita_plus \
//         cargo test --test integridade_mer
// =============================================================================

use sqlx::Row;
use std::collections::HashSet;

/// Obter a URL do banco a partir do ambiente.
fn url_do_banco() -> String {
    dotenvy::dotenv().ok();
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://visita:visita_forte_2026@localhost:5432/visita_plus".into())
}

/// Abre o pool e aplica as migracoes do MER.
async fn preparar_banco() -> sqlx::PgPool {
    let pool = sqlx::PgPool::connect(&url_do_banco())
        .await
        .expect("falha ao conectar no PostgreSQL (o docker-compose esta de pe? -> docker compose up -d postgres)");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("falha ao aplicar as migracoes do MER");
    pool
}

/// Lista as tabelas do schema public do banco.
async fn listar_tabelas(pool: &sqlx::PgPool) -> HashSet<String> {
    let linhas = sqlx::query(
        "SELECT table_name AS nome_tabela FROM information_schema.tables
         WHERE table_schema = 'public'",
    )
    .fetch_all(pool)
    .await
    .expect("falha ao consultar information_schema");

    linhas
        .iter()
        .map(|linha| linha.get::<String, _>("nome_tabela"))
        .filter(|nome| !nome.starts_with("_sqlx"))
        .collect()
}

/// Lista as colunas de uma tabela.
async fn listar_colunas(pool: &sqlx::PgPool, tabela: &str) -> HashSet<String> {
    let linhas = sqlx::query(
        "SELECT column_name AS nome_coluna FROM information_schema.columns
         WHERE table_schema = 'public' AND table_name = $1",
    )
    .bind(tabela)
    .fetch_all(pool)
    .await
    .unwrap_or_else(|erro| panic!("falha ao consultar colunas de {tabela}: {erro}"));

    linhas
        .iter()
        .map(|linha| linha.get::<String, _>("nome_coluna"))
        .collect()
}

// =============================================================================
// TESTES DE ESTRUTURA (schema)
// =============================================================================

#[tokio::test]
async fn o_banco_contem_todas_as_12_tabelas_do_mer() {
    let pool = preparar_banco().await;
    let tabelas = listar_tabelas(&pool).await;

    let esperadas = [
        "unidade",
        "usuario",
        "sessao",
        "recuperacao_acesso",
        "familia",
        "solicitacao_retorno",
        "visita",
        "registro_visita",
        "complemento_registro",
        "encaminhamento",
        "notificacao",
        "auditoria",
    ];

    for tabela in esperadas {
        assert!(
            tabelas.contains(tabela),
            "tabela do MER ausente no schema: {tabela}"
        );
    }
}

#[tokio::test]
async fn cada_tabela_do_mer_possui_suas_colunas_essenciais() {
    let pool = preparar_banco().await;

    let colunas_esperadas: Vec<(&str, Vec<&str>)> = vec![
        (
            "unidade",
            vec!["id_unidade", "nome", "tipo", "cidade", "criado_em"],
        ),
        (
            "usuario",
            vec![
                "id_usuario",
                "id_unidade",
                "nome",
                "email",
                "senha_hash",
                "perfil",
                "status",
                "exige_troca_senha",
                "ultimo_acesso_em",
            ],
        ),
        (
            "sessao",
            vec![
                "id_sessao",
                "id_usuario",
                "token_jwt",
                "expira_em",
                "encerrada_em",
            ],
        ),
        (
            "recuperacao_acesso",
            vec![
                "id_recuperacao",
                "id_usuario",
                "codigo",
                "expira_em",
                "usado_em",
            ],
        ),
        (
            "familia",
            vec![
                "id_familia",
                "id_unidade",
                "codigo",
                "nome_referencia",
                "contato",
                "endereco",
                "status",
                "criado_por",
            ],
        ),
        (
            "solicitacao_retorno",
            vec![
                "id_solicitacao",
                "id_visita",
                "motivo",
                "data_solicitacao",
                "status",
                "decidido_por",
            ],
        ),
        (
            "visita",
            vec![
                "id_visita",
                "id_familia",
                "id_tecnico",
                "id_agendado_por",
                "id_origem_retorno",
                "data_visita",
                "horario",
                "prioridade",
                "objetivo",
                "status",
                "motivo_cancelamento",
            ],
        ),
        (
            "registro_visita",
            vec![
                "id_registro",
                "id_visita",
                "status",
                "resultado",
                "descricao",
                "desfecho",
                "necessita_retorno",
                "registrado_por",
            ],
        ),
        (
            "complemento_registro",
            vec![
                "id_complemento",
                "id_registro",
                "descricao",
                "motivo",
                "registrado_por",
            ],
        ),
        (
            "encaminhamento",
            vec![
                "id_encaminhamento",
                "id_visita",
                "destino",
                "motivo",
                "data_encaminhamento",
                "status",
                "registrado_por",
            ],
        ),
        (
            "notificacao",
            vec![
                "id_notificacao",
                "id_usuario_destino",
                "titulo",
                "mensagem",
                "tipo",
                "lida",
                "data_hora",
            ],
        ),
        (
            "auditoria",
            vec![
                "id_auditoria",
                "id_usuario",
                "entidade",
                "entidade_id",
                "acao",
                "descricao",
                "data_hora",
            ],
        ),
    ];

    for (tabela, colunas) in colunas_esperadas {
        let existentes = listar_colunas(&pool, tabela).await;
        for coluna in colunas {
            assert!(
                existentes.contains(coluna),
                "coluna '{coluna}' ausente na tabela '{tabela}'"
            );
        }
    }
}

#[tokio::test]
async fn enums_do_mer_estao_registrados() {
    let pool = preparar_banco().await;

    let enums_esperados = [
        "tipo_unidade",
        "perfil_usuario",
        "status_usuario",
        "status_familia",
        "prioridade_visita",
        "status_visita",
        "status_registro",
        "resultado_registro",
        "status_solicitacao",
        "status_encaminhamento",
        "tipo_notificacao",
    ];

    for enum_ in enums_esperados {
        let existe: Option<String> =
            sqlx::query_scalar("SELECT typname FROM pg_type WHERE typname = $1")
                .bind(enum_)
                .fetch_optional(&pool)
                .await
                .expect("falha ao consultar pg_type");
        assert!(existe.is_some(), "enum do MER ausente: {enum_}");
    }
}

// =============================================================================
// TESTES DE INTEGRIDADE REFERENCIAL (operacoes reais no banco)
// =============================================================================

/// Gera um sufixo unico para garantir isolamento entre execucoes dos testes.
fn sufixo_unico() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..12].to_string()
}

/// Cria uma unidade e um usuário de teste; devolve os IDs.
/// O email gerado e unico por execucao, evitando colisoes em re-execucoes.
async fn criar_unidade_e_usuario(
    pool: &sqlx::PgPool,
    prefixo_email: &str,
) -> (uuid::Uuid, uuid::Uuid) {
    let token = sufixo_unico();
    let id_unidade: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO unidade (nome, tipo, cidade)
         VALUES ($1, 'CRAS', 'Cidade Teste')
         RETURNING id_unidade",
    )
    .bind(format!("Unidade Teste {token}"))
    .fetch_one(pool)
    .await
    .expect("falha ao inserir unidade");

    let email = format!("{prefixo_email}_{token}@teste.com");
    let id_usuario: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO usuario (id_unidade, nome, email, senha_hash, perfil, status, exige_troca_senha)
         VALUES ($1, 'Tecnico Teste', $2, 'hash_inexistente_para_teste', 'Tecnico', 'Ativo', TRUE)
         RETURNING id_usuario",
    )
    .bind(id_unidade)
    .bind(email)
    .fetch_one(pool)
    .await
    .expect("falha ao inserir usuario");

    (id_unidade, id_usuario)
}

#[tokio::test]
async fn chave_estrangeira_usuario_para_unidade_e_validada() {
    let pool = preparar_banco().await;

    // Referencia a uma unidade inexistente deve violar a FK
    let resultado = sqlx::query(
        "INSERT INTO usuario (id_unidade, nome, email, senha_hash, perfil)
         VALUES ($1, 'Nome', 'fk@teste.invalido', 'hash', 'Tecnico')",
    )
    .bind(uuid::Uuid::new_v4())
    .execute(&pool)
    .await;

    let erro = resultado.expect_err("deveria ter violado a FK de id_unidade");
    assert!(
        erro.to_string().contains("foreign key"),
        "erro inesperado: {erro}"
    );
}

#[tokio::test]
async fn email_de_usuario_duplicado_e_rejeitado() {
    let pool = preparar_banco().await;
    let token = sufixo_unico();

    // Cria apenas a unidade (sem usuario) e insere o MESMO email duas vezes
    let unidade: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO unidade (nome, tipo, cidade)
         VALUES ($1, 'CRAS', 'Cidade Teste')
         RETURNING id_unidade",
    )
    .bind(format!("Unidade Duplicidade {token}"))
    .fetch_one(&pool)
    .await
    .expect("falha ao inserir unidade");

    let email = format!("duplicado_{token}@teste.com");
    let sql = "INSERT INTO usuario (id_unidade, nome, email, senha_hash, perfil)
         VALUES ($1, 'Primeiro', $2, 'hash', 'Tecnico')";

    sqlx::query(sql)
        .bind(unidade)
        .bind(&email)
        .execute(&pool)
        .await
        .expect("primeiro usuario deveria ser inserido");

    let resultado = sqlx::query(sql)
        .bind(unidade)
        .bind(email)
        .execute(&pool)
        .await;
    let erro = resultado.expect_err("email duplicado deveria violar UNIQUE");
    assert!(
        erro.to_string().contains("unique"),
        "erro inesperado: {erro}"
    );
}

#[tokio::test]
async fn codigo_de_familia_duplicado_e_rejeitado() {
    let pool = preparar_banco().await;
    let (unidade, usuario) = criar_unidade_e_usuario(&pool, "familia").await;

    let codigo = format!("FAM-{}", sufixo_unico());
    let sql_inserir_familia = "INSERT INTO familia (id_unidade, codigo, nome_referencia, contato, endereco, status, criado_por)
             VALUES ($1, $2, 'Familia Exemplo', '(11) 99999-9999', 'Rua A, 123', 'Ativa', $3)
             RETURNING id_familia";

    sqlx::query(sql_inserir_familia)
        .bind(unidade)
        .bind(&codigo)
        .bind(usuario)
        .fetch_one(&pool)
        .await
        .expect("primeira familia deveria ser inserida");

    let resultado = sqlx::query(sql_inserir_familia)
        .bind(unidade)
        .bind(&codigo)
        .bind(usuario)
        .fetch_one(&pool)
        .await;
    let erro = resultado.expect_err("codigo de familia duplicado deveria violar UNIQUE");
    assert!(
        erro.to_string().contains("unique"),
        "erro inesperado: {erro}"
    );
}

#[tokio::test]
async fn visita_exige_relacionamento_1_para_0_ou_1_com_registro() {
    let pool = preparar_banco().await;
    let (unidade, tecnico) = criar_unidade_e_usuario(&pool, "visita_registro").await;

    // Coordenador que agendou
    let token = sufixo_unico();
    let coordenador: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO usuario (id_unidade, nome, email, senha_hash, perfil)
         VALUES ($1, 'Coordenador', $2, 'hash', 'Coordenador')
         RETURNING id_usuario",
    )
    .bind(unidade)
    .bind(format!("coordenador_visita_{token}@teste.com"))
    .fetch_one(&pool)
    .await
    .expect("falha ao criar coordenador");

    // Familia ficticia
    let familia: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO familia (id_unidade, codigo, nome_referencia, contato, endereco, criado_por)
         VALUES ($1, $2, 'Familia Visita', 'contato', 'endereco', $3)
         RETURNING id_familia",
    )
    .bind(unidade)
    .bind(format!("FAM-VIS-{token}"))
    .bind(coordenador)
    .bind(unidade)
    .bind(coordenador)
    .fetch_one(&pool)
    .await
    .expect("falha ao criar familia");

    // Visita agendada (amanha, horario valido)
    let visita: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO visita (id_familia, id_tecnico, id_agendado_por, data_visita, horario, prioridade, objetivo, status)
         VALUES ($1, $2, $3, CURRENT_DATE + 1, '09:00:00'::time, 'Normal', 'Acompanhamento', 'Agendada')
         RETURNING id_visita",
    )
    .bind(familia)
    .bind(tecnico)
    .bind(coordenador)
    .fetch_one(&pool)
    .await
    .expect("falha ao criar visita");

    // Um unico registro por visita: dois INSERT no mesmo id_visita devem violar UNIQUE
    let inserir_registro = || {
        sqlx::query(
            "INSERT INTO registro_visita (id_visita, status, registrado_por)
             VALUES ($1, 'Rascunho', $2)",
        )
        .bind(visita)
        .bind(tecnico)
    };

    inserir_registro()
        .execute(&pool)
        .await
        .expect("primeiro registro deveria ser inserido");

    let resultado = inserir_registro().execute(&pool).await;
    let erro = resultado.expect_err("segundo registro no mesmo id_visita deveria violar UNIQUE");
    assert!(
        erro.to_string().contains("unique"),
        "erro inesperado: {erro}"
    );
}

#[tokio::test]
async fn registro_realizada_exige_campos_de_fechamento() {
    let pool = preparar_banco().await;
    let (_, tecnico) = criar_unidade_e_usuario(&pool, "fechamento").await;

    // Tenta finalizar um registro sem os campos obrigatorios (RN13)
    // Construcao minima de uma visita valida para chegar ao registro.
    let (unidade, _) = criar_unidade_e_usuario(&pool, "fechamento2").await;
    let token = sufixo_unico();
    let coordenador: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO usuario (id_unidade, nome, email, senha_hash, perfil)
         VALUES ($1, 'Coord', $2, 'hash', 'Coordenador')
         RETURNING id_usuario",
    )
    .bind(unidade)
    .bind(format!("coord_fechamento_{token}@teste.com"))
    .fetch_one(&pool)
    .await
    .expect("falha ao criar coordenador");

    let familia: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO familia (id_unidade, codigo, nome_referencia, contato, endereco, criado_por)
         VALUES ($1, $2, 'Familia', 'contato', 'endereco', $3)
         RETURNING id_familia",
    )
    .bind(unidade)
    .bind(format!("FAM-FEC-{token}"))
    .bind(coordenador)
    .fetch_one(&pool)
    .await
    .expect("falha ao criar familia");

    let visita: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO visita (id_familia, id_tecnico, id_agendado_por, data_visita, horario, prioridade, objetivo, status)
         VALUES ($1, $2, $3, CURRENT_DATE + 1, '10:00:00'::time, 'Alta', 'Objetivo', 'Em andamento')
         RETURNING id_visita",
    )
    .bind(familia)
    .bind(tecnico)
    .bind(coordenador)
    .fetch_one(&pool)
    .await
    .expect("falha ao criar visita");

    // Primeiro inserimos o rascunho do registro, depois tentamos finalizar
    // com os campos obrigatorios faltantes (RN13).
    sqlx::query(
        "INSERT INTO registro_visita (id_visita, status, registrado_por)
         VALUES ($1, 'Rascunho', $2)",
    )
    .bind(visita)
    .bind(tecnico)
    .execute(&pool)
    .await
    .expect("falha ao criar rascunho");

    let resultado = sqlx::query(
        "UPDATE registro_visita
            SET status = 'Finalizado', resultado = 'Realizada'
          WHERE id_visita = $1",
    )
    .bind(visita)
    .execute(&pool)
    .await;

    let erro = resultado.expect_err("finalizacao sem campos obrigatorios deveria violar CHECK");
    assert!(
        erro.to_string().contains("chk_registro_realizada"),
        "erro inesperado: {erro}"
    );
}

#[tokio::test]
async fn sqlite_local_pode_ser_aberto_e_gravar_dados() {
    // Simula o banco local do dispositivo mobile usando um arquivo temporario
    let caminho = format!("/tmp/visita_plus_teste_{}.db", uuid::Uuid::new_v4());
    let pool = sqlx::SqlitePool::connect(&format!("sqlite:{caminho}?mode=rwc"))
        .await
        .expect("falha ao abrir o SQLite local");

    sqlx::query("CREATE TABLE IF NOT EXISTS teste_sync (id INTEGER PRIMARY KEY, valor TEXT)")
        .execute(&pool)
        .await
        .expect("falha ao criar tabela no sqlite");

    sqlx::query("INSERT INTO teste_sync (valor) VALUES ('visita offline')")
        .execute(&pool)
        .await
        .expect("falha ao inserir no sqlite");

    let contagem: i64 = sqlx::query_scalar("SELECT count(*) FROM teste_sync")
        .fetch_one(&pool)
        .await
        .expect("falha ao consultar no sqlite");
    assert_eq!(contagem, 1);

    pool.close().await;
    std::fs::remove_file(&caminho).ok();
}

#[tokio::test]
async fn auditoria_registra_acoes_criticas() {
    let pool = preparar_banco().await;
    let (_, usuario) = criar_unidade_e_usuario(&pool, "auditoria").await;

    sqlx::query(
        "INSERT INTO auditoria (id_usuario, entidade, entidade_id, acao, descricao)
         VALUES ($1, 'usuario', $2, 'LOGIN', 'Login realizado com sucesso')",
    )
    .bind(usuario)
    .bind(usuario)
    .execute(&pool)
    .await
    .expect("falha ao registrar auditoria");

    let acoes: Vec<String> = sqlx::query_scalar("SELECT acao FROM auditoria")
        .fetch_all(&pool)
        .await
        .expect("falha ao consultar auditoria");
    assert!(
        acoes.contains(&"LOGIN".to_string()),
        "auditoria deveria conter acao LOGIN"
    );
}
