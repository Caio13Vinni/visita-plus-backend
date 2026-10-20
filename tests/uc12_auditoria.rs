// =============================================================================
// Testes de integração — UC12 (Consultar auditoria)
// -----------------------------------------------------------------------------
// Exige o PostgreSQL do docker-compose em execucao:
//     docker compose up -d postgres
//     DATABASE_URL=postgres://... cargo test --test uc12_auditoria
// =============================================================================

mod comum;

use std::sync::Arc;

use chrono::{Duration, NaiveTime, Utc};
use comum::*;
use visita_plus::casos_de_uso::agendamento::CasoDeUsoAgendamento;
use visita_plus::casos_de_uso::auditoria::CasoDeUsoAuditoria;
use visita_plus::dominio::entidades::auditoria::FiltroAuditoria;
use visita_plus::dominio::entidades::visita::CadastroVisita;
use visita_plus::dominio::enums::{PerfilUsuario, PrioridadeVisita, StatusUsuario};
use visita_plus::dominio::erros::ErroAuditoria;
use visita_plus::infraestrutura::auditoria::{ServicoAuditoria, acao};
use visita_plus::infraestrutura::repositorios::auditoria_repositorio::RepositorioAuditoriaPostgres;
use visita_plus::infraestrutura::repositorios::familia_repositorio::RepositorioFamiliaPostgres;
use visita_plus::infraestrutura::repositorios::usuario_repositorio::RepositorioUsuarioPostgres;
use visita_plus::infraestrutura::repositorios::visita_repositorio::RepositorioVisitaPostgres;

// =============================================================================
// Helpers de montagem
// =============================================================================

struct SuitesAuditoria {
    agendamento: CasoDeUsoAgendamento,
    auditoria: CasoDeUsoAuditoria,
}

fn montar(pool: &sqlx::PgPool) -> SuitesAuditoria {
    let repositorio_usuario = Arc::new(RepositorioUsuarioPostgres::novo(pool.clone()));
    let repositorio_familia = Arc::new(RepositorioFamiliaPostgres::novo(pool.clone()));
    let repositorio_visita = Arc::new(RepositorioVisitaPostgres::novo(pool.clone()));
    let auditoria = ServicoAuditoria::novo(pool.clone());

    SuitesAuditoria {
        agendamento: CasoDeUsoAgendamento::novo(
            repositorio_visita,
            repositorio_familia,
            repositorio_usuario.clone(),
            auditoria.clone(),
        ),
        auditoria: CasoDeUsoAuditoria::novo(
            Arc::new(RepositorioAuditoriaPostgres::novo(pool.clone())),
            repositorio_usuario,
            auditoria,
        ),
    }
}

/// Coordenador + tecnico da mesma unidade + familia.
struct Cenario {
    coordenador: UsuarioDeTeste,
    tecnico: UsuarioDeTeste,
    familia: uuid::Uuid,
}

async fn cenario_padrao(pool: &sqlx::PgPool) -> Cenario {
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

    Cenario {
        coordenador,
        tecnico,
        familia,
    }
}

async fn agendar_visita(
    suites: &SuitesAuditoria,
    cenario: &Cenario,
    dias: i64,
    hora: u32,
    objetivo: &str,
) -> uuid::Uuid {
    suites
        .agendamento
        .agendar_visita(
            cenario.coordenador.id,
            CadastroVisita {
                id_familia: cenario.familia,
                id_tecnico: cenario.tecnico.id,
                data_visita: Utc::now().date_naive() + Duration::days(dias),
                horario: NaiveTime::from_hms_opt(hora, 0, 0).expect("horario valido"),
                prioridade: PrioridadeVisita::Normal,
                objetivo: objetivo.into(),
            },
        )
        .await
        .expect("visita agendada")
        .id
}

// =============================================================================
// Consulta a trilha
// =============================================================================

#[tokio::test]
async fn coordenador_lista_acoes_da_mais_recente_para_a_mais_antiga() {
    let pool = preparar_banco().await;
    let cenario = cenario_padrao(&pool).await;
    let suites = montar(&pool);

    agendar_visita(&suites, &cenario, 3, 8, "Primeira visita").await;
    agendar_visita(&suites, &cenario, 4, 9, "Segunda visita").await;
    let terceira = agendar_visita(&suites, &cenario, 5, 10, "Terceira visita").await;
    suites
        .agendamento
        .cancelar_visita(cenario.coordenador.id, terceira, "Motivo do cancelamento")
        .await
        .expect("visita cancelada");

    let eventos = suites
        .auditoria
        .consultar_auditoria(
            cenario.coordenador.id,
            FiltroAuditoria {
                id_usuario: Some(cenario.coordenador.id),
                ..Default::default()
            },
        )
        .await
        .expect("trilha de auditoria");

    assert_eq!(eventos.len(), 4, "tres agendamentos + um cancelamento");

    let acoes: Vec<&str> = eventos.iter().map(|e| e.acao.as_str()).collect();
    assert_eq!(
        acoes
            .iter()
            .filter(|a| **a == acao::VISITA_AGENDADA)
            .count(),
        3
    );
    assert!(acoes.contains(&acao::VISITA_CANCELADA));

    // Ordem da mais recente para a mais antiga.
    for janela in eventos.windows(2) {
        assert!(
            janela[0].data_hora >= janela[1].data_hora,
            "trilha deve vir em ordem descendente"
        );
    }

    // O evento mais recente e o cancelamento, com autor e descricao.
    let cancelamento = eventos
        .iter()
        .find(|e| e.acao == acao::VISITA_CANCELADA)
        .expect("evento de cancelamento");
    assert_eq!(cancelamento.id_usuario, Some(cenario.coordenador.id));
    assert!(
        cancelamento
            .descricao
            .as_deref()
            .expect("descricao do evento")
            .contains("Motivo do cancelamento")
    );
}

#[tokio::test]
async fn filtro_por_acao() {
    let pool = preparar_banco().await;
    let cenario = cenario_padrao(&pool).await;
    let suites = montar(&pool);

    agendar_visita(&suites, &cenario, 3, 8, "Visita a manter").await;
    let cancelada = agendar_visita(&suites, &cenario, 4, 9, "Visita a cancelar").await;
    suites
        .agendamento
        .cancelar_visita(cenario.coordenador.id, cancelada, "Motivo")
        .await
        .expect("visita cancelada");

    let cancelamentos = suites
        .auditoria
        .consultar_auditoria(
            cenario.coordenador.id,
            FiltroAuditoria {
                id_usuario: Some(cenario.coordenador.id),
                acao: Some(acao::VISITA_CANCELADA.to_string()),
                ..Default::default()
            },
        )
        .await
        .expect("filtro por acao");

    assert_eq!(cancelamentos.len(), 1);
    assert!(
        cancelamentos
            .iter()
            .all(|e| e.acao == acao::VISITA_CANCELADA)
    );
}

#[tokio::test]
async fn filtro_por_usuario() {
    let pool = preparar_banco().await;
    let cenario = cenario_padrao(&pool).await;
    let suites = montar(&pool);

    agendar_visita(&suites, &cenario, 3, 8, "Visita do coordenador").await;

    let do_coordenador = suites
        .auditoria
        .consultar_auditoria(
            cenario.coordenador.id,
            FiltroAuditoria {
                id_usuario: Some(cenario.coordenador.id),
                ..Default::default()
            },
        )
        .await
        .expect("filtro por usuario");

    assert!(!do_coordenador.is_empty());
    assert!(
        do_coordenador
            .iter()
            .all(|e| e.id_usuario == Some(cenario.coordenador.id))
    );

    let do_tecnico = suites
        .auditoria
        .consultar_auditoria(
            cenario.coordenador.id,
            FiltroAuditoria {
                id_usuario: Some(cenario.tecnico.id),
                ..Default::default()
            },
        )
        .await
        .expect("filtro por tecnico");

    assert!(do_tecnico.is_empty(), "tecnico nao gerou eventos no fluxo");
}

#[tokio::test]
async fn filtro_por_item() {
    let pool = preparar_banco().await;
    let cenario = cenario_padrao(&pool).await;
    let suites = montar(&pool);

    let alvo = agendar_visita(&suites, &cenario, 3, 8, "Visita alvo").await;
    agendar_visita(&suites, &cenario, 4, 9, "Outra visita").await;

    let do_item = suites
        .auditoria
        .consultar_auditoria(
            cenario.coordenador.id,
            FiltroAuditoria {
                id_usuario: Some(cenario.coordenador.id),
                entidade: Some("visita".to_string()),
                entidade_id: Some(alvo),
                ..Default::default()
            },
        )
        .await
        .expect("filtro por item");

    assert_eq!(do_item.len(), 1, "somente o agendamento da visita alvo");
    assert_eq!(do_item[0].entidade, "visita");
    assert_eq!(do_item[0].entidade_id, alvo);
    assert_eq!(do_item[0].acao, acao::VISITA_AGENDADA);
}

#[tokio::test]
async fn filtro_por_periodo() {
    let pool = preparar_banco().await;
    let cenario = cenario_padrao(&pool).await;
    let suites = montar(&pool);

    agendar_visita(&suites, &cenario, 3, 8, "Visita no periodo").await;

    let agora = Utc::now();

    let futuro = suites
        .auditoria
        .consultar_auditoria(
            cenario.coordenador.id,
            FiltroAuditoria {
                id_usuario: Some(cenario.coordenador.id),
                data_inicio: Some(agora + Duration::hours(1)),
                data_fim: Some(agora + Duration::hours(2)),
                ..Default::default()
            },
        )
        .await
        .expect("filtro por periodo futuro");
    assert!(futuro.is_empty());

    let presente = suites
        .auditoria
        .consultar_auditoria(
            cenario.coordenador.id,
            FiltroAuditoria {
                id_usuario: Some(cenario.coordenador.id),
                data_inicio: Some(agora - Duration::minutes(5)),
                data_fim: Some(agora + Duration::minutes(5)),
                ..Default::default()
            },
        )
        .await
        .expect("filtro pelo periodo atual");
    assert!(!presente.is_empty(), "eventos recentes dentro da janela");
}

#[tokio::test]
async fn tecnico_e_negado_e_tentativa_fica_registrada() {
    let pool = preparar_banco().await;
    let cenario = cenario_padrao(&pool).await;
    let suites = montar(&pool);

    let erro = suites
        .auditoria
        .consultar_auditoria(cenario.tecnico.id, FiltroAuditoria::default())
        .await
        .expect_err("tecnico nao consulta a auditoria");

    assert!(matches!(erro, ErroAuditoria::SemPermissao));

    // A tentativa negada fica registrada na propria trilha (RN22).
    let eventos = suites
        .auditoria
        .consultar_auditoria(
            cenario.coordenador.id,
            FiltroAuditoria {
                id_usuario: Some(cenario.tecnico.id),
                acao: Some(acao::AUDITORIA_ACESSO_NEGADO.to_string()),
                ..Default::default()
            },
        )
        .await
        .expect("trilha com a tentativa negada");

    assert_eq!(eventos.len(), 1);
    assert_eq!(eventos[0].acao, acao::AUDITORIA_ACESSO_NEGADO);
    assert_eq!(eventos[0].id_usuario, Some(cenario.tecnico.id));
}

#[tokio::test]
async fn periodo_invalido_e_rejeitado() {
    let pool = preparar_banco().await;
    let cenario = cenario_padrao(&pool).await;
    let suites = montar(&pool);

    let agora = Utc::now();
    let erro = suites
        .auditoria
        .consultar_auditoria(
            cenario.coordenador.id,
            FiltroAuditoria {
                data_inicio: Some(agora),
                data_fim: Some(agora - Duration::hours(1)),
                ..Default::default()
            },
        )
        .await
        .expect_err("periodo invertido deve falhar");

    assert!(matches!(erro, ErroAuditoria::DadosInvalidos(_)));
}

#[tokio::test]
async fn consulta_e_somente_leitura() {
    let pool = preparar_banco().await;
    let cenario = cenario_padrao(&pool).await;
    let suites = montar(&pool);

    agendar_visita(&suites, &cenario, 3, 8, "Visita para trilha").await;

    let antes = suites
        .auditoria
        .consultar_auditoria(
            cenario.coordenador.id,
            FiltroAuditoria {
                id_usuario: Some(cenario.coordenador.id),
                ..Default::default()
            },
        )
        .await
        .expect("trilha antes")
        .len();

    let _ = suites
        .auditoria
        .consultar_auditoria(
            cenario.coordenador.id,
            FiltroAuditoria {
                id_usuario: Some(cenario.coordenador.id),
                ..Default::default()
            },
        )
        .await
        .expect("consulta de leitura");

    let depois = suites
        .auditoria
        .consultar_auditoria(
            cenario.coordenador.id,
            FiltroAuditoria {
                id_usuario: Some(cenario.coordenador.id),
                ..Default::default()
            },
        )
        .await
        .expect("trilha depois")
        .len();

    assert_eq!(antes, depois, "a consulta nao deve alterar a trilha");
}
