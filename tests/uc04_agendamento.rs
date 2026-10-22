// =============================================================================
// Testes de integração — UC04 (Agendar visitas)
// -----------------------------------------------------------------------------
// Exige o PostgreSQL do docker-compose em execucao:
//     docker compose up -d postgres
//     DATABASE_URL=postgres://... cargo test --test uc04_agendamento
// =============================================================================

mod comum;

use std::sync::Arc;

use chrono::{Duration, NaiveTime, Utc};
use comum::*;
use visita_plus::casos_de_uso::agendamento::CasoDeUsoAgendamento;
use visita_plus::dominio::entidades::usuario::Usuario;
use visita_plus::dominio::entidades::visita::{CadastroVisita, ReagendamentoVisita};
use visita_plus::dominio::enums::{PerfilUsuario, StatusUsuario, StatusVisita};
use visita_plus::dominio::erros::ErroAgendamento;
use visita_plus::infraestrutura::auditoria::{ServicoAuditoria, acao};
use visita_plus::infraestrutura::repositorios::familia_repositorio::RepositorioFamiliaPostgres;
use visita_plus::infraestrutura::repositorios::usuario_repositorio::RepositorioUsuarioPostgres;
use visita_plus::infraestrutura::repositorios::visita_repositorio::{
    FiltroVisita, RepositorioVisita, RepositorioVisitaPostgres,
};

// =============================================================================
// Helpers de montagem
// =============================================================================

fn montar_agendamento(pool: &sqlx::PgPool) -> CasoDeUsoAgendamento {
    let repositorio_usuario = Arc::new(RepositorioUsuarioPostgres::novo(pool.clone()));
    let repositorio_familia = Arc::new(RepositorioFamiliaPostgres::novo(pool.clone()));
    let repositorio_visita = Arc::new(RepositorioVisitaPostgres::novo(pool.clone()));

    CasoDeUsoAgendamento::novo(
        repositorio_visita,
        repositorio_familia,
        repositorio_usuario,
        ServicoAuditoria::novo(pool.clone()),
    )
}

/// Coordenador + tecnico da mesma unidade + familia ativa.
struct Cenario {
    coordenador: uuid::Uuid,
    tecnico: uuid::Uuid,
    familia: uuid::Uuid,
}

async fn cenario_padrao(pool: &sqlx::PgPool, token: &str) -> Cenario {
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

    Cenario {
        coordenador: coordenador.id,
        tecnico: tecnico.id,
        familia,
    }
}

fn cadastro_visita(
    id_familia: uuid::Uuid,
    id_tecnico: uuid::Uuid,
    daqui_a_dias: i64,
    hora: u32,
) -> CadastroVisita {
    CadastroVisita {
        id_familia,
        id_tecnico,
        data_visita: Utc::now().date_naive() + Duration::days(daqui_a_dias),
        horario: NaiveTime::from_hms_opt(hora, 0, 0).expect("horario valido"),
        prioridade: visita_plus::dominio::enums::PrioridadeVisita::Normal,
        objetivo: "Acompanhamento familiar".into(),
    }
}

// =============================================================================
// Agendamento — regras RN09 (retroativo) e RN10 (conflito)
// =============================================================================

#[tokio::test]
async fn coordenador_agenda_visita_futura_com_sucesso() {
    let pool = preparar_banco().await;
    let cenario = cenario_padrao(&pool, &sufixo_unico()).await;
    let agendamento = montar_agendamento(&pool);

    let visita = agendamento
        .agendar_visita(
            cenario.coordenador,
            cadastro_visita(cenario.familia, cenario.tecnico, 2, 9),
        )
        .await
        .expect("coordenador deveria agendar a visita");

    assert_eq!(visita.status, StatusVisita::Agendada);
    assert_eq!(visita.id_agendado_por, cenario.coordenador);
    assert_eq!(visita.id_tecnico, cenario.tecnico);
    assert!(visita.motivo_cancelamento.is_none());

    let eventos = contar_auditoria(&pool, cenario.coordenador, acao::VISITA_AGENDADA).await;
    assert_eq!(
        eventos, 1,
        "agendamento deve gerar trilha de auditoria (RN22)"
    );
}

#[tokio::test]
async fn agendamento_retroativo_e_rejeitado() {
    let pool = preparar_banco().await;
    let cenario = cenario_padrao(&pool, &sufixo_unico()).await;
    let agendamento = montar_agendamento(&pool);

    // Dois horas no passado: cobre data de ontem e horario ja passado hoje.
    let passado = Utc::now() - Duration::hours(2);
    let mut cadastro = cadastro_visita(cenario.familia, cenario.tecnico, 0, 12);
    cadastro.data_visita = passado.date_naive();
    cadastro.horario = passado.time();

    let erro = agendamento
        .agendar_visita(cenario.coordenador, cadastro)
        .await
        .expect_err("agendamento retroativo deve falhar");

    assert!(matches!(erro, ErroAgendamento::DataRetroativa), "RN09");
}

#[tokio::test]
async fn conflito_de_horario_na_agenda_do_tecnico_e_rejeitado() {
    let pool = preparar_banco().await;
    let cenario = cenario_padrao(&pool, &sufixo_unico()).await;
    let agendamento = montar_agendamento(&pool);

    agendamento
        .agendar_visita(
            cenario.coordenador,
            cadastro_visita(cenario.familia, cenario.tecnico, 3, 14),
        )
        .await
        .expect("primeira visita deve ser agendada");

    let erro = agendamento
        .agendar_visita(
            cenario.coordenador,
            cadastro_visita(cenario.familia, cenario.tecnico, 3, 14),
        )
        .await
        .expect_err("mesmo tecnico, mesma data e horario deve conflitar");

    assert!(matches!(erro, ErroAgendamento::ConflitoDeHorario), "RN10");
}

#[tokio::test]
async fn mesmo_horario_em_tecnico_diferente_nao_conflita() {
    let pool = preparar_banco().await;
    let cenario = cenario_padrao(&pool, &sufixo_unico()).await;
    let unidade =
        sqlx::query_scalar::<_, uuid::Uuid>("SELECT id_unidade FROM familia WHERE id_familia = $1")
            .bind(cenario.familia)
            .fetch_one(&pool)
            .await
            .expect("unidade da familia");
    let outro_tecnico = criar_usuario_na_unidade(
        &pool,
        unidade,
        PerfilUsuario::Tecnico,
        StatusUsuario::Ativo,
        false,
    )
    .await;
    let agendamento = montar_agendamento(&pool);

    agendamento
        .agendar_visita(
            cenario.coordenador,
            cadastro_visita(cenario.familia, cenario.tecnico, 3, 14),
        )
        .await
        .expect("primeira visita deve ser agendada");

    let segunda = agendamento
        .agendar_visita(
            cenario.coordenador,
            cadastro_visita(cenario.familia, outro_tecnico.id, 3, 14),
        )
        .await
        .expect("agenda de outro tecnico nao conflita");

    assert_eq!(segunda.id_tecnico, outro_tecnico.id);
}

#[tokio::test]
async fn familia_inativa_nao_recebe_nova_visita() {
    let pool = preparar_banco().await;
    let cenario = cenario_padrao(&pool, &sufixo_unico()).await;
    let agendamento = montar_agendamento(&pool);

    sqlx::query("UPDATE familia SET status = 'Inativa' WHERE id_familia = $1")
        .bind(cenario.familia)
        .execute(&pool)
        .await
        .expect("falha ao inativar familia");

    let erro = agendamento
        .agendar_visita(
            cenario.coordenador,
            cadastro_visita(cenario.familia, cenario.tecnico, 2, 9),
        )
        .await
        .expect_err("familia inativa nao deve receber visita");

    assert!(matches!(erro, ErroAgendamento::FamiliaInativa));
}

#[tokio::test]
async fn tecnico_de_outra_unidade_nao_recebe_visita() {
    let pool = preparar_banco().await;
    let cenario = cenario_padrao(&pool, &sufixo_unico()).await;
    // criar_usuario abre uma NOVA unidade a cada chamada.
    let tecnico_outra_unidade =
        criar_usuario(&pool, PerfilUsuario::Tecnico, StatusUsuario::Ativo, false).await;
    let agendamento = montar_agendamento(&pool);

    let erro = agendamento
        .agendar_visita(
            cenario.coordenador,
            cadastro_visita(cenario.familia, tecnico_outra_unidade.id, 2, 9),
        )
        .await
        .expect_err("tecnico de outra unidade deve ser rejeitado");

    assert!(matches!(erro, ErroAgendamento::TecnicoInvalido));
}

// =============================================================================
// Permissoes — RN05/RN06 (tecnico so na propria agenda)
// =============================================================================

#[tokio::test]
async fn tecnico_nao_agenda_para_outro_tecnico() {
    let pool = preparar_banco().await;
    let cenario = cenario_padrao(&pool, &sufixo_unico()).await;
    let outro_tecnico = criar_usuario_na_unidade(
        &pool,
        sqlx::query_scalar::<_, uuid::Uuid>("SELECT id_unidade FROM familia WHERE id_familia = $1")
            .bind(cenario.familia)
            .fetch_one(&pool)
            .await
            .expect("unidade da familia"),
        PerfilUsuario::Tecnico,
        StatusUsuario::Ativo,
        false,
    )
    .await;
    let agendamento = montar_agendamento(&pool);

    let erro = agendamento
        .agendar_visita(
            cenario.tecnico,
            cadastro_visita(cenario.familia, outro_tecnico.id, 2, 9),
        )
        .await
        .expect_err("tecnico nao deveria agendar para outro tecnico");

    assert!(matches!(erro, ErroAgendamento::SemPermissao));
}

#[tokio::test]
async fn tecnico_agenda_na_propria_agenda() {
    let pool = preparar_banco().await;
    let cenario = cenario_padrao(&pool, &sufixo_unico()).await;
    let agendamento = montar_agendamento(&pool);

    let visita = agendamento
        .agendar_visita(
            cenario.tecnico,
            cadastro_visita(cenario.familia, cenario.tecnico, 2, 10),
        )
        .await
        .expect("tecnico deveria agendar na propria agenda");

    assert_eq!(visita.id_agendado_por, cenario.tecnico);
}

#[tokio::test]
async fn tecnico_so_enxerga_a_propria_agenda() {
    let pool = preparar_banco().await;
    let cenario = cenario_padrao(&pool, &sufixo_unico()).await;
    let unidade =
        sqlx::query_scalar::<_, uuid::Uuid>("SELECT id_unidade FROM familia WHERE id_familia = $1")
            .bind(cenario.familia)
            .fetch_one(&pool)
            .await
            .expect("unidade da familia");
    let outro_tecnico = criar_usuario_na_unidade(
        &pool,
        unidade,
        PerfilUsuario::Tecnico,
        StatusUsuario::Ativo,
        false,
    )
    .await;
    let agendamento = montar_agendamento(&pool);

    let minha = agendamento
        .agendar_visita(
            cenario.tecnico,
            cadastro_visita(cenario.familia, cenario.tecnico, 2, 9),
        )
        .await
        .expect("visita do tecnico A");
    let de_outro = agendamento
        .agendar_visita(
            cenario.coordenador,
            cadastro_visita(cenario.familia, outro_tecnico.id, 2, 11),
        )
        .await
        .expect("visita do tecnico B");

    // RN05/RN06: o tecnico A lista e so ve a propria visita.
    let filtro = FiltroVisita {
        id_unidade: Some(unidade),
        ..Default::default()
    };
    let visitas = agendamento
        .listar_visitas(cenario.tecnico, filtro)
        .await
        .expect("listagem do tecnico");

    assert!(
        visitas.iter().any(|visita| visita.id == minha.id),
        "tecnico deve ver a propria visita"
    );
    assert!(
        !visitas.iter().any(|visita| visita.id == de_outro.id),
        "tecnico nao deve ver visita de outro tecnico (RN05/RN06)"
    );

    // O coordenador enxerga a agenda geral da unidade.
    let filtro_geral = FiltroVisita {
        id_unidade: Some(unidade),
        ..Default::default()
    };
    let agenda_geral = agendamento
        .listar_visitas(cenario.coordenador, filtro_geral)
        .await
        .expect("listagem do coordenador");
    assert_eq!(agenda_geral.len(), 2, "coordenador ve a agenda completa");
}

// =============================================================================
// Reagendamento e cancelamento — RN18 (historico preservado)
// =============================================================================

#[tokio::test]
async fn reagendamento_para_horario_livre_funciona_e_audita() {
    let pool = preparar_banco().await;
    let cenario = cenario_padrao(&pool, &sufixo_unico()).await;
    let agendamento = montar_agendamento(&pool);

    let visita = agendamento
        .agendar_visita(
            cenario.coordenador,
            cadastro_visita(cenario.familia, cenario.tecnico, 2, 9),
        )
        .await
        .expect("visita agendada");

    let reagendamento = ReagendamentoVisita {
        data_visita: Utc::now().date_naive() + Duration::days(6),
        horario: NaiveTime::from_hms_opt(15, 30, 0).expect("horario valido"),
        id_tecnico: None,
    };

    let atualizada = agendamento
        .reagendar_visita(cenario.coordenador, visita.id, reagendamento)
        .await
        .expect("reagendamento deveria funcionar");

    assert_eq!(
        atualizada.data_visita,
        Utc::now().date_naive() + Duration::days(6)
    );
    assert_eq!(
        atualizada.horario,
        NaiveTime::from_hms_opt(15, 30, 0).unwrap()
    );
    assert_eq!(atualizada.id_tecnico, cenario.tecnico);
    assert_eq!(atualizada.status, StatusVisita::Agendada);

    let eventos = contar_auditoria(&pool, cenario.coordenador, acao::VISITA_REAGENDADA).await;
    assert_eq!(
        eventos, 1,
        "reagendamento deve ficar no historico (RN18/RN22)"
    );
}

#[tokio::test]
async fn reagendamento_para_horario_ocupado_conflita() {
    let pool = preparar_banco().await;
    let cenario = cenario_padrao(&pool, &sufixo_unico()).await;
    let agendamento = montar_agendamento(&pool);

    agendamento
        .agendar_visita(
            cenario.coordenador,
            cadastro_visita(cenario.familia, cenario.tecnico, 4, 9),
        )
        .await
        .expect("primeira visita");

    let segunda = agendamento
        .agendar_visita(
            cenario.coordenador,
            cadastro_visita(cenario.familia, cenario.tecnico, 4, 14),
        )
        .await
        .expect("segunda visita em horario livre");

    let reagendamento = ReagendamentoVisita {
        data_visita: Utc::now().date_naive() + Duration::days(4),
        horario: NaiveTime::from_hms_opt(9, 0, 0).expect("horario valido"),
        id_tecnico: None,
    };

    let erro = agendamento
        .reagendar_visita(cenario.coordenador, segunda.id, reagendamento)
        .await
        .expect_err("reagendamento para horario ocupado deve falhar");

    assert!(matches!(erro, ErroAgendamento::ConflitoDeHorario), "RN10");
}

#[tokio::test]
async fn coordenador_troca_o_responsavel_pela_visita() {
    let pool = preparar_banco().await;
    let cenario = cenario_padrao(&pool, &sufixo_unico()).await;
    let unidade =
        sqlx::query_scalar::<_, uuid::Uuid>("SELECT id_unidade FROM familia WHERE id_familia = $1")
            .bind(cenario.familia)
            .fetch_one(&pool)
            .await
            .expect("unidade da familia");
    let novo_tecnico = criar_usuario_na_unidade(
        &pool,
        unidade,
        PerfilUsuario::Tecnico,
        StatusUsuario::Ativo,
        false,
    )
    .await;
    let agendamento = montar_agendamento(&pool);

    let visita = agendamento
        .agendar_visita(
            cenario.coordenador,
            cadastro_visita(cenario.familia, cenario.tecnico, 2, 9),
        )
        .await
        .expect("visita agendada");

    let reagendamento = ReagendamentoVisita {
        data_visita: Utc::now().date_naive() + Duration::days(2),
        horario: NaiveTime::from_hms_opt(9, 0, 0).expect("horario valido"),
        id_tecnico: Some(novo_tecnico.id),
    };

    let atualizada = agendamento
        .reagendar_visita(cenario.coordenador, visita.id, reagendamento)
        .await
        .expect("troca de responsavel deveria funcionar");

    assert_eq!(atualizada.id_tecnico, novo_tecnico.id);
}

#[tokio::test]
async fn tecnico_nao_reagenda_para_trocar_o_responsavel() {
    let pool = preparar_banco().await;
    let cenario = cenario_padrao(&pool, &sufixo_unico()).await;
    let unidade =
        sqlx::query_scalar::<_, uuid::Uuid>("SELECT id_unidade FROM familia WHERE id_familia = $1")
            .bind(cenario.familia)
            .fetch_one(&pool)
            .await
            .expect("unidade da familia");
    let outro_tecnico = criar_usuario_na_unidade(
        &pool,
        unidade,
        PerfilUsuario::Tecnico,
        StatusUsuario::Ativo,
        false,
    )
    .await;
    let agendamento = montar_agendamento(&pool);

    let visita = agendamento
        .agendar_visita(
            cenario.tecnico,
            cadastro_visita(cenario.familia, cenario.tecnico, 2, 9),
        )
        .await
        .expect("visita agendada pelo tecnico");

    let reagendamento = ReagendamentoVisita {
        data_visita: Utc::now().date_naive() + Duration::days(2),
        horario: NaiveTime::from_hms_opt(9, 0, 0).expect("horario valido"),
        id_tecnico: Some(outro_tecnico.id),
    };

    let erro = agendamento
        .reagendar_visita(cenario.tecnico, visita.id, reagendamento)
        .await
        .expect_err("tecnico nao deveria transferir a visita");

    assert!(matches!(erro, ErroAgendamento::SemPermissao));
}

#[tokio::test]
async fn cancelamento_exige_motivo_obrigatorio() {
    let pool = preparar_banco().await;
    let cenario = cenario_padrao(&pool, &sufixo_unico()).await;
    let agendamento = montar_agendamento(&pool);

    let visita = agendamento
        .agendar_visita(
            cenario.coordenador,
            cadastro_visita(cenario.familia, cenario.tecnico, 2, 9),
        )
        .await
        .expect("visita agendada");

    let erro = agendamento
        .cancelar_visita(cenario.coordenador, visita.id, "   ")
        .await
        .expect_err("cancelamento sem motivo deve falhar");

    assert!(matches!(erro, ErroAgendamento::MotivoObrigatorio));
}

#[tokio::test]
async fn coordenador_cancela_visita_agendada_e_audita() {
    let pool = preparar_banco().await;
    let cenario = cenario_padrao(&pool, &sufixo_unico()).await;
    let agendamento = montar_agendamento(&pool);

    let visita = agendamento
        .agendar_visita(
            cenario.coordenador,
            cadastro_visita(cenario.familia, cenario.tecnico, 2, 9),
        )
        .await
        .expect("visita agendada");

    let cancelada = agendamento
        .cancelar_visita(cenario.coordenador, visita.id, "Familia viajou")
        .await
        .expect("cancelamento deveria funcionar");

    assert_eq!(cancelada.status, StatusVisita::Cancelada);
    assert_eq!(
        cancelada.motivo_cancelamento.as_deref(),
        Some("Familia viajou")
    );

    let eventos = contar_auditoria(&pool, cenario.coordenador, acao::VISITA_CANCELADA).await;
    assert_eq!(eventos, 1, "cancelamento deve ficar no historico (RN18)");
}

#[tokio::test]
async fn visita_cancelada_nao_pode_ser_alterada() {
    let pool = preparar_banco().await;
    let cenario = cenario_padrao(&pool, &sufixo_unico()).await;
    let agendamento = montar_agendamento(&pool);

    let visita = agendamento
        .agendar_visita(
            cenario.coordenador,
            cadastro_visita(cenario.familia, cenario.tecnico, 2, 9),
        )
        .await
        .expect("visita agendada");

    agendamento
        .cancelar_visita(cenario.coordenador, visita.id, "Solicitacao da familia")
        .await
        .expect("cancelamento");

    let reagendamento = ReagendamentoVisita {
        data_visita: Utc::now().date_naive() + Duration::days(7),
        horario: NaiveTime::from_hms_opt(10, 0, 0).expect("horario valido"),
        id_tecnico: None,
    };

    let erro = agendamento
        .reagendar_visita(cenario.coordenador, visita.id, reagendamento)
        .await
        .expect_err("visita cancelada nao pode ser reagendada");

    assert!(matches!(erro, ErroAgendamento::VisitaNaoPodeSerAlterada));
}

// =============================================================================
// Integridade do MER — mapeamento dos enums acentuados ('Em andamento',
// 'Não realizada') entre o PostgreSQL e o dominio Rust
// =============================================================================

#[tokio::test]
async fn enums_acentuados_do_mer_batem_com_o_mapeamento_do_dominio() {
    let pool = preparar_banco().await;
    let cenario = cenario_padrao(&pool, &sufixo_unico()).await;
    let repositorio = RepositorioVisitaPostgres::novo(pool.clone());

    let data = (Utc::now() + Duration::days(2)).date_naive();
    let horario = NaiveTime::from_hms_opt(9, 0, 0).expect("horario valido");

    for (status_banco, esperado) in [
        ("Em andamento", StatusVisita::EmAndamento),
        ("Não realizada", StatusVisita::NaoRealizada),
    ] {
        let id: uuid::Uuid = sqlx::query_scalar(
            "INSERT INTO visita
                (id_familia, id_tecnico, id_agendado_por, data_visita, horario, objetivo, status)
             VALUES ($1, $2, $3, $4, $5, 'Visita de teste de integridade', $6::status_visita)
             RETURNING id_visita",
        )
        .bind(cenario.familia)
        .bind(cenario.tecnico)
        .bind(cenario.coordenador)
        .bind(data)
        .bind(horario)
        .bind(status_banco)
        .fetch_one(&pool)
        .await
        .expect("insert direto no banco");

        let visita = repositorio
            .buscar_por_id(id)
            .await
            .expect("consulta da visita")
            .expect("visita deve existir");

        assert_eq!(
            visita.status, esperado,
            "status '{status_banco}' deve mapear para o dominio"
        );
        assert_eq!(visita.status.como_str(), status_banco);
    }
}

// =============================================================================
// Fluxo HTTP do UC04 (endpoints /api/v1/visitas)
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
    usuario_id: uuid::Uuid,
    id_unidade: uuid::Uuid,
    email: &str,
) -> String {
    let segredo = std::env::var("JWT_SEGREDO")
        .unwrap_or_else(|_| "segredo_de_desenvolvimento_visita_plus".to_string());
    let servico = ServicoJwt::novo(segredo, 60);
    // Sessao ativa vinculada ao token (exigida pelo extrator, RF04)
    let id_sessao = uuid::Uuid::new_v4();
    criar_sessao_ativa(pool, id_sessao, usuario_id).await;
    servico
        .emitir(
            &Usuario {
                id: usuario_id,
                id_unidade,
                nome: "Usuario Teste".into(),
                email: email.into(),
                senha_hash: "nao_utilizado".into(),
                perfil: PerfilUsuario::Tecnico,
                status: StatusUsuario::Ativo,
                exige_troca_senha: false,
                ultimo_acesso_em: None,
            },
            id_sessao,
        )
        .expect("deve emitir token")
}

#[tokio::test]
async fn endpoint_agenda_visita_responde_201() {
    let pool = preparar_banco().await;
    let cenario = cenario_padrao(&pool, &sufixo_unico()).await;
    let roteador = montar_roteador(pool.clone());
    let unidade =
        sqlx::query_scalar::<_, uuid::Uuid>("SELECT id_unidade FROM familia WHERE id_familia = $1")
            .bind(cenario.familia)
            .fetch_one(&pool)
            .await
            .expect("unidade da familia");
    let token = emitir_token(&pool, cenario.coordenador, unidade, "coord@teste.com").await;

    let data = (Utc::now() + Duration::days(2)).date_naive();
    let corpo = serde_json::json!({
        "id_familia": cenario.familia,
        "id_tecnico": cenario.tecnico,
        "data_visita": data.to_string(),
        "horario": "09:30:00",
        "prioridade": "Alta",
        "objetivo": "Acompanhamento de rotina"
    });

    let resposta = roteador
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/visitas")
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
async fn endpoint_conflito_de_horario_responde_409() {
    let pool = preparar_banco().await;
    let cenario = cenario_padrao(&pool, &sufixo_unico()).await;
    let agendamento = montar_agendamento(&pool);
    agendamento
        .agendar_visita(
            cenario.coordenador,
            cadastro_visita(cenario.familia, cenario.tecnico, 3, 14),
        )
        .await
        .expect("primeira visita");

    let roteador = montar_roteador(pool.clone());
    let unidade =
        sqlx::query_scalar::<_, uuid::Uuid>("SELECT id_unidade FROM familia WHERE id_familia = $1")
            .bind(cenario.familia)
            .fetch_one(&pool)
            .await
            .expect("unidade da familia");
    let token = emitir_token(&pool, cenario.coordenador, unidade, "coord@teste.com").await;

    let data = (Utc::now() + Duration::days(3)).date_naive();
    let corpo = serde_json::json!({
        "id_familia": cenario.familia,
        "id_tecnico": cenario.tecnico,
        "data_visita": data.to_string(),
        "horario": "14:00:00",
        "prioridade": "Normal",
        "objetivo": "Nova visita em horario ocupado"
    });

    let resposta = roteador
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/visitas")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::from(corpo.to_string()))
                .expect("requisicao valida"),
        )
        .await
        .expect("falha ao executar a requisicao");

    assert_eq!(resposta.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn endpoint_sem_token_responde_401() {
    let pool = preparar_banco().await;
    let roteador = montar_roteador(pool);

    let corpo = serde_json::json!({
        "id_familia": uuid::Uuid::new_v4(),
        "id_tecnico": uuid::Uuid::new_v4(),
        "data_visita": (Utc::now() + Duration::days(2)).date_naive().to_string(),
        "horario": "09:00:00",
        "prioridade": "Normal",
        "objetivo": "Tentativa sem autenticacao"
    });

    let resposta = roteador
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/visitas")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(corpo.to_string()))
                .expect("requisicao valida"),
        )
        .await
        .expect("falha ao executar a requisicao");

    assert_eq!(resposta.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn coordenador_de_outra_unidade_nao_gerencia_visita_alheia() {
    let pool = preparar_banco().await;
    let cenario = cenario_padrao(&pool, &sufixo_unico()).await;
    let coordenador_alheio = criar_usuario(
        &pool,
        PerfilUsuario::Coordenador,
        StatusUsuario::Ativo,
        false,
    )
    .await;
    let agendamento = montar_agendamento(&pool);

    let visita = agendamento
        .agendar_visita(
            cenario.coordenador,
            cadastro_visita(cenario.familia, cenario.tecnico, 2, 9),
        )
        .await
        .expect("visita agendada");

    // Detalhar: mesmo erro de "nao encontrado", para nao revelar a agenda.
    let erro = agendamento
        .detalhar_visita(coordenador_alheio.id, visita.id)
        .await
        .expect_err("coordenador alheio nao deveria detalhar visita de outra unidade");
    assert!(matches!(erro, ErroAgendamento::NaoEncontrado));

    // Cancelar: sem permissao.
    let erro = agendamento
        .cancelar_visita(
            coordenador_alheio.id,
            visita.id,
            "Tentativa de cancelamento",
        )
        .await
        .expect_err("coordenador alheio nao deveria cancelar visita de outra unidade");
    assert!(matches!(erro, ErroAgendamento::SemPermissao));

    // Reagendar: sem permissao.
    let erro = agendamento
        .reagendar_visita(
            coordenador_alheio.id,
            visita.id,
            ReagendamentoVisita {
                data_visita: Utc::now().date_naive() + Duration::days(5),
                horario: NaiveTime::from_hms_opt(15, 0, 0).expect("horario valido"),
                id_tecnico: None,
            },
        )
        .await
        .expect_err("coordenador alheio nao deveria reagendar visita de outra unidade");
    assert!(matches!(erro, ErroAgendamento::SemPermissao));

    // Coordenador da propria unidade continua gerenciando normalmente.
    let detalhe = agendamento
        .detalhar_visita(cenario.coordenador, visita.id)
        .await
        .expect("coordenador da unidade deveria detalhar a visita");
    assert_eq!(detalhe.visita.id, visita.id);
}
