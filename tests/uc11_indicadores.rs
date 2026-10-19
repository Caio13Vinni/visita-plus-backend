// =============================================================================
// Testes de integração — UC11 (Consultar indicadores)
// -----------------------------------------------------------------------------
// Exige o PostgreSQL do docker-compose em execucao:
//     docker compose up -d postgres
//     DATABASE_URL=postgres://... cargo test --test uc11_indicadores
// =============================================================================

mod comum;

use std::sync::Arc;

use chrono::{Duration, NaiveTime, Utc};
use comum::*;
use visita_plus::casos_de_uso::agendamento::CasoDeUsoAgendamento;
use visita_plus::casos_de_uso::indicadores::CasoDeUsoIndicadores;
use visita_plus::casos_de_uso::solicitacao_retorno::CasoDeUsoSolicitacaoRetorno;
use visita_plus::dominio::entidades::visita::{CadastroVisita, ReagendamentoVisita};
use visita_plus::dominio::enums::{PerfilUsuario, PrioridadeVisita, StatusUsuario, StatusVisita};
use visita_plus::dominio::erros::ErroIndicadores;
use visita_plus::infraestrutura::auditoria::ServicoAuditoria;
use visita_plus::infraestrutura::repositorios::familia_repositorio::RepositorioFamiliaPostgres;
use visita_plus::infraestrutura::repositorios::indicadores_repositorio::RepositorioIndicadoresPostgres;
use visita_plus::infraestrutura::repositorios::solicitacao_repositorio::RepositorioSolicitacaoPostgres;
use visita_plus::infraestrutura::repositorios::usuario_repositorio::RepositorioUsuarioPostgres;
use visita_plus::infraestrutura::repositorios::visita_repositorio::{
    RepositorioVisita, RepositorioVisitaPostgres,
};

// =============================================================================
// Helpers de montagem
// =============================================================================

struct SuitesIndicadores {
    agendamento: CasoDeUsoAgendamento,
    retorno: CasoDeUsoSolicitacaoRetorno,
    indicadores: CasoDeUsoIndicadores,
}

fn montar(pool: &sqlx::PgPool) -> SuitesIndicadores {
    let repositorio_usuario = Arc::new(RepositorioUsuarioPostgres::novo(pool.clone()));
    let repositorio_familia = Arc::new(RepositorioFamiliaPostgres::novo(pool.clone()));
    let repositorio_visita = Arc::new(RepositorioVisitaPostgres::novo(pool.clone()));
    let auditoria = ServicoAuditoria::novo(pool.clone());

    SuitesIndicadores {
        agendamento: CasoDeUsoAgendamento::novo(
            repositorio_visita.clone(),
            repositorio_familia.clone(),
            repositorio_usuario.clone(),
            auditoria.clone(),
        ),
        retorno: CasoDeUsoSolicitacaoRetorno::novo(
            Arc::new(RepositorioSolicitacaoPostgres::novo(pool.clone())),
            repositorio_visita.clone(),
            repositorio_familia,
            repositorio_usuario.clone(),
            auditoria,
        ),
        indicadores: CasoDeUsoIndicadores::novo(
            Arc::new(RepositorioIndicadoresPostgres::novo(pool.clone())),
            repositorio_usuario,
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

async fn agendar(
    suites: &SuitesIndicadores,
    cenario: &Cenario,
    dias: i64,
    horario_h: u32,
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
                horario: NaiveTime::from_hms_opt(horario_h, 0, 0).expect("horario valido"),
                prioridade: PrioridadeVisita::Normal,
                objetivo: objetivo.into(),
            },
        )
        .await
        .expect("visita agendada")
        .id
}

async fn marcar_status(pool: &sqlx::PgPool, id_visita: uuid::Uuid, status: StatusVisita) {
    RepositorioVisitaPostgres::novo(pool.clone())
        .atualizar_status(id_visita, status)
        .await
        .expect("status atualizado");
}

// =============================================================================
// Indicadores
// =============================================================================

#[tokio::test]
async fn coordenador_consulta_quantidades_por_status_e_retornos_pendentes() {
    let pool = preparar_banco().await;
    let cenario = cenario_padrao(&pool).await;
    let suites = montar(&pool);

    let realizada = agendar(&suites, &cenario, 2, 8, "Visita realizada").await;
    marcar_status(&pool, realizada, StatusVisita::Realizada).await;

    let nao_realizada = agendar(&suites, &cenario, 3, 9, "Visita nao realizada").await;
    marcar_status(&pool, nao_realizada, StatusVisita::NaoRealizada).await;

    let cancelada = agendar(&suites, &cenario, 4, 10, "Visita cancelada").await;
    suites
        .agendamento
        .cancelar_visita(cenario.coordenador.id, cancelada, "Motivo do cancelamento")
        .await
        .expect("visita cancelada");

    let em_andamento = agendar(&suites, &cenario, 5, 11, "Visita em andamento").await;
    marcar_status(&pool, em_andamento, StatusVisita::EmAndamento).await;

    // Duas agendadas permanecem.
    agendar(&suites, &cenario, 6, 14, "Visita agendada um").await;
    agendar(&suites, &cenario, 7, 15, "Visita agendada dois").await;

    // Pedido de retorno pendente.
    suites
        .retorno
        .solicitar_retorno(
            cenario.tecnico.id,
            realizada,
            "Motivo do retorno.",
            PrioridadeVisita::Normal,
        )
        .await
        .expect("pedido de retorno");

    let hoje = Utc::now().date_naive();
    let indicadores = suites
        .indicadores
        .consultar_indicadores(cenario.coordenador.id, hoje, hoje + Duration::days(30))
        .await
        .expect("indicadores");

    assert_eq!(indicadores.visitas_agendadas, 2);
    assert_eq!(indicadores.visitas_em_andamento, 1);
    assert_eq!(indicadores.visitas_realizadas, 1);
    assert_eq!(indicadores.visitas_nao_realizadas, 1);
    assert_eq!(indicadores.visitas_canceladas, 1);
    assert_eq!(indicadores.total_visitas(), 6);
    assert_eq!(indicadores.retornos_pendentes, 1);
    assert!(!indicadores.periodo_sem_dados());
}

#[tokio::test]
async fn tecnico_nao_consulta_indicadores() {
    let pool = preparar_banco().await;
    let cenario = cenario_padrao(&pool).await;
    let suites = montar(&pool);

    let hoje = Utc::now().date_naive();
    let erro = suites
        .indicadores
        .consultar_indicadores(cenario.tecnico.id, hoje, hoje + Duration::days(30))
        .await
        .expect_err("tecnico nao consulta indicadores");

    assert!(matches!(erro, ErroIndicadores::SemPermissao));
}

#[tokio::test]
async fn periodo_invalido_e_rejeitado() {
    let pool = preparar_banco().await;
    let cenario = cenario_padrao(&pool).await;
    let suites = montar(&pool);

    let hoje = Utc::now().date_naive();
    let erro = suites
        .indicadores
        .consultar_indicadores(cenario.coordenador.id, hoje, hoje - Duration::days(1))
        .await
        .expect_err("periodo invertido deve falhar");

    assert!(matches!(erro, ErroIndicadores::DadosInvalidos(_)));
}

#[tokio::test]
async fn periodo_sem_dados_retorna_zeros() {
    let pool = preparar_banco().await;
    let cenario = cenario_padrao(&pool).await;
    let suites = montar(&pool);

    agendar(&suites, &cenario, 2, 8, "Visita fora do periodo").await;

    // Periodo distante: sem dados, sem erro (fluxo do UC11).
    let hoje = Utc::now().date_naive();
    let indicadores = suites
        .indicadores
        .consultar_indicadores(
            cenario.coordenador.id,
            hoje + Duration::days(300),
            hoje + Duration::days(330),
        )
        .await
        .expect("periodo vazio nao e erro");

    assert_eq!(indicadores.total_visitas(), 0);
    assert_eq!(indicadores.retornos_pendentes, 0);
    assert!(indicadores.periodo_sem_dados());
}

#[tokio::test]
async fn coordenador_enxerga_apenas_a_propria_unidade() {
    let pool = preparar_banco().await;
    let cenario = cenario_padrao(&pool).await;
    let coordenador_outra_unidade = criar_usuario(
        &pool,
        PerfilUsuario::Coordenador,
        StatusUsuario::Ativo,
        false,
    )
    .await;
    let suites = montar(&pool);

    agendar(&suites, &cenario, 2, 8, "Visita da unidade A").await;

    let hoje = Utc::now().date_naive();
    let da_unidade_a = suites
        .indicadores
        .consultar_indicadores(cenario.coordenador.id, hoje, hoje + Duration::days(30))
        .await
        .expect("indicadores da unidade A");
    let da_unidade_b = suites
        .indicadores
        .consultar_indicadores(
            coordenador_outra_unidade.id,
            hoje,
            hoje + Duration::days(30),
        )
        .await
        .expect("indicadores da unidade B");

    assert_eq!(da_unidade_a.total_visitas(), 1);
    assert_eq!(
        da_unidade_b.total_visitas(),
        0,
        "unidade B nao ve dados da unidade A"
    );
    assert!(da_unidade_b.periodo_sem_dados());
}

#[tokio::test]
async fn reagendamento_para_forca_do_periodo_altera_a_contagem() {
    let pool = preparar_banco().await;
    let cenario = cenario_padrao(&pool).await;
    let suites = montar(&pool);

    let visita = agendar(&suites, &cenario, 2, 8, "Visita que saira do periodo").await;

    let hoje = Utc::now().date_naive();

    let antes = suites
        .indicadores
        .consultar_indicadores(cenario.coordenador.id, hoje, hoje + Duration::days(10))
        .await
        .expect("indicadores antes");
    assert_eq!(antes.visitas_agendadas, 1);

    // Reagenda para alem do periodo consultado.
    suites
        .agendamento
        .reagendar_visita(
            cenario.coordenador.id,
            visita,
            ReagendamentoVisita {
                data_visita: hoje + Duration::days(60),
                horario: NaiveTime::from_hms_opt(9, 0, 0).expect("horario valido"),
                id_tecnico: None,
            },
        )
        .await
        .expect("visita reagendada");

    let depois = suites
        .indicadores
        .consultar_indicadores(cenario.coordenador.id, hoje, hoje + Duration::days(10))
        .await
        .expect("indicadores depois");
    assert_eq!(depois.visitas_agendadas, 0, "visita saiu do periodo");
    assert!(depois.periodo_sem_dados());
}
