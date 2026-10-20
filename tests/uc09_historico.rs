// =============================================================================
// Testes de integração — UC09 (Consultar historico)
// -----------------------------------------------------------------------------
// Exige o PostgreSQL do docker-compose em execucao:
//     docker compose up -d postgres
//     DATABASE_URL=postgres://... cargo test --test uc09_historico
// =============================================================================

mod comum;

use std::sync::Arc;

use chrono::{Duration, NaiveTime, Utc};
use comum::*;
use visita_plus::casos_de_uso::agendamento::CasoDeUsoAgendamento;
use visita_plus::casos_de_uso::historico::CasoDeUsoHistorico;
use visita_plus::casos_de_uso::registro::CasoDeUsoRegistro;
use visita_plus::dominio::entidades::registro_visita::{DadosRegistro, NovoComplemento};
use visita_plus::dominio::entidades::visita::{CadastroVisita, ReagendamentoVisita};
use visita_plus::dominio::enums::{
    PerfilUsuario, PrioridadeVisita, ResultadoRegistro, StatusUsuario, StatusVisita,
};
use visita_plus::dominio::erros::ErroHistorico;
use visita_plus::infraestrutura::auditoria::{ServicoAuditoria, acao};
use visita_plus::infraestrutura::repositorios::familia_repositorio::RepositorioFamiliaPostgres;
use visita_plus::infraestrutura::repositorios::registro_repositorio::RepositorioRegistroPostgres;
use visita_plus::infraestrutura::repositorios::usuario_repositorio::RepositorioUsuarioPostgres;
use visita_plus::infraestrutura::repositorios::visita_repositorio::{
    FiltroVisita, RepositorioVisitaPostgres,
};

// =============================================================================
// Helpers de montagem
// =============================================================================

struct SuitesHistorico {
    agendamento: CasoDeUsoAgendamento,
    registro: CasoDeUsoRegistro,
    historico: CasoDeUsoHistorico,
}

fn montar(pool: &sqlx::PgPool) -> SuitesHistorico {
    let repositorio_usuario = Arc::new(RepositorioUsuarioPostgres::novo(pool.clone()));
    let repositorio_familia = Arc::new(RepositorioFamiliaPostgres::novo(pool.clone()));
    let repositorio_visita = Arc::new(RepositorioVisitaPostgres::novo(pool.clone()));
    let repositorio_registro = Arc::new(RepositorioRegistroPostgres::novo(pool.clone()));
    let auditoria = ServicoAuditoria::novo(pool.clone());

    SuitesHistorico {
        agendamento: CasoDeUsoAgendamento::novo(
            repositorio_visita.clone(),
            repositorio_familia.clone(),
            repositorio_usuario.clone(),
            auditoria.clone(),
        ),
        registro: CasoDeUsoRegistro::novo(
            repositorio_registro.clone(),
            repositorio_visita.clone(),
            repositorio_familia.clone(),
            repositorio_usuario.clone(),
            auditoria.clone(),
        ),
        historico: CasoDeUsoHistorico::novo(
            repositorio_visita,
            repositorio_familia,
            repositorio_registro,
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

async fn agendar(
    suites: &SuitesHistorico,
    id_agendado_por: uuid::Uuid,
    cadastro: CadastroVisita,
) -> uuid::Uuid {
    suites
        .agendamento
        .agendar_visita(id_agendado_por, cadastro)
        .await
        .expect("visita agendada")
        .id
}

fn cadastro_em(cenario: &Cenario, dias: i64, horario: NaiveTime) -> CadastroVisita {
    CadastroVisita {
        id_familia: cenario.familia,
        id_tecnico: cenario.tecnico.id,
        data_visita: Utc::now().date_naive() + Duration::days(dias),
        horario,
        prioridade: PrioridadeVisita::Normal,
        objetivo: "Acompanhamento familiar".into(),
    }
}

fn hora(h: u32, m: u32) -> NaiveTime {
    NaiveTime::from_hms_opt(h, m, 0).expect("horario valido")
}

async fn realizar_visita(
    suites: &SuitesHistorico,
    id_tecnico: uuid::Uuid,
    id_visita: uuid::Uuid,
) -> uuid::Uuid {
    let registro = suites
        .registro
        .iniciar_visita(id_tecnico, id_visita)
        .await
        .expect("registro iniciado");

    suites
        .registro
        .finalizar_registro(
            id_tecnico,
            registro.id,
            &DadosRegistro {
                resultado: Some(ResultadoRegistro::Realizada),
                descricao: Some("Familia recebida em casa.".into()),
                desfecho: Some("Acompanhamento mantido".into()),
                necessita_retorno: false,
                motivo_nao_realizada: None,
            },
        )
        .await
        .expect("registro finalizado");

    registro.id
}

// =============================================================================
// Listagem do historico
// =============================================================================

#[tokio::test]
async fn historico_lista_da_mais_recente_para_a_mais_antiga() {
    let pool = preparar_banco().await;
    let cenario = cenario_padrao(&pool).await;
    let suites = montar(&pool);

    let antiga = agendar(
        &suites,
        cenario.coordenador.id,
        cadastro_em(&cenario, 2, hora(8, 0)),
    )
    .await;
    let media = agendar(
        &suites,
        cenario.coordenador.id,
        cadastro_em(&cenario, 5, hora(9, 0)),
    )
    .await;
    let recente = agendar(
        &suites,
        cenario.coordenador.id,
        cadastro_em(&cenario, 9, hora(10, 0)),
    )
    .await;

    let historico = suites
        .historico
        .consultar_historico(cenario.tecnico.id, FiltroVisita::default())
        .await
        .expect("historico do tecnico");

    let ids: Vec<uuid::Uuid> = historico.iter().map(|v| v.id).collect();
    assert_eq!(
        ids,
        vec![recente, media, antiga],
        "ordem deve ser descendente"
    );
}

#[tokio::test]
async fn historico_filtra_por_periodo_status_e_familia() {
    let pool = preparar_banco().await;
    let cenario = cenario_padrao(&pool).await;
    let suites = montar(&pool);

    let dentro = agendar(
        &suites,
        cenario.coordenador.id,
        cadastro_em(&cenario, 3, hora(8, 0)),
    )
    .await;
    let fora = agendar(
        &suites,
        cenario.coordenador.id,
        cadastro_em(&cenario, 20, hora(8, 0)),
    )
    .await;
    let cancelada = agendar(
        &suites,
        cenario.coordenador.id,
        cadastro_em(&cenario, 4, hora(15, 0)),
    )
    .await;
    suites
        .agendamento
        .cancelar_visita(cenario.coordenador.id, cancelada, "Familia em viagem")
        .await
        .expect("visita cancelada");

    let hoje = Utc::now().date_naive();

    // Filtro por periodo: so a visita dentro da janela aparece.
    let por_periodo = suites
        .historico
        .consultar_historico(
            cenario.coordenador.id,
            FiltroVisita {
                data_inicio: Some(hoje),
                data_fim: Some(hoje + Duration::days(10)),
                ..Default::default()
            },
        )
        .await
        .expect("filtro por periodo");
    let ids: Vec<uuid::Uuid> = por_periodo.iter().map(|v| v.id).collect();
    assert!(ids.contains(&dentro));
    assert!(!ids.contains(&fora));

    // Filtro por status: so a cancelada aparece.
    let canceladas = suites
        .historico
        .consultar_historico(
            cenario.coordenador.id,
            FiltroVisita {
                status: Some(StatusVisita::Cancelada),
                ..Default::default()
            },
        )
        .await
        .expect("filtro por status");
    assert_eq!(canceladas.len(), 1);
    assert_eq!(canceladas[0].id, cancelada);
    assert_eq!(
        canceladas[0].motivo_cancelamento.as_deref(),
        Some("Familia em viagem")
    );

    // Filtro por familia.
    let outra_familia = criar_familia(
        &pool,
        cenario.coordenador.id_unidade,
        &format!("FAM-{}", sufixo_unico()),
    )
    .await;
    let _de_outra = agendar(
        &suites,
        cenario.coordenador.id,
        CadastroVisita {
            id_familia: outra_familia,
            ..cadastro_em(&cenario, 3, hora(11, 0))
        },
    )
    .await;

    let da_familia = suites
        .historico
        .consultar_historico(
            cenario.coordenador.id,
            FiltroVisita {
                id_familia: Some(cenario.familia),
                ..Default::default()
            },
        )
        .await
        .expect("filtro por familia");
    assert!(da_familia.iter().all(|v| v.id_familia == cenario.familia));
}

#[tokio::test]
async fn tecnico_somente_enxerga_a_propria_agenda_no_historico() {
    let pool = preparar_banco().await;
    let cenario = cenario_padrao(&pool).await;
    let outro_tecnico = criar_usuario_na_unidade(
        &pool,
        cenario.coordenador.id_unidade,
        PerfilUsuario::Tecnico,
        StatusUsuario::Ativo,
        false,
    )
    .await;
    let suites = montar(&pool);

    let do_tecnico = agendar(
        &suites,
        cenario.coordenador.id,
        cadastro_em(&cenario, 3, hora(8, 0)),
    )
    .await;
    let _do_outro = agendar(
        &suites,
        cenario.coordenador.id,
        CadastroVisita {
            id_tecnico: outro_tecnico.id,
            ..cadastro_em(&cenario, 4, hora(9, 0))
        },
    )
    .await;

    let historico = suites
        .historico
        .consultar_historico(cenario.tecnico.id, FiltroVisita::default())
        .await
        .expect("historico do tecnico");

    let ids: Vec<uuid::Uuid> = historico.iter().map(|v| v.id).collect();
    assert!(ids.contains(&do_tecnico));
    assert!(!ids.contains(&_do_outro));
}

#[tokio::test]
async fn coordenador_alheio_nao_enxerga_historico_da_unidade() {
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

    agendar(
        &suites,
        cenario.coordenador.id,
        cadastro_em(&cenario, 3, hora(8, 0)),
    )
    .await;

    // Coordenador da unidade enxerga.
    let do_coordenador = suites
        .historico
        .consultar_historico(cenario.coordenador.id, FiltroVisita::default())
        .await
        .expect("historico do coordenador");
    assert_eq!(do_coordenador.len(), 1);

    // Coordenador de outra unidade enxerga lista vazia (sem indicar erro).
    let de_outra = suites
        .historico
        .consultar_historico(coordenador_outra_unidade.id, FiltroVisita::default())
        .await
        .expect("historico de outra unidade");
    assert!(de_outra.is_empty());
}

#[tokio::test]
async fn historico_sem_itens_retorna_lista_vazia() {
    let pool = preparar_banco().await;
    let cenario = cenario_padrao(&pool).await;
    let suites = montar(&pool);

    let historico = suites
        .historico
        .consultar_historico(cenario.tecnico.id, FiltroVisita::default())
        .await
        .expect("historico vazio nao e erro");

    assert!(historico.is_empty());
}

// =============================================================================
// Detalhe do historico
// =============================================================================

#[tokio::test]
async fn detalhe_traz_registro_complementos_e_mudancas_com_autor_e_data() {
    let pool = preparar_banco().await;
    let cenario = cenario_padrao(&pool).await;
    let suites = montar(&pool);

    let visita = agendar(
        &suites,
        cenario.coordenador.id,
        cadastro_em(&cenario, 3, hora(9, 0)),
    )
    .await;

    // Reagendamento preservado no historico (RN18).
    suites
        .agendamento
        .reagendar_visita(
            cenario.coordenador.id,
            visita,
            ReagendamentoVisita {
                data_visita: Utc::now().date_naive() + Duration::days(4),
                horario: hora(10, 0),
                id_tecnico: None,
            },
        )
        .await
        .expect("visita reagendada");

    let id_registro = realizar_visita(&suites, cenario.tecnico.id, visita).await;

    suites
        .registro
        .adicionar_complemento(
            cenario.tecnico.id,
            id_registro,
            &NovoComplemento {
                descricao: "Familia informou novo numero de contato.".into(),
                motivo: "Correcao do contato registrado".into(),
            },
        )
        .await
        .expect("complemento registrado");

    let detalhe = suites
        .historico
        .detalhar_historico(cenario.tecnico.id, visita)
        .await
        .expect("detalhe do historico");

    assert_eq!(detalhe.visita.id, visita);

    let registro = detalhe.registro.expect("registro finalizado presente");
    assert_eq!(registro.resultado, Some(ResultadoRegistro::Realizada));

    // Complemento imutavel com autor e data (RN16/RN17).
    assert_eq!(detalhe.complementos.len(), 1);
    assert_eq!(detalhe.complementos[0].registrado_por, cenario.tecnico.id);
    assert!(detalhe.complementos[0].criado_em <= Utc::now());

    // Mudancas com autor e data: agendamento, reagendamento e registro.
    let acoes: Vec<&str> = detalhe.mudancas.iter().map(|e| e.acao.as_str()).collect();
    assert!(acoes.contains(&acao::VISITA_AGENDADA), "acoes: {acoes:?}");
    assert!(acoes.contains(&acao::VISITA_REAGENDADA), "acoes: {acoes:?}");
    assert!(
        acoes.contains(&acao::REGISTRO_FINALIZADO),
        "acoes: {acoes:?}"
    );
    assert!(
        detalhe
            .mudancas
            .iter()
            .all(|e| e.id_usuario.is_some() && e.data_hora <= Utc::now()),
        "toda mudanca deve ter autor e data"
    );

    // Ordem cronologica das mudancas.
    let recebidas: Vec<_> = detalhe.mudancas.iter().map(|e| e.data_hora).collect();
    let mut ordenadas = recebidas.clone();
    ordenadas.sort();
    assert_eq!(
        recebidas, ordenadas,
        "mudancas devem vir em ordem cronologica"
    );
}

#[tokio::test]
async fn cancelamento_aparece_nas_mudancas_do_historico() {
    let pool = preparar_banco().await;
    let cenario = cenario_padrao(&pool).await;
    let suites = montar(&pool);

    let visita = agendar(
        &suites,
        cenario.coordenador.id,
        cadastro_em(&cenario, 3, hora(9, 0)),
    )
    .await;
    suites
        .agendamento
        .cancelar_visita(
            cenario.coordenador.id,
            visita,
            "Tecnico afastado temporariamente",
        )
        .await
        .expect("visita cancelada");

    let detalhe = suites
        .historico
        .detalhar_historico(cenario.coordenador.id, visita)
        .await
        .expect("detalhe do historico");

    let cancelamento = detalhe
        .mudancas
        .iter()
        .find(|e| e.acao == acao::VISITA_CANCELADA)
        .expect("cancelamento preservado no historico (RN18)");
    assert_eq!(cancelamento.id_usuario, Some(cenario.coordenador.id));
    assert!(
        cancelamento
            .descricao
            .as_deref()
            .expect("descricao do cancelamento")
            .contains("Tecnico afastado temporariamente")
    );
    assert!(detalhe.registro.is_none());
}

#[tokio::test]
async fn detalhe_de_visita_sem_registro_retorna_campos_vazios() {
    let pool = preparar_banco().await;
    let cenario = cenario_padrao(&pool).await;
    let suites = montar(&pool);

    let visita = agendar(
        &suites,
        cenario.coordenador.id,
        cadastro_em(&cenario, 3, hora(9, 0)),
    )
    .await;

    let detalhe = suites
        .historico
        .detalhar_historico(cenario.tecnico.id, visita)
        .await
        .expect("detalhe da visita sem registro");

    assert!(detalhe.registro.is_none());
    assert!(detalhe.complementos.is_empty());
    assert_eq!(detalhe.mudancas.len(), 1, "somente o agendamento");
    assert_eq!(detalhe.mudancas[0].acao, acao::VISITA_AGENDADA);
}

#[tokio::test]
async fn tecnico_nao_consulta_detalhe_de_visita_alheia() {
    let pool = preparar_banco().await;
    let cenario = cenario_padrao(&pool).await;
    let outro_tecnico = criar_usuario_na_unidade(
        &pool,
        cenario.coordenador.id_unidade,
        PerfilUsuario::Tecnico,
        StatusUsuario::Ativo,
        false,
    )
    .await;
    let suites = montar(&pool);

    let visita = agendar(
        &suites,
        cenario.coordenador.id,
        cadastro_em(&cenario, 3, hora(9, 0)),
    )
    .await;

    let erro = suites
        .historico
        .detalhar_historico(outro_tecnico.id, visita)
        .await
        .expect_err("tecnico nao consulta visita de outro");

    assert!(matches!(erro, ErroHistorico::NaoEncontrado));

    // A tentativa negada fica registrada na auditoria (RN22).
    assert_eq!(
        contar_auditoria(&pool, outro_tecnico.id, acao::HISTORICO_ACESSO_NEGADO).await,
        1
    );
}

#[tokio::test]
async fn coordenador_alheio_nao_consulta_detalhe_e_tentativa_e_auditada() {
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

    let visita = agendar(
        &suites,
        cenario.coordenador.id,
        cadastro_em(&cenario, 3, hora(9, 0)),
    )
    .await;

    let erro = suites
        .historico
        .detalhar_historico(coordenador_outra_unidade.id, visita)
        .await
        .expect_err("coordenador alheio nao consulta detalhe");

    assert!(matches!(erro, ErroHistorico::NaoEncontrado));
    assert_eq!(
        contar_auditoria(
            &pool,
            coordenador_outra_unidade.id,
            acao::HISTORICO_ACESSO_NEGADO
        )
        .await,
        1
    );
}

#[tokio::test]
async fn detalhe_nao_encontra_visita_inexistente() {
    let pool = preparar_banco().await;
    let cenario = cenario_padrao(&pool).await;
    let suites = montar(&pool);

    let erro = suites
        .historico
        .detalhar_historico(cenario.tecnico.id, uuid::Uuid::new_v4())
        .await
        .expect_err("visita inexistente");

    assert!(matches!(erro, ErroHistorico::NaoEncontrado));
}
