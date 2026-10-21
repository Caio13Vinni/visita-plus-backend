// =============================================================================
// Testes de integração — UC01 (Autenticar usuario)
// -----------------------------------------------------------------------------
// Exige o PostgreSQL do docker-compose em execucao:
//     docker compose up -d postgres
//     DATABASE_URL=postgres://visita:visita_forte_2026@localhost:5432/visita_plus \
//         cargo test --test uc01_autenticacao
// =============================================================================

mod comum;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use std::sync::Arc;
use tower::ServiceExt;

use comum::*;
use visita_plus::casos_de_uso::autenticacao::{CasoDeUsoAutenticacao, Credenciais};
use visita_plus::dominio::enums::{PerfilUsuario, StatusUsuario};
use visita_plus::dominio::erros::ErroAutenticacao;
use visita_plus::infraestrutura::auditoria::ServicoAuditoria;
use visita_plus::infraestrutura::auditoria::acao;
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
        ServicoJwt::novo("segredo_teste_uc01", 60),
        ServicoAuditoria::novo(pool.clone()),
    )
}

// =============================================================================
// Fluxo principal do UC01 (cobrindo a camada de dados)
// =============================================================================

#[tokio::test]
async fn login_com_credenciais_validas_emite_token_com_claims_corretos() {
    let pool = preparar_banco().await;
    let usuario = criar_usuario(&pool, PerfilUsuario::Tecnico, StatusUsuario::Ativo, false).await;
    let caso = montar_caso_de_uso(&pool);

    let resultado = caso
        .autenticar(
            Credenciais {
                email: usuario.email.clone(),
                senha: usuario.senha.clone(),
            },
            None,
            None,
        )
        .await
        .expect("login deveria ter sucesso");

    assert!(!resultado.exige_troca_senha);
    assert_eq!(resultado.usuario.id, usuario.id);

    // O token deve conter os claims de ID, Email e Perfil
    let servico = ServicoJwt::novo("segredo_teste_uc01", 60);
    let claims = servico
        .validar(&resultado.token)
        .expect("token deveria ser valido");
    assert_eq!(claims.sub, usuario.id);
    assert_eq!(claims.email, usuario.email);
    assert_eq!(claims.perfil, PerfilUsuario::Tecnico);
}

#[tokio::test]
async fn login_com_senha_incorreta_retorna_credenciais_invalidas_e_audita() {
    let pool = preparar_banco().await;
    let usuario = criar_usuario(&pool, PerfilUsuario::Tecnico, StatusUsuario::Ativo, false).await;
    let caso = montar_caso_de_uso(&pool);

    let erro = caso
        .autenticar(
            Credenciais {
                email: usuario.email.clone(),
                senha: "senha_totalmente_errada".into(),
            },
            Some("10.0.0.1"),
            Some("rust-test"),
        )
        .await
        .expect_err("senha incorreta deveria falhar");

    assert!(matches!(erro, ErroAutenticacao::CredenciaisInvalidas));

    // Tabela auditoria deve conter a tentativa falha
    let total = contar_auditoria(&pool, usuario.id, acao::LOGIN_FALHA).await;
    assert_eq!(total, 1, "deveria haver 1 evento LOGIN_FALHA na auditoria");
}

#[tokio::test]
async fn login_de_conta_inativa_e_bloqueado_e_auditado() {
    let pool = preparar_banco().await;
    let usuario = criar_usuario(&pool, PerfilUsuario::Tecnico, StatusUsuario::Inativo, false).await;
    let caso = montar_caso_de_uso(&pool);

    let erro = caso
        .autenticar(
            Credenciais {
                email: usuario.email.clone(),
                senha: usuario.senha.clone(),
            },
            None,
            None,
        )
        .await
        .expect_err("conta inativa deveria ser bloqueada");

    assert!(matches!(erro, ErroAutenticacao::ContaInativa));

    let total = contar_auditoria(&pool, usuario.id, acao::LOGIN_BLOQUEADO).await;
    assert_eq!(
        total, 1,
        "deveria haver 1 evento LOGIN_BLOQUEADO na auditoria"
    );
}

#[tokio::test]
async fn primeiro_acesso_exige_troca_de_senha() {
    let pool = preparar_banco().await;
    let usuario = criar_usuario(&pool, PerfilUsuario::Tecnico, StatusUsuario::Ativo, true).await;
    let caso = montar_caso_de_uso(&pool);

    let resultado = caso
        .autenticar(
            Credenciais {
                email: usuario.email,
                senha: usuario.senha,
            },
            None,
            None,
        )
        .await
        .expect("login deveria ter sucesso (com sinalizacao de troca)");

    assert!(
        resultado.exige_troca_senha,
        "primeiro acesso deve exigir troca de senha (RN05)"
    );
}

#[tokio::test]
async fn troca_de_senha_limpa_exigencia_e_permite_login_com_nova_senha() {
    let pool = preparar_banco().await;
    let usuario = criar_usuario(
        &pool,
        PerfilUsuario::Coordenador,
        StatusUsuario::Ativo,
        true,
    )
    .await;
    let caso = montar_caso_de_uso(&pool);

    caso.trocar_senha(usuario.id, &usuario.senha, "NovaSenha#456", None, None)
        .await
        .expect("troca de senha deveria ter sucesso");

    // A senha antiga nao funciona mais
    let erro = caso
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
    assert!(matches!(erro, ErroAutenticacao::CredenciaisInvalidas));

    // A nova senha funciona e nao exige troca
    let resultado = caso
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
    assert!(
        !resultado.exige_troca_senha,
        "exigencia de troca deve ser limpa"
    );
}

#[tokio::test]
async fn login_de_usuario_inexistente_nao_revela_informacao() {
    let pool = preparar_banco().await;
    let caso = montar_caso_de_uso(&pool);

    let erro = caso
        .autenticar(
            Credenciais {
                email: "ninguem@inexistente.com".into(),
                senha: "qualquer_senha".into(),
            },
            None,
            None,
        )
        .await
        .expect_err("email inexistente deveria falhar");

    // Resposta generica: nao diferencia email inexistente de senha errada
    assert!(matches!(erro, ErroAutenticacao::CredenciaisInvalidas));
}

#[tokio::test]
async fn email_com_formato_invalido_e_rejeitado_antes_de_consultar_o_banco() {
    let pool = preparar_banco().await;
    let caso = montar_caso_de_uso(&pool);

    let erro = caso
        .autenticar(
            Credenciais {
                email: "email_sem_arroba".into(),
                senha: "qualquer".into(),
            },
            None,
            None,
        )
        .await
        .expect_err("email malformado deveria ser rejeitado");

    assert!(matches!(erro, ErroAutenticacao::EmailInvalido));
}

#[tokio::test]
async fn login_com_sucesso_registra_ultimo_acesso() {
    let pool = preparar_banco().await;
    let usuario = criar_usuario(&pool, PerfilUsuario::Tecnico, StatusUsuario::Ativo, false).await;
    let caso = montar_caso_de_uso(&pool);

    caso.autenticar(
        Credenciais {
            email: usuario.email,
            senha: usuario.senha,
        },
        None,
        None,
    )
    .await
    .expect("login deveria ter sucesso");

    let ultimo_acesso: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT ultimo_acesso_em FROM usuario WHERE id_usuario = $1")
            .bind(usuario.id)
            .fetch_one(&pool)
            .await
            .expect("deveria retornar o usuario");
    assert!(
        ultimo_acesso.is_some(),
        "ultimo_acesso_em deve ser preenchido apos login"
    );
}

// =============================================================================
// Fluxo HTTP do UC01 (endpoints /api/v1/auth)
// =============================================================================

/// Constroi o roteador com estado real (JWT default de desenvolvimento).
fn montar_roteador(pool: sqlx::PgPool) -> axum::Router {
    let estado =
        EstadoAplicacao::novo(pool, format!("/tmp/visita_plus_http_{}.db", sufixo_unico()));
    construir_roteador(estado)
}

#[tokio::test]
async fn endpoint_login_responde_200_com_token() {
    let pool = preparar_banco().await;
    let usuario = criar_usuario(
        &pool,
        PerfilUsuario::Coordenador,
        StatusUsuario::Ativo,
        false,
    )
    .await;
    let roteador = montar_roteador(pool);

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

    assert_eq!(resposta.status(), StatusCode::OK);

    let corpo_resposta = axum::body::to_bytes(resposta.into_body(), 1024 * 1024)
        .await
        .expect("falha ao ler o corpo");
    let json: serde_json::Value = serde_json::from_slice(&corpo_resposta).expect("json valido");

    assert!(json["token"].as_str().is_some(), "deve conter token");
    assert_eq!(json["usuario"]["perfil"], "Coordenador");
    assert_eq!(json["exige_troca_senha"], false);
}

#[tokio::test]
async fn endpoint_login_com_senha_errada_responde_400() {
    let pool = preparar_banco().await;
    let usuario = criar_usuario(&pool, PerfilUsuario::Tecnico, StatusUsuario::Ativo, false).await;
    let roteador = montar_roteador(pool);

    let corpo = serde_json::json!({
        "email": usuario.email,
        "senha": "senha_errada",
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
async fn endpoint_login_de_conta_inativa_responde_403() {
    let pool = preparar_banco().await;
    let usuario = criar_usuario(&pool, PerfilUsuario::Tecnico, StatusUsuario::Inativo, false).await;
    let roteador = montar_roteador(pool);

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

    assert_eq!(resposta.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn endpoint_trocar_senha_aceita_token_valido() {
    let pool = preparar_banco().await;
    let usuario = criar_usuario(&pool, PerfilUsuario::Tecnico, StatusUsuario::Ativo, true).await;
    // Mesma resolucao de segredo do EstadoAplicacao::novo, para o token
    // bater com o servico de validacao do roteador mesmo com .env presente.
    let segredo = std::env::var("JWT_SEGREDO")
        .unwrap_or_else(|_| "segredo_de_desenvolvimento_visita_plus".to_string());
    let servico = ServicoJwt::novo(segredo, 60);
    // Sessao ativa vinculada ao token (exigida pelo extrator, RF04)
    let id_sessao = uuid::Uuid::new_v4();
    comum::criar_sessao_ativa(&pool, id_sessao, usuario.id).await;
    let token = servico
        .emitir(
            &visita_plus::dominio::entidades::usuario::Usuario {
                id: usuario.id,
                id_unidade: usuario.id_unidade,
                nome: "Usuario Teste".into(),
                email: usuario.email.clone(),
                senha_hash: "nao_utilizado".into(),
                perfil: PerfilUsuario::Tecnico,
                status: StatusUsuario::Ativo,
                exige_troca_senha: true,
                ultimo_acesso_em: None,
            },
            id_sessao,
        )
        .expect("deve emitir token");

    let roteador = montar_roteador(pool);
    let corpo = serde_json::json!({
        "senha_atual": usuario.senha,
        "nova_senha": "OutraSenha#789",
    });

    let resposta = roteador
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/trocar-senha")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::from(corpo.to_string()))
                .expect("requisicao valida"),
        )
        .await
        .expect("falha ao executar a requisicao");

    assert_eq!(resposta.status(), StatusCode::NO_CONTENT);
}
