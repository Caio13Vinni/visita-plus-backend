// =============================================================================
// Testes de integração — UC05 (Registrar visita)
// -----------------------------------------------------------------------------
// Exige o PostgreSQL do docker-compose em execucao:
//     docker compose up -d postgres
//     DATABASE_URL=postgres://... cargo test --test uc05_registro_visita
// =============================================================================

mod comum;

use std::sync::Arc;

use chrono::{Duration, NaiveTime, Utc};
use comum::*;
use visita_plus::casos_de_uso::agendamento::CasoDeUsoAgendamento;
use visita_plus::casos_de_uso::registro::CasoDeUsoRegistro;
use visita_plus::dominio::entidades::registro_visita::{DadosRegistro, NovoComplemento};
use visita_plus::dominio::entidades::usuario::Usuario;
use visita_plus::dominio::entidades::visita::CadastroVisita;
use visita_plus::dominio::enums::{
    PerfilUsuario, PrioridadeVisita, ResultadoRegistro, StatusRegistro, StatusUsuario, StatusVisita,
};
use visita_plus::dominio::erros::ErroRegistro;
use visita_plus::infraestrutura::auditoria::{ServicoAuditoria, acao};
use visita_plus::infraestrutura::repositorios::familia_repositorio::RepositorioFamiliaPostgres;
use visita_plus::infraestrutura::repositorios::registro_repositorio::RepositorioRegistroPostgres;
use visita_plus::infraestrutura::repositorios::usuario_repositorio::RepositorioUsuarioPostgres;
use visita_plus::infraestrutura::repositorios::visita_repositorio::RepositorioVisitaPostgres;

// =============================================================================
// Helpers de montagem
// =============================================================================

struct SuitesRegistro {
    agendamento: CasoDeUsoAgendamento,
    registro: CasoDeUsoRegistro,
}

fn montar(pool: &sqlx::PgPool) -> SuitesRegistro {
    let repositorio_usuario = Arc::new(RepositorioUsuarioPostgres::novo(pool.clone()));
    let repositorio_familia = Arc::new(RepositorioFamiliaPostgres::novo(pool.clone()));
    let repositorio_visita = Arc::new(RepositorioVisitaPostgres::novo(pool.clone()));
    let auditoria = ServicoAuditoria::novo(pool.clone());

    SuitesRegistro {
        agendamento: CasoDeUsoAgendamento::novo(
            repositorio_visita.clone(),
            repositorio_familia.clone(),
            repositorio_usuario.clone(),
            auditoria.clone(),
        ),
        registro: CasoDeUsoRegistro::novo(
            Arc::new(RepositorioRegistroPostgres::novo(pool.clone())),
            repositorio_visita,
            repositorio_familia,
            repositorio_usuario,
            auditoria,
        ),
    }
}

/// Coordenador + tecnico da mesma unidade + familia + visita agendada.
struct Cenario {
    coordenador: uuid::Uuid,
    tecnico: uuid::Uuid,
    visita: uuid::Uuid,
}

async fn cenario_com_visita_agendada(pool: &sqlx::PgPool, token: &str) -> Cenario {
    let coordenador = criar_usuario(
        pool,
        PerfilUsuario::Coordenador,
        StatusUsuario::Ativo,
        false,
    )
    .await;
    let tecnico = criar_usuario_na_unidade(
        pool,
        coordenador.id_unidade,
        PerfilUsuario::Tecnico,
        StatusUsuario::Ativo,
        false,
    )
    .await;
    let familia = criar_familia(pool, coordenador.id_unidade, &format!("FAM-{token}")).await;

    let suites = montar(pool);
    let visita = suites
        .agendamento
        .agendar_visita(
            coordenador.id,
            CadastroVisita {
                id_familia: familia,
                id_tecnico: tecnico.id,
                data_visita: Utc::now().date_naive() + Duration::days(2),
                horario: NaiveTime::from_hms_opt(9, 0, 0).expect("horario valido"),
                prioridade: PrioridadeVisita::Normal,
                objetivo: "Acompanhamento familiar".into(),
            },
        )
        .await
        .expect("visita agendada");

    Cenario {
        coordenador: coordenador.id,
        tecnico: tecnico.id,
        visita: visita.id,
    }
}

fn dados_realizada() -> DadosRegistro {
    DadosRegistro {
        resultado: Some(ResultadoRegistro::Realizada),
        descricao: Some("Familia recebida em casa; todos bem de saude.".into()),
        desfecho: Some("Acompanhamento mensal mantido".into()),
        necessita_retorno: false,
        motivo_nao_realizada: None,
    }
}

fn dados_nao_realizada() -> DadosRegistro {
    DadosRegistro {
        resultado: Some(ResultadoRegistro::NaoRealizada),
        descricao: None,
        desfecho: None,
        necessita_retorno: true,
        motivo_nao_realizada: Some("Familia ausente apos tres tentativas".into()),
    }
}

// =============================================================================
// Inicio da visita e rascunho (RN12)
// =============================================================================

#[tokio::test]
async fn tecnico_inicia_visita_e_cria_registro_em_rascunho() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_agendada(&pool, &sufixo_unico()).await;
    let suites = montar(&pool);

    let registro = suites
        .registro
        .iniciar_visita(cenario.tecnico, cenario.visita)
        .await
        .expect("tecnico deveria iniciar a visita");

    assert_eq!(registro.status, StatusRegistro::Rascunho);
    assert_eq!(registro.registrado_por, cenario.tecnico);
    assert_eq!(registro.id_visita, cenario.visita);
    assert!(
        registro.data_hora_inicio.is_some(),
        "inicio da visita deve ser registrado"
    );
    assert!(registro.resultado.is_none());

    let visita = suites
        .agendamento
        .detalhar_visita(cenario.tecnico, cenario.visita)
        .await
        .expect("visita");
    assert_eq!(
        visita.status,
        StatusVisita::EmAndamento,
        "visita deve passar para 'Em andamento'"
    );

    let eventos = contar_auditoria(&pool, cenario.tecnico, acao::REGISTRO_INICIADO).await;
    assert_eq!(eventos, 1, "inicio deve gerar trilha de auditoria (RN22)");
}

#[tokio::test]
async fn coordenador_nao_inicia_visita_do_tecnico() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_agendada(&pool, &sufixo_unico()).await;
    let suites = montar(&pool);

    let erro = suites
        .registro
        .iniciar_visita(cenario.coordenador, cenario.visita)
        .await
        .expect_err("coordenador nao executa visita");

    assert!(matches!(erro, ErroRegistro::SemPermissao));
}

#[tokio::test]
async fn tecnico_nao_inicia_visita_de_outro_tecnico() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_agendada(&pool, &sufixo_unico()).await;
    let unidade =
        sqlx::query_scalar::<_, uuid::Uuid>("SELECT id_unidade FROM usuario WHERE id_usuario = $1")
            .bind(cenario.tecnico)
            .fetch_one(&pool)
            .await
            .expect("unidade do tecnico");
    let outro_tecnico = criar_usuario_na_unidade(
        &pool,
        unidade,
        PerfilUsuario::Tecnico,
        StatusUsuario::Ativo,
        false,
    )
    .await;
    let suites = montar(&pool);

    let erro = suites
        .registro
        .iniciar_visita(outro_tecnico.id, cenario.visita)
        .await
        .expect_err("tecnico nao inicia visita de outro");

    assert!(matches!(erro, ErroRegistro::SemPermissao));
}

#[tokio::test]
async fn visita_cancelada_nao_pode_ser_iniciada() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_agendada(&pool, &sufixo_unico()).await;
    let suites = montar(&pool);

    suites
        .agendamento
        .cancelar_visita(
            cenario.coordenador,
            cenario.visita,
            "Solicitacao da familia",
        )
        .await
        .expect("cancelamento");

    let erro = suites
        .registro
        .iniciar_visita(cenario.tecnico, cenario.visita)
        .await
        .expect_err("visita cancelada nao inicia");

    assert!(matches!(erro, ErroRegistro::VisitaNaoAgendada));
}

#[tokio::test]
async fn rascunho_pode_ser_salvo_sem_finalizar() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_agendada(&pool, &sufixo_unico()).await;
    let suites = montar(&pool);

    let registro = suites
        .registro
        .iniciar_visita(cenario.tecnico, cenario.visita)
        .await
        .expect("registro em rascunho");

    let mut dados = dados_realizada();
    dados.desfecho = None;

    let salvo = suites
        .registro
        .salvar_rascunho(cenario.tecnico, registro.id, &dados)
        .await
        .expect("rascunho salvo");

    assert_eq!(
        salvo.status,
        StatusRegistro::Rascunho,
        "RN12: continua rascunho"
    );
    assert_eq!(salvo.descricao, dados.descricao);
    assert!(salvo.data_hora_fim.is_none(), "rascunho nao tem termino");

    let visita = suites
        .agendamento
        .detalhar_visita(cenario.tecnico, cenario.visita)
        .await
        .expect("visita");
    assert_eq!(
        visita.status,
        StatusVisita::EmAndamento,
        "rascunho nao altera o status da visita"
    );
}

// =============================================================================
// Finalizacao (RN13 e RN14)
// =============================================================================

#[tokio::test]
async fn finalizar_realizada_sem_descricao_e_rejeitada() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_agendada(&pool, &sufixo_unico()).await;
    let suites = montar(&pool);
    let registro = suites
        .registro
        .iniciar_visita(cenario.tecnico, cenario.visita)
        .await
        .expect("registro em rascunho");

    let mut dados = dados_realizada();
    dados.descricao = Some("   ".into());

    let erro = suites
        .registro
        .finalizar_registro(cenario.tecnico, registro.id, &dados)
        .await
        .expect_err("RN13 exige descricao");

    assert!(matches!(erro, ErroRegistro::DadosInvalidos(_)));
}

#[tokio::test]
async fn finalizar_realizada_sem_desfecho_e_rejeitada() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_agendada(&pool, &sufixo_unico()).await;
    let suites = montar(&pool);
    let registro = suites
        .registro
        .iniciar_visita(cenario.tecnico, cenario.visita)
        .await
        .expect("registro em rascunho");

    let mut dados = dados_realizada();
    dados.desfecho = None;

    let erro = suites
        .registro
        .finalizar_registro(cenario.tecnico, registro.id, &dados)
        .await
        .expect_err("RN13 exige desfecho");

    assert!(matches!(erro, ErroRegistro::DadosInvalidos(_)));
}

#[tokio::test]
async fn tecnico_finaliza_visita_realizada_com_sucesso() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_agendada(&pool, &sufixo_unico()).await;
    let suites = montar(&pool);
    let registro = suites
        .registro
        .iniciar_visita(cenario.tecnico, cenario.visita)
        .await
        .expect("registro em rascunho");

    let finalizado = suites
        .registro
        .finalizar_registro(cenario.tecnico, registro.id, &dados_realizada())
        .await
        .expect("finalizacao valida");

    assert_eq!(finalizado.status, StatusRegistro::Finalizado);
    assert_eq!(finalizado.resultado, Some(ResultadoRegistro::Realizada));
    assert!(
        finalizado.data_hora_fim.is_some(),
        "RN13: horario de termino"
    );

    let visita = suites
        .agendamento
        .detalhar_visita(cenario.tecnico, cenario.visita)
        .await
        .expect("visita");
    assert_eq!(
        visita.status,
        StatusVisita::Realizada,
        "status da visita reflete o resultado"
    );

    let eventos = contar_auditoria(&pool, cenario.tecnico, acao::REGISTRO_FINALIZADO).await;
    assert_eq!(eventos, 1, "finalizacao deve ser auditada (RN22)");
}

#[tokio::test]
async fn finalizar_nao_realizada_sem_motivo_e_rejeitada() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_agendada(&pool, &sufixo_unico()).await;
    let suites = montar(&pool);
    let registro = suites
        .registro
        .iniciar_visita(cenario.tecnico, cenario.visita)
        .await
        .expect("registro em rascunho");

    let mut dados = dados_nao_realizada();
    dados.motivo_nao_realizada = None;

    let erro = suites
        .registro
        .finalizar_registro(cenario.tecnico, registro.id, &dados)
        .await
        .expect_err("RN14 exige motivo obrigatorio");

    assert!(matches!(erro, ErroRegistro::DadosInvalidos(_)));
}

#[tokio::test]
async fn tecnico_finaliza_visita_nao_realizada_com_motivo() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_agendada(&pool, &sufixo_unico()).await;
    let suites = montar(&pool);
    let registro = suites
        .registro
        .iniciar_visita(cenario.tecnico, cenario.visita)
        .await
        .expect("registro em rascunho");

    let finalizado = suites
        .registro
        .finalizar_registro(cenario.tecnico, registro.id, &dados_nao_realizada())
        .await
        .expect("finalizacao valida");

    assert_eq!(finalizado.resultado, Some(ResultadoRegistro::NaoRealizada));
    assert_eq!(
        finalizado.motivo_nao_realizada.as_deref(),
        Some("Familia ausente apos tres tentativas")
    );
    assert!(finalizado.necessita_retorno, "flag de retorno preservada");

    let visita = suites
        .agendamento
        .detalhar_visita(cenario.tecnico, cenario.visita)
        .await
        .expect("visita");
    assert_eq!(visita.status, StatusVisita::NaoRealizada);
}

// =============================================================================
// Imutabilidade e complementos (RN16 e RN17)
// =============================================================================

#[tokio::test]
async fn registro_finalizado_nao_pode_ser_editado() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_agendada(&pool, &sufixo_unico()).await;
    let suites = montar(&pool);
    let registro = suites
        .registro
        .iniciar_visita(cenario.tecnico, cenario.visita)
        .await
        .expect("registro em rascunho");
    suites
        .registro
        .finalizar_registro(cenario.tecnico, registro.id, &dados_realizada())
        .await
        .expect("finalizado");

    let erro = suites
        .registro
        .salvar_rascunho(cenario.tecnico, registro.id, &dados_realizada())
        .await
        .expect_err("RN16: finalizado e imutavel");

    assert!(matches!(erro, ErroRegistro::RegistroNaoRascunho));
}

#[tokio::test]
async fn coordenador_complementa_registro_finalizado() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_agendada(&pool, &sufixo_unico()).await;
    let suites = montar(&pool);
    let registro = suites
        .registro
        .iniciar_visita(cenario.tecnico, cenario.visita)
        .await
        .expect("registro em rascunho");
    suites
        .registro
        .finalizar_registro(cenario.tecnico, registro.id, &dados_realizada())
        .await
        .expect("finalizado");

    let complemento = NovoComplemento {
        descricao: "A familia informou novo numero de contato.".into(),
        motivo: "Correcao do contato apos finalizacao".into(),
    };

    let criado = suites
        .registro
        .adicionar_complemento(cenario.coordenador, registro.id, &complemento)
        .await
        .expect("RN17: complemento permitido");

    assert_eq!(criado.registrado_por, cenario.coordenador);

    let historico = suites
        .registro
        .listar_complementos(cenario.tecnico, registro.id)
        .await
        .expect("historico de complementos");
    assert_eq!(historico.len(), 1);
    assert_eq!(historico[0].motivo, complemento.motivo);

    let eventos = contar_auditoria(&pool, cenario.coordenador, acao::COMPLEMENTO_REGISTRADO).await;
    assert_eq!(eventos, 1, "complemento deve ser auditado (RN22)");
}

#[tokio::test]
async fn complemento_em_rascunho_e_rejeitado() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_agendada(&pool, &sufixo_unico()).await;
    let suites = montar(&pool);
    let registro = suites
        .registro
        .iniciar_visita(cenario.tecnico, cenario.visita)
        .await
        .expect("registro em rascunho");

    let complemento = NovoComplemento {
        descricao: "Tentativa de corrigir rascunho".into(),
        motivo: "Motivo qualquer".into(),
    };

    let erro = suites
        .registro
        .adicionar_complemento(cenario.tecnico, registro.id, &complemento)
        .await
        .expect_err("RN17: complemento exige registro finalizado");

    assert!(matches!(erro, ErroRegistro::RegistroNaoFinalizado));
}

#[tokio::test]
async fn complemento_sem_motivo_justificativa_e_rejeitado() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_agendada(&pool, &sufixo_unico()).await;
    let suites = montar(&pool);
    let registro = suites
        .registro
        .iniciar_visita(cenario.tecnico, cenario.visita)
        .await
        .expect("registro em rascunho");
    suites
        .registro
        .finalizar_registro(cenario.tecnico, registro.id, &dados_realizada())
        .await
        .expect("finalizado");

    let complemento = NovoComplemento {
        descricao: "Descricao ok".into(),
        motivo: "   ".into(),
    };

    let erro = suites
        .registro
        .adicionar_complemento(cenario.tecnico, registro.id, &complemento)
        .await
        .expect_err("RN17 exige justificativa");

    assert!(matches!(erro, ErroRegistro::DadosInvalidos(_)));
}

#[tokio::test]
async fn tecnico_de_outra_unidade_nao_enxerga_o_registro() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_agendada(&pool, &sufixo_unico()).await;
    // criar_usuario abre uma NOVA unidade a cada chamada.
    let tecnico_outra_unidade =
        criar_usuario(&pool, PerfilUsuario::Tecnico, StatusUsuario::Ativo, false).await;
    let suites = montar(&pool);

    suites
        .registro
        .iniciar_visita(cenario.tecnico, cenario.visita)
        .await
        .expect("registro em rascunho");

    let erro = suites
        .registro
        .detalhar_por_visita(tecnico_outra_unidade.id, cenario.visita)
        .await
        .expect_err("RN05/RN06: registro invisivel fora da unidade");

    // Sem revelar a existencia do registro.
    assert!(matches!(erro, ErroRegistro::NaoEncontrado));
}

// =============================================================================
// Fluxo HTTP do UC05 (endpoints /api/v1/registros)
// =============================================================================

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use tower::ServiceExt;
use visita_plus::infraestrutura::seguranca::jwt::ServicoJwt;
use visita_plus::interface::api::construir_roteador;
use visita_plus::interface::estado::EstadoAplicacao;

fn montar_roteador(pool: sqlx::PgPool) -> axum::Router {
    let estado =
        EstadoAplicacao::novo(pool, format!("/tmp/visita_plus_http_{}.db", sufixo_unico()));
    construir_roteador(estado)
}

async fn emitir_token(
    pool: &sqlx::PgPool,
    id: uuid::Uuid,
    id_unidade: uuid::Uuid,
    perfil: PerfilUsuario,
) -> String {
    let segredo = std::env::var("JWT_SEGREDO")
        .unwrap_or_else(|_| "segredo_de_desenvolvimento_visita_plus".to_string());
    let servico = ServicoJwt::novo(segredo, 60);
    // Sessao ativa vinculada ao token (exigida pelo extrator, RF04)
    let id_sessao = uuid::Uuid::new_v4();
    criar_sessao_ativa(pool, id_sessao, id).await;
    servico
        .emitir(
            &Usuario {
                id,
                id_unidade,
                nome: "Usuario Teste".into(),
                email: "tecnico@teste.com".into(),
                senha_hash: "nao_utilizado".into(),
                perfil,
                status: StatusUsuario::Ativo,
                exige_troca_senha: false,
                ultimo_acesso_em: None,
            },
            id_sessao,
        )
        .expect("deve emitir token")
}

#[tokio::test]
async fn endpoint_inicia_visita_responde_201() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_agendada(&pool, &sufixo_unico()).await;
    let roteador = montar_roteador(pool.clone());
    let unidade =
        sqlx::query_scalar::<_, uuid::Uuid>("SELECT id_unidade FROM usuario WHERE id_usuario = $1")
            .bind(cenario.tecnico)
            .fetch_one(&pool)
            .await
            .expect("unidade do tecnico");
    let token = emitir_token(&pool, cenario.tecnico, unidade, PerfilUsuario::Tecnico).await;

    let corpo = serde_json::json!({ "id_visita": cenario.visita });

    let resposta = roteador
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/registros")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::from(corpo.to_string()))
                .expect("requisicao valida"),
        )
        .await
        .expect("falha ao executar a requisicao");

    assert_eq!(resposta.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn endpoint_finaliza_registro_responde_200_e_visita_realizada() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_agendada(&pool, &sufixo_unico()).await;
    let suites = montar(&pool);
    let registro = suites
        .registro
        .iniciar_visita(cenario.tecnico, cenario.visita)
        .await
        .expect("registro em rascunho");

    let roteador = montar_roteador(pool.clone());
    let unidade =
        sqlx::query_scalar::<_, uuid::Uuid>("SELECT id_unidade FROM usuario WHERE id_usuario = $1")
            .bind(cenario.tecnico)
            .fetch_one(&pool)
            .await
            .expect("unidade do tecnico");
    let token = emitir_token(&pool, cenario.tecnico, unidade, PerfilUsuario::Tecnico).await;

    let corpo = serde_json::json!({
        "resultado": "Realizada",
        "descricao": "Visita concluida com a familia presente",
        "desfecho": "Acompanhamento trimestral agendado",
        "necessita_retorno": false
    });

    let resposta = roteador
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/registros/{}/finalizar", registro.id))
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::from(corpo.to_string()))
                .expect("requisicao valida"),
        )
        .await
        .expect("falha ao executar a requisicao");

    assert_eq!(resposta.status(), StatusCode::OK);

    let visita = suites
        .agendamento
        .detalhar_visita(cenario.tecnico, cenario.visita)
        .await
        .expect("visita");
    assert_eq!(visita.status, StatusVisita::Realizada);
}

#[tokio::test]
async fn endpoint_complemento_em_rascunho_responde_400() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_agendada(&pool, &sufixo_unico()).await;
    let suites = montar(&pool);
    let registro = suites
        .registro
        .iniciar_visita(cenario.tecnico, cenario.visita)
        .await
        .expect("registro em rascunho");

    let roteador = montar_roteador(pool.clone());
    let unidade =
        sqlx::query_scalar::<_, uuid::Uuid>("SELECT id_unidade FROM usuario WHERE id_usuario = $1")
            .bind(cenario.tecnico)
            .fetch_one(&pool)
            .await
            .expect("unidade do tecnico");
    let token = emitir_token(&pool, cenario.tecnico, unidade, PerfilUsuario::Tecnico).await;

    let corpo = serde_json::json!({
        "descricao": "Tentativa em rascunho",
        "motivo": "Motivo"
    });

    let resposta = roteador
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/registros/{}/complementos", registro.id))
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::from(corpo.to_string()))
                .expect("requisicao valida"),
        )
        .await
        .expect("falha ao executar a requisicao");

    assert_eq!(resposta.status(), StatusCode::BAD_REQUEST);
}
