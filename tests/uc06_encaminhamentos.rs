// =============================================================================
// Testes de integração — UC06/RF17 (Encaminhamentos da visita)
// -----------------------------------------------------------------------------
// Exige o PostgreSQL do docker-compose em execucao:
//     docker compose up -d postgres
//     DATABASE_URL=postgres://... cargo test --test uc06_encaminhamentos
// =============================================================================

mod comum;

use std::sync::Arc;

use chrono::{Duration, NaiveTime, Utc};
use comum::*;
use visita_plus::casos_de_uso::agendamento::CasoDeUsoAgendamento;
use visita_plus::casos_de_uso::encaminhamentos::CasoDeUsoEncaminhamentos;
use visita_plus::casos_de_uso::registro::CasoDeUsoRegistro;
use visita_plus::dominio::entidades::visita::CadastroVisita;
use visita_plus::dominio::enums::{
    PerfilUsuario, PrioridadeVisita, StatusEncaminhamento, StatusUsuario,
};
use visita_plus::dominio::erros::ErroEncaminhamento;
use visita_plus::infraestrutura::auditoria::{ServicoAuditoria, acao};
use visita_plus::infraestrutura::repositorios::encaminhamento_repositorio::RepositorioEncaminhamentoPostgres;
use visita_plus::infraestrutura::repositorios::familia_repositorio::RepositorioFamiliaPostgres;
use visita_plus::infraestrutura::repositorios::registro_repositorio::RepositorioRegistroPostgres;
use visita_plus::infraestrutura::repositorios::usuario_repositorio::RepositorioUsuarioPostgres;
use visita_plus::infraestrutura::repositorios::visita_repositorio::RepositorioVisitaPostgres;

// =============================================================================
// Helpers de montagem
// =============================================================================

struct SuitesEncaminhamentos {
    agendamento: CasoDeUsoAgendamento,
    registro: CasoDeUsoRegistro,
    encaminhamentos: CasoDeUsoEncaminhamentos,
}

fn montar(pool: &sqlx::PgPool) -> SuitesEncaminhamentos {
    let repositorio_usuario = Arc::new(RepositorioUsuarioPostgres::novo(pool.clone()));
    let repositorio_familia = Arc::new(RepositorioFamiliaPostgres::novo(pool.clone()));
    let repositorio_visita = Arc::new(RepositorioVisitaPostgres::novo(pool.clone()));
    let auditoria = ServicoAuditoria::novo(pool.clone());

    SuitesEncaminhamentos {
        agendamento: CasoDeUsoAgendamento::novo(
            repositorio_visita.clone(),
            repositorio_familia.clone(),
            repositorio_usuario.clone(),
            auditoria.clone(),
        ),
        registro: CasoDeUsoRegistro::novo(
            Arc::new(RepositorioRegistroPostgres::novo(pool.clone())),
            repositorio_visita.clone(),
            repositorio_familia.clone(),
            repositorio_usuario.clone(),
            auditoria.clone(),
        ),
        encaminhamentos: CasoDeUsoEncaminhamentos::novo(
            Arc::new(RepositorioEncaminhamentoPostgres::novo(pool.clone())),
            repositorio_visita,
            repositorio_familia,
            repositorio_usuario,
            auditoria,
        ),
    }
}

/// Coordenador + tecnico da mesma unidade + familia + visita em andamento.
struct Cenario {
    coordenador: UsuarioDeTeste,
    tecnico: UsuarioDeTeste,
    visita: uuid::Uuid,
}

async fn cenario_com_visita_em_andamento(pool: &sqlx::PgPool) -> Cenario {
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
    let familia = criar_familia(
        pool,
        coordenador.id_unidade,
        &format!("FAM-{}", sufixo_unico()),
    )
    .await;

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

    suites
        .registro
        .iniciar_visita(tecnico.id, visita.id)
        .await
        .expect("visita em andamento");

    Cenario {
        coordenador,
        tecnico,
        visita: visita.id,
    }
}

// =============================================================================
// Registro de encaminhamentos (RF17)
// =============================================================================

#[tokio::test]
async fn tecnico_registra_encaminhamento_durante_o_atendimento() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_em_andamento(&pool).await;
    let suites = montar(&pool);

    let criado = suites
        .encaminhamentos
        .registrar_encaminhamento(
            cenario.tecnico.id,
            cenario.visita,
            "UBS Central",
            "Encaminhar crianca para avaliacao medica.",
        )
        .await
        .expect("encaminhamento registrado");

    assert_eq!(criado.id_visita, cenario.visita);
    assert_eq!(criado.destino, "UBS Central");
    assert_eq!(criado.registrado_por, cenario.tecnico.id);
    assert_eq!(criado.status, StatusEncaminhamento::Aberto);

    assert_eq!(
        contar_auditoria(&pool, cenario.tecnico.id, acao::ENCAMINHAMENTO_REGISTRADO).await,
        1
    );
}

#[tokio::test]
async fn permite_multiplos_encaminhamentos_para_a_mesma_visita() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_em_andamento(&pool).await;
    let suites = montar(&pool);

    suites
        .encaminhamentos
        .registrar_encaminhamento(
            cenario.tecnico.id,
            cenario.visita,
            "UBS Central",
            "Avaliacao medica.",
        )
        .await
        .expect("primeiro encaminhamento");

    suites
        .encaminhamentos
        .registrar_encaminhamento(
            cenario.tecnico.id,
            cenario.visita,
            "Conselho Tutelar",
            "Acompanhamento de vulnerabilidade.",
        )
        .await
        .expect("segundo encaminhamento");

    let lista = suites
        .encaminhamentos
        .listar_encaminhamentos(cenario.tecnico.id, cenario.visita)
        .await
        .expect("lista de encaminhamentos");

    assert_eq!(lista.len(), 2, "RF17: um ou mais por visita");
    assert!(
        lista
            .iter()
            .all(|e| e.status == StatusEncaminhamento::Aberto)
    );
}

#[tokio::test]
async fn destino_e_motivo_sao_obrigatorios() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_em_andamento(&pool).await;
    let suites = montar(&pool);

    let erro = suites
        .encaminhamentos
        .registrar_encaminhamento(cenario.tecnico.id, cenario.visita, "   ", "Motivo")
        .await
        .expect_err("destino vazio deve falhar");
    assert!(matches!(erro, ErroEncaminhamento::DestinoObrigatorio));

    let erro = suites
        .encaminhamentos
        .registrar_encaminhamento(cenario.tecnico.id, cenario.visita, "UBS", "  ")
        .await
        .expect_err("motivo vazio deve falhar");
    assert!(matches!(erro, ErroEncaminhamento::MotivoObrigatorio));
}

#[tokio::test]
async fn visita_nao_iniciada_nao_admite_encaminhamento() {
    let pool = preparar_banco().await;
    let coordenador = criar_usuario(
        &pool,
        PerfilUsuario::Coordenador,
        StatusUsuario::Ativo,
        false,
    )
    .await;
    let tecnico = criar_usuario_na_unidade(
        &pool,
        coordenador.id_unidade,
        PerfilUsuario::Tecnico,
        StatusUsuario::Ativo,
        false,
    )
    .await;
    let familia = criar_familia(
        &pool,
        coordenador.id_unidade,
        &format!("FAM-{}", sufixo_unico()),
    )
    .await;

    let suites = montar(&pool);
    let visita = suites
        .agendamento
        .agendar_visita(
            coordenador.id,
            CadastroVisita {
                id_familia: familia,
                id_tecnico: tecnico.id,
                data_visita: Utc::now().date_naive() + Duration::days(3),
                horario: NaiveTime::from_hms_opt(10, 0, 0).expect("horario valido"),
                prioridade: PrioridadeVisita::Normal,
                objetivo: "Visita agendada".into(),
            },
        )
        .await
        .expect("visita agendada");

    let erro = suites
        .encaminhamentos
        .registrar_encaminhamento(tecnico.id, visita.id, "UBS", "Motivo")
        .await
        .expect_err("visita agendada nao admite encaminhamento");

    assert!(matches!(erro, ErroEncaminhamento::VisitaInvalida));
}

#[tokio::test]
async fn coordenador_nao_registra_encaminhamento() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_em_andamento(&pool).await;
    let suites = montar(&pool);

    let erro = suites
        .encaminhamentos
        .registrar_encaminhamento(cenario.coordenador.id, cenario.visita, "CREAS", "Motivo")
        .await
        .expect_err("coordenador nao registra encaminhamento");

    assert!(matches!(erro, ErroEncaminhamento::SemPermissao));
}

#[tokio::test]
async fn tecnico_de_outra_agenda_nao_registra_encaminhamento() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_em_andamento(&pool).await;
    let outro_tecnico = criar_usuario_na_unidade(
        &pool,
        cenario.coordenador.id_unidade,
        PerfilUsuario::Tecnico,
        StatusUsuario::Ativo,
        false,
    )
    .await;
    let suites = montar(&pool);

    let erro = suites
        .encaminhamentos
        .registrar_encaminhamento(outro_tecnico.id, cenario.visita, "UBS", "Motivo")
        .await
        .expect_err("tecnico alheio nao registra encaminhamento");

    assert!(matches!(erro, ErroEncaminhamento::NaoEncontrado));
}

// =============================================================================
// Listagem por perfil (RN05/RN06)
// =============================================================================

#[tokio::test]
async fn listagem_respeita_a_visibilidade_do_perfil() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_em_andamento(&pool).await;
    let outro_tecnico = criar_usuario_na_unidade(
        &pool,
        cenario.coordenador.id_unidade,
        PerfilUsuario::Tecnico,
        StatusUsuario::Ativo,
        false,
    )
    .await;
    let coordenador_outra_unidade = criar_usuario(
        &pool,
        PerfilUsuario::Coordenador,
        StatusUsuario::Ativo,
        false,
    )
    .await;
    let suites = montar(&pool);

    suites
        .encaminhamentos
        .registrar_encaminhamento(cenario.tecnico.id, cenario.visita, "UBS Central", "Motivo")
        .await
        .expect("encaminhamento registrado");

    // Tecnico responsavel enxerga.
    let do_tecnico = suites
        .encaminhamentos
        .listar_encaminhamentos(cenario.tecnico.id, cenario.visita)
        .await
        .expect("lista do tecnico");
    assert_eq!(do_tecnico.len(), 1);

    // Coordenador da mesma unidade enxerga.
    let do_coordenador = suites
        .encaminhamentos
        .listar_encaminhamentos(cenario.coordenador.id, cenario.visita)
        .await
        .expect("lista do coordenador");
    assert_eq!(do_coordenador.len(), 1);

    // Tecnico de outra agenda nao enxerga.
    let erro = suites
        .encaminhamentos
        .listar_encaminhamentos(outro_tecnico.id, cenario.visita)
        .await
        .expect_err("tecnico alheio nao lista");
    assert!(matches!(erro, ErroEncaminhamento::NaoEncontrado));

    // Coordenador de outra unidade nao enxerga.
    let erro = suites
        .encaminhamentos
        .listar_encaminhamentos(coordenador_outra_unidade.id, cenario.visita)
        .await
        .expect_err("coordenador alheio nao lista");
    assert!(matches!(erro, ErroEncaminhamento::NaoEncontrado));
}

// =============================================================================
// Conclusao
// =============================================================================

#[tokio::test]
async fn tecnico_conclui_o_proprio_encaminhamento() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_em_andamento(&pool).await;
    let suites = montar(&pool);

    let criado = suites
        .encaminhamentos
        .registrar_encaminhamento(cenario.tecnico.id, cenario.visita, "UBS Central", "Motivo")
        .await
        .expect("encaminhamento registrado");

    let concluido = suites
        .encaminhamentos
        .concluir_encaminhamento(cenario.tecnico.id, criado.id)
        .await
        .expect("encaminhamento concluido");

    assert_eq!(concluido.status, StatusEncaminhamento::Concluido);

    assert_eq!(
        contar_auditoria(&pool, cenario.tecnico.id, acao::ENCAMINHAMENTO_CONCLUIDO).await,
        1
    );
}

#[tokio::test]
async fn coordenador_da_unidade_conclui_o_encaminhamento() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_em_andamento(&pool).await;
    let suites = montar(&pool);

    let criado = suites
        .encaminhamentos
        .registrar_encaminhamento(cenario.tecnico.id, cenario.visita, "CREAS", "Motivo")
        .await
        .expect("encaminhamento registrado");

    let concluido = suites
        .encaminhamentos
        .concluir_encaminhamento(cenario.coordenador.id, criado.id)
        .await
        .expect("coordenador conclui o encaminhamento");

    assert_eq!(concluido.status, StatusEncaminhamento::Concluido);
}

#[tokio::test]
async fn encaminhamento_ja_concluido_nao_pode_ser_concluido_novamente() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_em_andamento(&pool).await;
    let suites = montar(&pool);

    let criado = suites
        .encaminhamentos
        .registrar_encaminhamento(cenario.tecnico.id, cenario.visita, "UBS Central", "Motivo")
        .await
        .expect("encaminhamento registrado");

    suites
        .encaminhamentos
        .concluir_encaminhamento(cenario.tecnico.id, criado.id)
        .await
        .expect("primeira conclusao");

    let erro = suites
        .encaminhamentos
        .concluir_encaminhamento(cenario.tecnico.id, criado.id)
        .await
        .expect_err("segunda conclusao deve falhar");

    assert!(matches!(erro, ErroEncaminhamento::EstadoInvalido));
}

#[tokio::test]
async fn tecnico_alheio_nao_conclui_o_encaminhamento() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_em_andamento(&pool).await;
    let outro_tecnico = criar_usuario_na_unidade(
        &pool,
        cenario.coordenador.id_unidade,
        PerfilUsuario::Tecnico,
        StatusUsuario::Ativo,
        false,
    )
    .await;
    let suites = montar(&pool);

    let criado = suites
        .encaminhamentos
        .registrar_encaminhamento(cenario.tecnico.id, cenario.visita, "UBS Central", "Motivo")
        .await
        .expect("encaminhamento registrado");

    let erro = suites
        .encaminhamentos
        .concluir_encaminhamento(outro_tecnico.id, criado.id)
        .await
        .expect_err("tecnico alheio nao conclui");

    assert!(matches!(erro, ErroEncaminhamento::NaoEncontrado));
}
