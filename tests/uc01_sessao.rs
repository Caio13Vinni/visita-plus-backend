// =============================================================================
// Testes de integração — UC01/RF04 (Encerramento e gestao de sessao)
// -----------------------------------------------------------------------------
// Exige o PostgreSQL do docker-compose em execucao:
//     docker compose up -d postgres
//     DATABASE_URL=postgres://visita:visita_forte_2026@localhost:5432/visita_plus \
//         cargo test --test uc01_sessao
// =============================================================================

mod comum;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

use comum::*;
use visita_plus::casos_de_uso::autenticacao::CasoDeUsoAutenticacao;
use visita_plus::casos_de_uso::recuperacao_acesso::CasoDeUsoRecuperacaoAcesso;
use visita_plus::dominio::enums::{PerfilUsuario, StatusUsuario};
use visita_plus::infraestrutura::auditoria::ServicoAuditoria;
use visita_plus::infraestrutura::auditoria::acao;
use visita_plus::infraestrutura::repositorios::recuperacao_repositorio::RepositorioRecuperacaoPostgres;
use visita_plus::infraestrutura::repositorios::sessao_repositorio::RepositorioSessaoPostgres;
use visita_plus::infraestrutura::repositorios::usuario_repositorio::RepositorioUsuarioPostgres;
use visita_plus::infraestrutura::seguranca::jwt::ServicoJwt;
use visita_plus::interface::api::construir_roteador;
use visita_plus::interface::estado::EstadoAplicacao;

/// Monta o caso de uso UC01 contra o banco real.
fn montar_caso_de_uso(pool: &sqlx::PgPool) -> CasoDeUsoAutenticacao {
    CasoDeUsoAutenticacao::novo(
        Arc::new(RepositorioUsuarioPostgres::novo(pool.clone())),
        Arc::new(RepositorioSessaoPostgres::novo(pool.clone())),
        ServicoJwt::novo("segredo_teste_sessao", 60),
        ServicoAuditoria::novo(pool.clone()),
    )
}

/// Monta o caso de uso de recuperacao (RF03) para o cenario de invalidacao.
fn montar_caso_recuperacao(pool: &sqlx::PgPool) -> CasoDeUsoRecuperacaoAcesso {
    CasoDeUsoRecuperacaoAcesso::novo(
        Arc::new(RepositorioRecuperacaoPostgres::novo(pool.clone())),
        Arc::new(RepositorioUsuarioPostgres::novo(pool.clone())),
        Arc::new(RepositorioSessaoPostgres::novo(pool.clone())),
        ServicoAuditoria::novo(pool.clone()),
    )
}

/// Monta o roteador HTTP com estado real.
fn montar_roteador(pool: sqlx::PgPool) -> axum::Router {
    let estado = EstadoAplicacao::novo(
        pool,
        format!("/tmp/visita_plus_sessao_{}.db", sufixo_unico()),
    );
    construir_roteador(estado)
}

/// Faz login via HTTP e devolve o token JWT.
async fn login_via_http(roteador: &axum::Router, email: &str, senha: &str) -> String {
    let resposta = roteador
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::json!({ "email": email, "senha": senha }).to_string(),
                ))
                .expect("requisicao valida"),
        )
        .await
        .expect("falha ao executar a requisicao");

    assert_eq!(
        resposta.status(),
        StatusCode::OK,
        "login deveria ter sucesso"
    );

    let corpo = axum::body::to_bytes(resposta.into_body(), 1024 * 1024)
        .await
        .expect("falha ao ler o corpo");
    let json: serde_json::Value = serde_json::from_slice(&corpo).expect("json valido");
    json["token"]
        .as_str()
        .expect("resposta de login deve conter token")
        .to_string()
}

/// Busca a sessao mais recente de um usuario na tabela `sessao`.
async fn buscar_ultima_sessao(
    pool: &sqlx::PgPool,
    id_usuario: Uuid,
) -> Option<(
    Uuid,
    Option<chrono::DateTime<chrono::Utc>>,
    chrono::DateTime<chrono::Utc>,
)> {
    sqlx::query_as::<
        _,
        (
            Uuid,
            Option<chrono::DateTime<chrono::Utc>>,
            chrono::DateTime<chrono::Utc>,
        ),
    >(
        "SELECT id_sessao, encerrada_em, expira_em FROM sessao
          WHERE id_usuario = $1 ORDER BY criado_em DESC LIMIT 1",
    )
    .bind(id_usuario)
    .fetch_optional(pool)
    .await
    .expect("falha ao consultar sessao")
}

// =============================================================================
// Criacao da sessao no login (RF04)
// =============================================================================

#[tokio::test]
async fn login_cria_sessao_ativa_vinculada_ao_token() {
    let pool = preparar_banco().await;
    let usuario = criar_usuario(&pool, PerfilUsuario::Tecnico, StatusUsuario::Ativo, false).await;
    let caso = montar_caso_de_uso(&pool);

    let resultado = caso
        .autenticar(
            visita_plus::casos_de_uso::autenticacao::Credenciais {
                email: usuario.email.clone(),
                senha: usuario.senha.clone(),
            },
            None,
            None,
        )
        .await
        .expect("login deveria ter sucesso");

    // A sessao deve existir, estar ativa e vinculada ao usuario
    let sessao = buscar_ultima_sessao(&pool, usuario.id)
        .await
        .expect("login deveria criar uma sessao");
    assert!(sessao.1.is_none(), "sessao nova nao deve estar encerrada");
    assert!(
        sessao.2 > chrono::Utc::now(),
        "expiracao da sessao deve estar no futuro"
    );

    // O token deve carregar o id da sessao nos claims
    let servico = ServicoJwt::novo("segredo_teste_sessao", 60);
    let claims = servico.validar(&resultado.token).expect("token valido");
    assert_eq!(
        claims.sid, sessao.0,
        "token deve referenciar a sessao criada"
    );
}

// =============================================================================
// Encerramento manual da sessao (RF04)
// =============================================================================

#[tokio::test]
async fn logout_encerra_a_sessao_e_token_deixa_de_valer() {
    let pool = preparar_banco().await;
    let usuario = criar_usuario(&pool, PerfilUsuario::Tecnico, StatusUsuario::Ativo, false).await;
    let roteador = montar_roteador(pool.clone());
    let token = login_via_http(&roteador, &usuario.email, &usuario.senha).await;

    // Logout encerra a sessao
    let resposta = roteador
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/logout")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .expect("requisicao valida"),
        )
        .await
        .expect("falha ao executar a requisicao");
    assert_eq!(resposta.status(), StatusCode::NO_CONTENT);

    // A sessao foi marcada como encerrada no banco
    let sessao = buscar_ultima_sessao(&pool, usuario.id)
        .await
        .expect("sessao deveria existir");
    assert!(
        sessao.1.is_some(),
        "logout deve marcar encerrada_em na sessao"
    );

    // O mesmo token e rejeitado em rota protegida apos o logout
    let resposta = roteador
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/trocar-senha")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "senha_atual": usuario.senha,
                        "nova_senha": "NovaSenha#456"
                    })
                    .to_string(),
                ))
                .expect("requisicao valida"),
        )
        .await
        .expect("falha ao executar a requisicao");
    assert_eq!(
        resposta.status(),
        StatusCode::UNAUTHORIZED,
        "token com sessao encerrada deve ser rejeitado"
    );

    // Auditoria do encerramento
    let total = contar_auditoria(&pool, usuario.id, acao::SESSAO_ENCERRADA).await;
    assert_eq!(total, 1, "deveria haver 1 evento SESSAO_ENCERRADA");
}

#[tokio::test]
async fn sessao_expirada_por_inatividade_e_rejeitada() {
    let pool = preparar_banco().await;
    let usuario = criar_usuario(&pool, PerfilUsuario::Tecnico, StatusUsuario::Ativo, false).await;
    let roteador = montar_roteador(pool.clone());
    let token = login_via_http(&roteador, &usuario.email, &usuario.senha).await;

    // Simula inatividade: expira a sessao diretamente no banco
    sqlx::query("UPDATE sessao SET expira_em = now() - interval '1 minute' WHERE id_usuario = $1")
        .bind(usuario.id)
        .execute(&pool)
        .await
        .expect("falha ao expirar a sessao");

    let resposta = roteador
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/trocar-senha")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "senha_atual": usuario.senha,
                        "nova_senha": "NovaSenha#456"
                    })
                    .to_string(),
                ))
                .expect("requisicao valida"),
        )
        .await
        .expect("falha ao executar a requisicao");
    assert_eq!(
        resposta.status(),
        StatusCode::UNAUTHORIZED,
        "sessao expirada deve ser rejeitada"
    );
}

#[tokio::test]
async fn atividade_renova_a_expiracao_da_sessao() {
    let pool = preparar_banco().await;
    let usuario = criar_usuario(&pool, PerfilUsuario::Tecnico, StatusUsuario::Ativo, false).await;
    let roteador = montar_roteador(pool.clone());
    let token = login_via_http(&roteador, &usuario.email, &usuario.senha).await;

    // Sessao prestes a expirar: com a janela de 1440 min, restar 1 minuto
    // deve disparar a renovacao deslizante na proxima requisicao
    sqlx::query("UPDATE sessao SET expira_em = now() + interval '1 minute' WHERE id_usuario = $1")
        .bind(usuario.id)
        .execute(&pool)
        .await
        .expect("falha ao ajustar a expiracao da sessao");

    let resposta = roteador
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/trocar-senha")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "senha_atual": usuario.senha,
                        "nova_senha": "NovaSenha#456"
                    })
                    .to_string(),
                ))
                .expect("requisicao valida"),
        )
        .await
        .expect("falha ao executar a requisicao");
    assert_eq!(resposta.status(), StatusCode::NO_CONTENT);

    // A expiracao foi prorrogada para agora + janela
    let sessao = buscar_ultima_sessao(&pool, usuario.id)
        .await
        .expect("sessao deveria existir");
    assert!(
        sessao.2 > chrono::Utc::now() + chrono::Duration::minutes(30),
        "atividade deve renovar a expiracao da sessao (expira_em={})",
        sessao.2
    );
}

// =============================================================================
// Encerramento em lote e regras de posse (RF04)
// =============================================================================

#[tokio::test]
async fn redefinicao_por_recuperacao_encerra_todas_as_sessoes_do_usuario() {
    let pool = preparar_banco().await;
    let usuario = criar_usuario(&pool, PerfilUsuario::Tecnico, StatusUsuario::Ativo, false).await;
    let roteador = montar_roteador(pool.clone());

    // Duas sessoes ativas (dois logins)
    let token_primeiro = login_via_http(&roteador, &usuario.email, &usuario.senha).await;
    let token_segundo = login_via_http(&roteador, &usuario.email, &usuario.senha).await;
    assert_ne!(token_primeiro, token_segundo);

    // Redefine a senha via recuperacao (RF03 + RF04)
    let recuperacao = montar_caso_recuperacao(&pool);
    let codigo = recuperacao
        .solicitar(&usuario.email)
        .await
        .expect("solicitacao deveria ter sucesso")
        .codigo
        .expect("deveria ter codigo");
    recuperacao
        .redefinir(&usuario.email, &codigo, "SenhaRecuperada#2026")
        .await
        .expect("redefinicao deveria ter sucesso");

    // Todas as sessoes do usuario foram encerradas
    let ativas: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM sessao
          WHERE id_usuario = $1 AND encerrada_em IS NULL AND expira_em > now()",
    )
    .bind(usuario.id)
    .fetch_one(&pool)
    .await
    .expect("falha ao contar sessoes ativas");
    assert_eq!(
        ativas, 0,
        "redefinicao deve encerrar todas as sessoes ativas"
    );

    // Tokens antigos deixam de valer
    let resposta = roteador
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/trocar-senha")
                .header(header::AUTHORIZATION, format!("Bearer {token_segundo}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "senha_atual": usuario.senha,
                        "nova_senha": "OutraSenha#789"
                    })
                    .to_string(),
                ))
                .expect("requisicao valida"),
        )
        .await
        .expect("falha ao executar a requisicao");
    assert_eq!(
        resposta.status(),
        StatusCode::UNAUTHORIZED,
        "token apos redefinicao da senha deve ser rejeitado"
    );
}

#[tokio::test]
async fn usuario_nao_encerra_sessao_alheia() {
    let pool = preparar_banco().await;
    let dono = criar_usuario(&pool, PerfilUsuario::Tecnico, StatusUsuario::Ativo, false).await;
    let outro = criar_usuario(&pool, PerfilUsuario::Tecnico, StatusUsuario::Ativo, false).await;
    let caso = montar_caso_de_uso(&pool);

    let resultado = caso
        .autenticar(
            visita_plus::casos_de_uso::autenticacao::Credenciais {
                email: dono.email.clone(),
                senha: dono.senha.clone(),
            },
            None,
            None,
        )
        .await
        .expect("login deveria ter sucesso");
    let servico = ServicoJwt::novo("segredo_teste_sessao", 60);
    let claims = servico.validar(&resultado.token).expect("token valido");

    // Outro usuario tenta encerrar a sessao do dono
    let erro = caso
        .encerrar_sessao(outro.id, claims.sid)
        .await
        .expect_err("sessao alheia nao pode ser encerrada");
    assert!(matches!(
        erro,
        visita_plus::dominio::erros::ErroAutenticacao::SessaoInvalida
    ));

    // A sessao permanece ativa
    let sessao = buscar_ultima_sessao(&pool, dono.id)
        .await
        .expect("sessao deveria existir");
    assert!(sessao.1.is_none(), "sessao nao deve ter sido encerrada");
}
