// =============================================================================
// Testes de integração — UC01/RF03 (Recuperacao de acesso por codigo)
// -----------------------------------------------------------------------------
// Exige o PostgreSQL do docker-compose em execucao:
//     docker compose up -d postgres
//     DATABASE_URL=postgres://visita:visita_forte_2026@localhost:5432/visita_plus \
//         cargo test --test uc01_recuperacao_acesso
// =============================================================================

mod comum;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

use comum::*;
use visita_plus::casos_de_uso::autenticacao::{CasoDeUsoAutenticacao, Credenciais};
use visita_plus::casos_de_uso::recuperacao_acesso::{
    CasoDeUsoRecuperacaoAcesso, MENSAGEM_RECUPERACAO_GENERICA,
};
use visita_plus::dominio::enums::{PerfilUsuario, StatusUsuario};
use visita_plus::dominio::erros::ErroRecuperacao;
use visita_plus::infraestrutura::auditoria::ServicoAuditoria;
use visita_plus::infraestrutura::auditoria::acao;
use visita_plus::infraestrutura::repositorios::recuperacao_repositorio::RepositorioRecuperacaoPostgres;
use visita_plus::infraestrutura::repositorios::usuario_repositorio::RepositorioUsuarioPostgres;
use visita_plus::infraestrutura::seguranca::jwt::ServicoJwt;
use visita_plus::interface::api::construir_roteador;
use visita_plus::interface::estado::EstadoAplicacao;

/// Monta o caso de uso de recuperacao de acesso contra o banco real.
fn montar_caso_de_uso(pool: &sqlx::PgPool) -> CasoDeUsoRecuperacaoAcesso {
    CasoDeUsoRecuperacaoAcesso::novo(
        Arc::new(RepositorioRecuperacaoPostgres::novo(pool.clone())),
        Arc::new(RepositorioUsuarioPostgres::novo(pool.clone())),
        ServicoAuditoria::novo(pool.clone()),
    )
}

/// Monta o caso de uso de autenticacao para conferir a senha efetiva.
fn montar_caso_autenticacao(pool: &sqlx::PgPool) -> CasoDeUsoAutenticacao {
    CasoDeUsoAutenticacao::novo(
        Arc::new(RepositorioUsuarioPostgres::novo(pool.clone())),
        ServicoJwt::novo("segredo_teste_recuperacao", 60),
        ServicoAuditoria::novo(pool.clone()),
    )
}

/// Conta os codigos de recuperacao vinculados a um usuario.
async fn contar_recuperacoes(pool: &sqlx::PgPool, id_usuario: Uuid) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM recuperacao_acesso WHERE id_usuario = $1")
        .bind(id_usuario)
        .fetch_one(pool)
        .await
        .expect("falha ao contar recuperacoes")
}

/// Conta eventos de auditoria pela acao + descricao (eventos sem usuario).
async fn contar_auditoria_por_descricao(pool: &sqlx::PgPool, acao: &str, descricao: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM auditoria WHERE acao = $1 AND descricao = $2")
        .bind(acao)
        .bind(descricao)
        .fetch_one(pool)
        .await
        .expect("falha ao consultar auditoria")
}

// =============================================================================
// Fluxo principal do RF03 (camada de dados e caso de uso)
// =============================================================================

#[tokio::test]
async fn solicitar_para_email_ativo_cria_codigo_de_6_digitos_e_audita() {
    let pool = preparar_banco().await;
    let usuario = criar_usuario(&pool, PerfilUsuario::Tecnico, StatusUsuario::Ativo, false).await;
    let caso = montar_caso_de_uso(&pool);

    let solicitacao = caso
        .solicitar(&usuario.email)
        .await
        .expect("solicitacao deveria ter sucesso");

    assert_eq!(solicitacao.mensagem, MENSAGEM_RECUPERACAO_GENERICA);

    // O codigo fica disponivel no resultado do caso de uso (prototipo sem SMTP)
    let codigo = solicitacao
        .codigo
        .expect("conta ativa deveria receber codigo");
    assert_eq!(codigo.len(), 6, "codigo deve ter 6 digitos");
    assert!(
        codigo.chars().all(|c| c.is_ascii_digit()),
        "codigo deve ser numerico: {codigo}"
    );

    // O codigo deve estar persistido na tabela `recuperacao_acesso`
    let salvo: Option<String> = sqlx::query_scalar(
        "SELECT codigo FROM recuperacao_acesso
          WHERE id_usuario = $1 AND usado_em IS NULL AND expira_em > now()",
    )
    .bind(usuario.id)
    .fetch_optional(&pool)
    .await
    .expect("falha ao consultar recuperacao_acesso");
    assert_eq!(salvo.as_deref(), Some(codigo.as_str()));

    let total = contar_auditoria(&pool, usuario.id, acao::RECUPERACAO_SOLICITADA).await;
    assert_eq!(
        total, 1,
        "deveria haver 1 evento RECUPERACAO_SOLICITADA na auditoria"
    );
}

#[tokio::test]
async fn solicitar_para_email_inexistente_retorna_mesma_resposta_sem_codigo_e_audita_negacao() {
    let pool = preparar_banco().await;
    let caso = montar_caso_de_uso(&pool);
    let email = format!("inexistente_{}@teste.com", sufixo_unico());

    let solicitacao = caso
        .solicitar(&email)
        .await
        .expect("solicitacao deveria retornar Ok generico");

    // Mesma resposta publica, sem codigo
    assert_eq!(solicitacao.mensagem, MENSAGEM_RECUPERACAO_GENERICA);
    assert!(
        solicitacao.codigo.is_none(),
        "e-mail inexistente nao deve gerar codigo"
    );

    // Auditoria de negacao sem vinculo de usuario: evento global, acumula
    // entre execucoes no banco compartilhado dos testes
    let total = contar_auditoria_por_descricao(
        &pool,
        acao::RECUPERACAO_NEGADA,
        "solicitacao de recuperacao para e-mail inexistente",
    )
    .await;
    assert!(
        total >= 1,
        "deveria haver ao menos 1 evento RECUPERACAO_NEGADA na auditoria"
    );
}

#[tokio::test]
async fn solicitar_para_conta_inativa_retorna_mesma_resposta_sem_codigo() {
    let pool = preparar_banco().await;
    let usuario = criar_usuario(&pool, PerfilUsuario::Tecnico, StatusUsuario::Inativo, false).await;
    let caso = montar_caso_de_uso(&pool);

    let solicitacao = caso
        .solicitar(&usuario.email)
        .await
        .expect("solicitacao deveria retornar Ok generico");

    // Resposta publica identica a do e-mail inexistente, sem codigo
    assert_eq!(solicitacao.mensagem, MENSAGEM_RECUPERACAO_GENERICA);
    assert!(
        solicitacao.codigo.is_none(),
        "conta inativa nao deve gerar codigo"
    );

    let total = contar_recuperacoes(&pool, usuario.id).await;
    assert_eq!(total, 0, "conta inativa nao deve criar registro de codigo");

    let total = contar_auditoria_por_descricao(
        &pool,
        acao::RECUPERACAO_NEGADA,
        "solicitacao de recuperacao para conta inativa",
    )
    .await;
    assert!(
        total >= 1,
        "deveria haver ao menos 1 evento RECUPERACAO_NEGADA"
    );
}

#[tokio::test]
async fn nova_solicitacao_invalida_codigo_anterior() {
    let pool = preparar_banco().await;
    let usuario = criar_usuario(&pool, PerfilUsuario::Tecnico, StatusUsuario::Ativo, false).await;
    let caso = montar_caso_de_uso(&pool);

    let primeira = caso
        .solicitar(&usuario.email)
        .await
        .expect("primeira solicitacao deveria ter sucesso");
    let codigo_anterior = primeira.codigo.expect("deveria ter codigo");

    let segunda = caso
        .solicitar(&usuario.email)
        .await
        .expect("segunda solicitacao deveria ter sucesso");
    let codigo_novo = segunda.codigo.expect("deveria ter codigo");

    assert_ne!(
        codigo_anterior, codigo_novo,
        "nova solicitacao deve gerar codigo distinto"
    );

    // O codigo anterior foi invalidado: nao serve mais para redefinir
    let erro = caso
        .redefinir(&usuario.email, &codigo_anterior, "NovaSenha#456")
        .await
        .expect_err("codigo anterior invalidado deveria falhar");
    assert!(matches!(erro, ErroRecuperacao::CodigoInvalidoOuExpirado));

    // O codigo mais recente continua valido
    caso.redefinir(&usuario.email, &codigo_novo, "NovaSenha#456")
        .await
        .expect("codigo mais recente deveria funcionar");
}

#[tokio::test]
async fn redefinir_com_codigo_valido_troca_senha_e_audita_conclusao() {
    let pool = preparar_banco().await;
    let usuario = criar_usuario(&pool, PerfilUsuario::Tecnico, StatusUsuario::Ativo, false).await;
    let caso = montar_caso_de_uso(&pool);
    let autenticacao = montar_caso_autenticacao(&pool);

    let solicitacao = caso
        .solicitar(&usuario.email)
        .await
        .expect("solicitacao deveria ter sucesso");
    let codigo = solicitacao.codigo.expect("deveria ter codigo");

    caso.redefinir(&usuario.email, &codigo, "NovaSenha#456")
        .await
        .expect("redefinicao deveria ter sucesso");

    // Senha antiga nao funciona mais
    let erro = autenticacao
        .autenticar(
            Credenciais {
                email: usuario.email.clone(),
                senha: usuario.senha.clone(),
            },
            None,
            None,
        )
        .await
        .expect_err("senha antiga nao deveria funcionar");
    assert!(matches!(
        erro,
        visita_plus::dominio::erros::ErroAutenticacao::CredenciaisInvalidas
    ));

    // Nova senha funciona
    let resultado = autenticacao
        .autenticar(
            Credenciais {
                email: usuario.email.clone(),
                senha: "NovaSenha#456".into(),
            },
            None,
            None,
        )
        .await
        .expect("nova senha deveria funcionar");
    assert_eq!(resultado.usuario.id, usuario.id);

    // O codigo foi consumido (uso unico)
    let salvo: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT usado_em FROM recuperacao_acesso
          WHERE id_usuario = $1 AND codigo = $2",
    )
    .bind(usuario.id)
    .bind(&codigo)
    .fetch_one(&pool)
    .await
    .expect("falha ao consultar recuperacao_acesso");
    assert!(
        salvo.is_some(),
        "codigo deve ficar marcado como usado apos a redefinicao"
    );

    let total = contar_auditoria(&pool, usuario.id, acao::RECUPERACAO_CONCLUIDA).await;
    assert_eq!(
        total, 1,
        "deveria haver 1 evento RECUPERACAO_CONCLUIDA na auditoria"
    );
}

#[tokio::test]
async fn redefinir_com_codigo_errado_falha_com_erro_generico_e_audita() {
    let pool = preparar_banco().await;
    let usuario = criar_usuario(&pool, PerfilUsuario::Tecnico, StatusUsuario::Ativo, false).await;
    let caso = montar_caso_de_uso(&pool);

    caso.solicitar(&usuario.email)
        .await
        .expect("solicitacao deveria ter sucesso");

    let erro = caso
        .redefinir(&usuario.email, "000000", "NovaSenha#456")
        .await
        .expect_err("codigo incorreto deveria falhar");
    assert!(matches!(erro, ErroRecuperacao::CodigoInvalidoOuExpirado));

    // A senha original permanece valida (redefinicao nao ocorreu)
    let autenticacao = montar_caso_autenticacao(&pool);
    autenticacao
        .autenticar(
            Credenciais {
                email: usuario.email.clone(),
                senha: usuario.senha.clone(),
            },
            None,
            None,
        )
        .await
        .expect("senha original deveria continuar valida");

    let total = contar_auditoria(&pool, usuario.id, acao::RECUPERACAO_FALHA).await;
    assert_eq!(
        total, 1,
        "deveria haver 1 evento RECUPERACAO_FALHA na auditoria"
    );
}

#[tokio::test]
async fn redefinir_com_codigo_ja_usado_falha() {
    let pool = preparar_banco().await;
    let usuario = criar_usuario(&pool, PerfilUsuario::Tecnico, StatusUsuario::Ativo, false).await;
    let caso = montar_caso_de_uso(&pool);

    let solicitacao = caso
        .solicitar(&usuario.email)
        .await
        .expect("solicitacao deveria ter sucesso");
    let codigo = solicitacao.codigo.expect("deveria ter codigo");

    caso.redefinir(&usuario.email, &codigo, "NovaSenha#456")
        .await
        .expect("primeira redefinicao deveria ter sucesso");

    // Reuso do mesmo codigo e negado (uso unico — RF03)
    let erro = caso
        .redefinir(&usuario.email, &codigo, "OutraSenha#789")
        .await
        .expect_err("codigo ja usado deveria falhar");
    assert!(matches!(erro, ErroRecuperacao::CodigoInvalidoOuExpirado));
}

#[tokio::test]
async fn redefinir_com_codigo_expirado_falha() {
    let pool = preparar_banco().await;
    let usuario = criar_usuario(&pool, PerfilUsuario::Tecnico, StatusUsuario::Ativo, false).await;
    let caso = montar_caso_de_uso(&pool);

    // Codigo ja expirado inserido diretamente na tabela
    sqlx::query(
        "INSERT INTO recuperacao_acesso (id_usuario, codigo, expira_em)
         VALUES ($1, '123456', now() - interval '1 minute')",
    )
    .bind(usuario.id)
    .execute(&pool)
    .await
    .expect("falha ao criar codigo expirado");

    let erro = caso
        .redefinir(&usuario.email, "123456", "NovaSenha#456")
        .await
        .expect_err("codigo expirado deveria falhar");
    assert!(matches!(erro, ErroRecuperacao::CodigoInvalidoOuExpirado));
}

#[tokio::test]
async fn senha_fraca_e_rejeitada_na_redefinicao_sem_consumir_o_codigo() {
    let pool = preparar_banco().await;
    let usuario = criar_usuario(&pool, PerfilUsuario::Tecnico, StatusUsuario::Ativo, false).await;
    let caso = montar_caso_de_uso(&pool);

    let solicitacao = caso
        .solicitar(&usuario.email)
        .await
        .expect("solicitacao deveria ter sucesso");
    let codigo = solicitacao.codigo.expect("deveria ter codigo");

    let erro = caso
        .redefinir(&usuario.email, &codigo, "123")
        .await
        .expect_err("senha fraca deveria ser rejeitada");
    assert!(matches!(erro, ErroRecuperacao::SenhaFraca));

    // O codigo nao foi consumido pela tentativa com senha fraca
    caso.redefinir(&usuario.email, &codigo, "NovaSenha#456")
        .await
        .expect("o mesmo codigo ainda deveria ser valido");
}

#[tokio::test]
async fn redefinir_para_email_inexistente_retorna_erro_generico() {
    let pool = preparar_banco().await;
    let caso = montar_caso_de_uso(&pool);
    let email = format!("inexistente_{}@teste.com", sufixo_unico());

    let erro = caso
        .redefinir(&email, "123456", "NovaSenha#456")
        .await
        .expect_err("e-mail inexistente deveria falhar com erro generico");
    assert!(matches!(erro, ErroRecuperacao::CodigoInvalidoOuExpirado));
}

// =============================================================================
// Fluxo HTTP do RF03 (endpoints anonimos /api/v1/auth/recuperar-senha)
// =============================================================================

/// Constroi o roteador com estado real (JWT default de desenvolvimento).
fn montar_roteador(pool: sqlx::PgPool) -> axum::Router {
    let estado = EstadoAplicacao::novo(
        pool,
        format!("/tmp/visita_plus_recuperacao_{}.db", sufixo_unico()),
    );
    construir_roteador(estado)
}

#[tokio::test]
async fn endpoint_recuperar_senha_retorna_200_com_mensagem_generica_sem_expor_codigo() {
    let pool = preparar_banco().await;
    let usuario = criar_usuario(&pool, PerfilUsuario::Tecnico, StatusUsuario::Ativo, false).await;
    let roteador = montar_roteador(pool);

    let corpo = serde_json::json!({ "email": usuario.email });
    let resposta = roteador
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/recuperar-senha")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(corpo.to_string()))
                .expect("requisicao valida"),
        )
        .await
        .expect("falha ao executar a requisicao");

    assert_eq!(resposta.status(), StatusCode::OK);

    let corpo_resposta = axum::body::to_bytes(resposta.into_body(), 1024 * 1024)
        .await
        .expect("falha ao ler o corpo");
    let json: serde_json::Value = serde_json::from_slice(&corpo_resposta).expect("json valido");

    // Resposta generica: mensagem publica, sem campo de codigo
    assert_eq!(json["mensagem"], MENSAGEM_RECUPERACAO_GENERICA);
    assert!(
        json.get("codigo").is_none(),
        "a resposta HTTP nao deve expor o codigo de recuperacao"
    );
}

#[tokio::test]
async fn endpoint_recuperar_senha_com_email_invalido_responde_400() {
    let pool = preparar_banco().await;
    let roteador = montar_roteador(pool);

    let corpo = serde_json::json!({ "email": "email_sem_arroba" });
    let resposta = roteador
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/recuperar-senha")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(corpo.to_string()))
                .expect("requisicao valida"),
        )
        .await
        .expect("falha ao executar a requisicao");

    assert_eq!(resposta.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn endpoint_redefinir_retorna_200_e_nova_senha_passa_a_valer_no_login() {
    let pool = preparar_banco().await;
    let usuario = criar_usuario(&pool, PerfilUsuario::Tecnico, StatusUsuario::Ativo, false).await;

    // O codigo e obtido fora da API (prototipo sem SMTP): via caso de uso
    let caso = montar_caso_de_uso(&pool);
    let codigo = caso
        .solicitar(&usuario.email)
        .await
        .expect("solicitacao deveria ter sucesso")
        .codigo
        .expect("deveria ter codigo");

    let roteador = montar_roteador(pool);

    let corpo = serde_json::json!({
        "email": usuario.email,
        "codigo": codigo,
        "nova_senha": "SenhaRecuperada#2026",
    });
    let resposta = roteador
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/recuperar-senha/redefinir")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(corpo.to_string()))
                .expect("requisicao valida"),
        )
        .await
        .expect("falha ao executar a requisicao");
    assert_eq!(resposta.status(), StatusCode::OK);

    // Login com a nova senha funciona
    let corpo = serde_json::json!({
        "email": usuario.email,
        "senha": "SenhaRecuperada#2026",
    });
    let resposta = roteador
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(corpo.to_string()))
                .expect("requisicao valida"),
        )
        .await
        .expect("falha ao executar a requisicao");
    assert_eq!(resposta.status(), StatusCode::OK);

    // Login com a senha antiga falha
    let corpo = serde_json::json!({
        "email": usuario.email,
        "senha": usuario.senha,
    });
    let resposta = roteador
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(corpo.to_string()))
                .expect("requisicao valida"),
        )
        .await
        .expect("falha ao executar a requisicao");
    assert_eq!(resposta.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn endpoint_redefinir_com_codigo_errado_responde_400_com_mensagem_generica() {
    let pool = preparar_banco().await;
    let usuario = criar_usuario(&pool, PerfilUsuario::Tecnico, StatusUsuario::Ativo, false).await;
    let roteador = montar_roteador(pool);

    let corpo = serde_json::json!({
        "email": usuario.email,
        "codigo": "999999",
        "nova_senha": "SenhaRecuperada#2026",
    });
    let resposta = roteador
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/recuperar-senha/redefinir")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(corpo.to_string()))
                .expect("requisicao valida"),
        )
        .await
        .expect("falha ao executar a requisicao");

    assert_eq!(resposta.status(), StatusCode::BAD_REQUEST);

    let corpo_resposta = axum::body::to_bytes(resposta.into_body(), 1024 * 1024)
        .await
        .expect("falha ao ler o corpo");
    let json: serde_json::Value = serde_json::from_slice(&corpo_resposta).expect("json valido");
    assert_eq!(json["mensagem"], "codigo invalido ou expirado");
}
