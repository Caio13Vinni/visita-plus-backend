// =============================================================================
// Testes de integração — UC08 (Solicitacao de retorno)
// -----------------------------------------------------------------------------
// Exige o PostgreSQL do docker-compose em execucao:
//     docker compose up -d postgres
//     DATABASE_URL=postgres://... cargo test --test uc08_solicitacao_retorno
// =============================================================================

mod comum;

use std::sync::Arc;

use chrono::{Duration, NaiveTime, Utc};
use comum::*;
use visita_plus::casos_de_uso::agendamento::CasoDeUsoAgendamento;
use visita_plus::casos_de_uso::solicitacao_retorno::CasoDeUsoSolicitacaoRetorno;
use visita_plus::dominio::entidades::solicitacao_retorno::AprovacaoRetorno;
use visita_plus::dominio::entidades::visita::CadastroVisita;
use visita_plus::dominio::enums::{
    PerfilUsuario, PrioridadeVisita, StatusSolicitacao, StatusUsuario, StatusVisita,
};
use visita_plus::dominio::erros::ErroSolicitacao;
use visita_plus::infraestrutura::auditoria::{ServicoAuditoria, acao};
use visita_plus::infraestrutura::repositorios::familia_repositorio::RepositorioFamiliaPostgres;
use visita_plus::infraestrutura::repositorios::solicitacao_repositorio::{
    FiltroSolicitacao, RepositorioSolicitacaoPostgres,
};
use visita_plus::infraestrutura::repositorios::usuario_repositorio::RepositorioUsuarioPostgres;
use visita_plus::infraestrutura::repositorios::visita_repositorio::{
    RepositorioVisita, RepositorioVisitaPostgres,
};

// =============================================================================
// Helpers de montagem
// =============================================================================

struct SuitesRetorno {
    agendamento: CasoDeUsoAgendamento,
    retorno: CasoDeUsoSolicitacaoRetorno,
    repositorio_visita: RepositorioVisitaPostgres,
}

fn montar(pool: &sqlx::PgPool) -> SuitesRetorno {
    let repositorio_usuario = Arc::new(RepositorioUsuarioPostgres::novo(pool.clone()));
    let repositorio_familia = Arc::new(RepositorioFamiliaPostgres::novo(pool.clone()));
    let repositorio_visita = RepositorioVisitaPostgres::novo(pool.clone());
    let auditoria = ServicoAuditoria::novo(pool.clone());

    SuitesRetorno {
        agendamento: CasoDeUsoAgendamento::novo(
            Arc::new(RepositorioVisitaPostgres::novo(pool.clone())),
            repositorio_familia.clone(),
            repositorio_usuario.clone(),
            auditoria.clone(),
        ),
        retorno: CasoDeUsoSolicitacaoRetorno::novo(
            Arc::new(RepositorioSolicitacaoPostgres::novo(pool.clone())),
            Arc::new(RepositorioVisitaPostgres::novo(pool.clone())),
            repositorio_familia,
            repositorio_usuario,
            auditoria,
        ),
        repositorio_visita,
    }
}

/// Coordenador + tecnico da mesma unidade + familia + visita realizada.
struct Cenario {
    coordenador: UsuarioDeTeste,
    tecnico: UsuarioDeTeste,
    familia: uuid::Uuid,
    visita: uuid::Uuid,
}

async fn cenario_com_visita_realizada(pool: &sqlx::PgPool, token: &str) -> Cenario {
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

    suites
        .repositorio_visita
        .atualizar_status(visita.id, StatusVisita::Realizada)
        .await
        .expect("visita realizada");

    Cenario {
        coordenador,
        tecnico,
        familia,
        visita: visita.id,
    }
}

fn aprovacao_padrao(id_tecnico: uuid::Uuid) -> AprovacaoRetorno {
    AprovacaoRetorno {
        id_tecnico,
        data_visita: Utc::now().date_naive() + Duration::days(5),
        horario: NaiveTime::from_hms_opt(14, 0, 0).expect("horario valido"),
    }
}

// =============================================================================
// Solicitacao de retorno (RN19)
// =============================================================================

#[tokio::test]
async fn tecnico_solicita_retorno_para_visita_realizada() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_realizada(&pool, &sufixo_unico()).await;
    let suites = montar(&pool);

    let solicitacao = suites
        .retorno
        .solicitar_retorno(
            cenario.tecnico.id,
            cenario.visita,
            "Crianca com suspeita de escorbuto; acompanhar retorno medico.",
            PrioridadeVisita::Alta,
        )
        .await
        .expect("solicitacao criada");

    assert_eq!(solicitacao.id_visita, cenario.visita);
    assert_eq!(solicitacao.solicitado_por, cenario.tecnico.id);
    assert_eq!(solicitacao.prioridade, PrioridadeVisita::Alta);
    assert_eq!(solicitacao.status, StatusSolicitacao::Pendente);
    assert_eq!(solicitacao.decidido_por, None);

    assert_eq!(
        contar_auditoria(&pool, cenario.tecnico.id, acao::RETORNO_SOLICITADO).await,
        1
    );
}

#[tokio::test]
async fn motivo_da_solicitacao_e_obrigatorio() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_realizada(&pool, &sufixo_unico()).await;
    let suites = montar(&pool);

    let erro = suites
        .retorno
        .solicitar_retorno(
            cenario.tecnico.id,
            cenario.visita,
            "   ",
            PrioridadeVisita::Normal,
        )
        .await
        .expect_err("motivo vazio deve falhar");

    assert!(matches!(erro, ErroSolicitacao::MotivoObrigatorio));
}

#[tokio::test]
async fn coordenador_nao_pode_solicitar_retorno() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_realizada(&pool, &sufixo_unico()).await;
    let suites = montar(&pool);

    let erro = suites
        .retorno
        .solicitar_retorno(
            cenario.coordenador.id,
            cenario.visita,
            "Demanda identificada pela gestao.",
            PrioridadeVisita::Normal,
        )
        .await
        .expect_err("coordenador nao solicita retorno");

    assert!(matches!(erro, ErroSolicitacao::SemPermissao));
}

#[tokio::test]
async fn visita_nao_realizada_nao_admite_retorno() {
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
                objetivo: "Primeira visita a familia".into(),
            },
        )
        .await
        .expect("visita agendada");

    let erro = suites
        .retorno
        .solicitar_retorno(
            tecnico.id,
            visita.id,
            "Motivo qualquer.",
            PrioridadeVisita::Normal,
        )
        .await
        .expect_err("visita agendada nao admite retorno");

    assert!(matches!(erro, ErroSolicitacao::VisitaInvalida));
}

#[tokio::test]
async fn visita_de_outro_tecnico_e_invisivel_para_solicitacao() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_realizada(&pool, &sufixo_unico()).await;
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
        .retorno
        .solicitar_retorno(
            outro_tecnico.id,
            cenario.visita,
            "Motivo qualquer.",
            PrioridadeVisita::Normal,
        )
        .await
        .expect_err("visita alheia nao pode ser solicitada");

    assert!(matches!(erro, ErroSolicitacao::NaoEncontrado));
}

#[tokio::test]
async fn pedido_em_aberto_bloqueia_nova_solicitacao_para_a_mesma_visita() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_realizada(&pool, &sufixo_unico()).await;
    let suites = montar(&pool);

    suites
        .retorno
        .solicitar_retorno(
            cenario.tecnico.id,
            cenario.visita,
            "Primeiro pedido.",
            PrioridadeVisita::Normal,
        )
        .await
        .expect("primeira solicitacao");

    let erro = suites
        .retorno
        .solicitar_retorno(
            cenario.tecnico.id,
            cenario.visita,
            "Segundo pedido.",
            PrioridadeVisita::Normal,
        )
        .await
        .expect_err("pedido em aberto deve barrar novo pedido");

    assert!(matches!(erro, ErroSolicitacao::SolicitacaoAbertaExistente));
}

// =============================================================================
// Aprovacao do retorno (RN19/RN20)
// =============================================================================

#[tokio::test]
async fn coordenador_aprova_e_agenda_visita_de_retorno() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_realizada(&pool, &sufixo_unico()).await;
    let suites = montar(&pool);

    let pedido = suites
        .retorno
        .solicitar_retorno(
            cenario.tecnico.id,
            cenario.visita,
            "Retorno para acompanhamento de encaminhamento.",
            PrioridadeVisita::Alta,
        )
        .await
        .expect("solicitacao criada");

    let resultado = suites
        .retorno
        .aprovar_retorno(
            cenario.coordenador.id,
            pedido.id,
            aprovacao_padrao(cenario.tecnico.id),
        )
        .await
        .expect("retorno aprovado e agendado");

    let solicitacao = resultado.solicitacao;
    assert_eq!(solicitacao.status, StatusSolicitacao::Agendada);
    assert_eq!(solicitacao.decidido_por, Some(cenario.coordenador.id));
    assert!(solicitacao.decidido_em.is_some());

    let visita = resultado.visita;
    assert_eq!(visita.id_origem_retorno, Some(pedido.id));
    assert_eq!(visita.id_familia, cenario.familia);
    assert_eq!(visita.id_tecnico, cenario.tecnico.id);
    assert_eq!(visita.prioridade, PrioridadeVisita::Alta);
    assert_eq!(visita.status, StatusVisita::Agendada);

    // A visita criada persiste o vinculo de origem no banco (RN19).
    let persistida = suites
        .repositorio_visita
        .buscar_por_id(visita.id)
        .await
        .expect("consulta da visita")
        .expect("visita persistida");
    assert_eq!(persistida.id_origem_retorno, Some(pedido.id));

    assert_eq!(
        contar_auditoria(&pool, cenario.coordenador.id, acao::RETORNO_AGENDADO).await,
        1
    );
}

#[tokio::test]
async fn aprovacao_com_data_retroativa_e_rejeitada() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_realizada(&pool, &sufixo_unico()).await;
    let suites = montar(&pool);

    let pedido = suites
        .retorno
        .solicitar_retorno(
            cenario.tecnico.id,
            cenario.visita,
            "Motivo do retorno.",
            PrioridadeVisita::Normal,
        )
        .await
        .expect("solicitacao criada");

    let mut aprovacao = aprovacao_padrao(cenario.tecnico.id);
    aprovacao.data_visita = Utc::now().date_naive() - Duration::days(1);

    let erro = suites
        .retorno
        .aprovar_retorno(cenario.coordenador.id, pedido.id, aprovacao)
        .await
        .expect_err("data retroativa deve falhar");

    assert!(matches!(erro, ErroSolicitacao::DataRetroativa));
}

#[tokio::test]
async fn aprovacao_respeita_conflito_de_horario_do_tecnico() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_realizada(&pool, &sufixo_unico()).await;
    let suites = montar(&pool);

    let pedido = suites
        .retorno
        .solicitar_retorno(
            cenario.tecnico.id,
            cenario.visita,
            "Motivo do retorno.",
            PrioridadeVisita::Normal,
        )
        .await
        .expect("solicitacao criada");

    let aprovacao = aprovacao_padrao(cenario.tecnico.id);

    // Ocupando a agenda do tecnico no mesmo horario (RN10).
    suites
        .agendamento
        .agendar_visita(
            cenario.coordenador.id,
            CadastroVisita {
                id_familia: cenario.familia,
                id_tecnico: cenario.tecnico.id,
                data_visita: aprovacao.data_visita,
                horario: aprovacao.horario,
                prioridade: PrioridadeVisita::Normal,
                objetivo: "Visita ja ocupando o horario".into(),
            },
        )
        .await
        .expect("visita ocupando o horario");

    let erro = suites
        .retorno
        .aprovar_retorno(cenario.coordenador.id, pedido.id, aprovacao)
        .await
        .expect_err("conflito de horario deve falhar");

    assert!(matches!(erro, ErroSolicitacao::ConflitoDeHorario));
}

#[tokio::test]
async fn aprovacao_com_tecnico_de_outra_unidade_e_invalida() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_realizada(&pool, &sufixo_unico()).await;
    let tecnico_outra_unidade =
        criar_usuario(&pool, PerfilUsuario::Tecnico, StatusUsuario::Ativo, false).await;
    let suites = montar(&pool);

    let pedido = suites
        .retorno
        .solicitar_retorno(
            cenario.tecnico.id,
            cenario.visita,
            "Motivo do retorno.",
            PrioridadeVisita::Normal,
        )
        .await
        .expect("solicitacao criada");

    let erro = suites
        .retorno
        .aprovar_retorno(
            cenario.coordenador.id,
            pedido.id,
            aprovacao_padrao(tecnico_outra_unidade.id),
        )
        .await
        .expect_err("tecnico de outra unidade deve falhar");

    assert!(matches!(erro, ErroSolicitacao::DadosInvalidos(_)));
}

#[tokio::test]
async fn tecnico_nao_pode_aprovar_retorno() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_realizada(&pool, &sufixo_unico()).await;
    let suites = montar(&pool);

    let pedido = suites
        .retorno
        .solicitar_retorno(
            cenario.tecnico.id,
            cenario.visita,
            "Motivo do retorno.",
            PrioridadeVisita::Normal,
        )
        .await
        .expect("solicitacao criada");

    let erro = suites
        .retorno
        .aprovar_retorno(
            cenario.tecnico.id,
            pedido.id,
            aprovacao_padrao(cenario.tecnico.id),
        )
        .await
        .expect_err("tecnico nao aprova retorno");

    assert!(matches!(erro, ErroSolicitacao::SemPermissao));
}

#[tokio::test]
async fn pedido_agendado_nao_pode_ser_decidido_novamente() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_realizada(&pool, &sufixo_unico()).await;
    let suites = montar(&pool);

    let pedido = suites
        .retorno
        .solicitar_retorno(
            cenario.tecnico.id,
            cenario.visita,
            "Motivo do retorno.",
            PrioridadeVisita::Normal,
        )
        .await
        .expect("solicitacao criada");

    suites
        .retorno
        .aprovar_retorno(
            cenario.coordenador.id,
            pedido.id,
            aprovacao_padrao(cenario.tecnico.id),
        )
        .await
        .expect("retorno aprovado");

    let erro = suites
        .retorno
        .rejeitar_retorno(cenario.coordenador.id, pedido.id, "Tentativa tardia.")
        .await
        .expect_err("pedido ja agendado nao pode ser decidido");

    assert!(matches!(erro, ErroSolicitacao::EstadoInvalido));
}

// =============================================================================
// Rejeicao e cancelamento (RN19/RN20)
// =============================================================================

#[tokio::test]
async fn coordenador_rejeita_pedido_com_justificativa() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_realizada(&pool, &sufixo_unico()).await;
    let suites = montar(&pool);

    let pedido = suites
        .retorno
        .solicitar_retorno(
            cenario.tecnico.id,
            cenario.visita,
            "Motivo do retorno.",
            PrioridadeVisita::Normal,
        )
        .await
        .expect("solicitacao criada");

    let rejeitada = suites
        .retorno
        .rejeitar_retorno(
            cenario.coordenador.id,
            pedido.id,
            "Demanda absorvida pelo acompanhamento mensal vigente.",
        )
        .await
        .expect("pedido rejeitado");

    assert_eq!(rejeitada.status, StatusSolicitacao::Rejeitada);
    assert_eq!(rejeitada.decidido_por, Some(cenario.coordenador.id));
    assert_eq!(
        rejeitada.decisao_motivo.as_deref(),
        Some("Demanda absorvida pelo acompanhamento mensal vigente.")
    );
    assert!(rejeitada.decidido_em.is_some());

    assert_eq!(
        contar_auditoria(&pool, cenario.coordenador.id, acao::RETORNO_REJEITADO).await,
        1
    );
}

#[tokio::test]
async fn rejeicao_exige_justificativa_obrigatoria() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_realizada(&pool, &sufixo_unico()).await;
    let suites = montar(&pool);

    let pedido = suites
        .retorno
        .solicitar_retorno(
            cenario.tecnico.id,
            cenario.visita,
            "Motivo do retorno.",
            PrioridadeVisita::Normal,
        )
        .await
        .expect("solicitacao criada");

    let erro = suites
        .retorno
        .rejeitar_retorno(cenario.coordenador.id, pedido.id, "   ")
        .await
        .expect_err("rejeicao sem justificativa deve falhar");

    assert!(matches!(erro, ErroSolicitacao::JustificativaObrigatoria));
}

#[tokio::test]
async fn pedido_rejeitado_permite_nova_solicitacao_para_a_mesma_visita() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_realizada(&pool, &sufixo_unico()).await;
    let suites = montar(&pool);

    let pedido = suites
        .retorno
        .solicitar_retorno(
            cenario.tecnico.id,
            cenario.visita,
            "Primeiro pedido.",
            PrioridadeVisita::Normal,
        )
        .await
        .expect("solicitacao criada");

    suites
        .retorno
        .rejeitar_retorno(
            cenario.coordenador.id,
            pedido.id,
            "Sem necessidade no momento.",
        )
        .await
        .expect("pedido rejeitado");

    let novo = suites
        .retorno
        .solicitar_retorno(
            cenario.tecnico.id,
            cenario.visita,
            "Novo contexto justifica o retorno.",
            PrioridadeVisita::Normal,
        )
        .await
        .expect("nova solicitacao permitida apos rejeicao");

    assert_eq!(novo.status, StatusSolicitacao::Pendente);
}

#[tokio::test]
async fn solicitante_cancela_o_proprio_pedido_pendente() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_realizada(&pool, &sufixo_unico()).await;
    let suites = montar(&pool);

    let pedido = suites
        .retorno
        .solicitar_retorno(
            cenario.tecnico.id,
            cenario.visita,
            "Motivo do retorno.",
            PrioridadeVisita::Normal,
        )
        .await
        .expect("solicitacao criada");

    let cancelada = suites
        .retorno
        .cancelar_solicitacao(cenario.tecnico.id, pedido.id)
        .await
        .expect("pedido cancelado");

    assert_eq!(cancelada.status, StatusSolicitacao::Cancelada);
    assert_eq!(cancelada.decidido_por, Some(cenario.tecnico.id));

    assert_eq!(
        contar_auditoria(&pool, cenario.tecnico.id, acao::RETORNO_CANCELADO).await,
        1
    );
}

#[tokio::test]
async fn somente_o_solicitante_cancela_o_pedido() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_realizada(&pool, &sufixo_unico()).await;
    let suites = montar(&pool);

    let pedido = suites
        .retorno
        .solicitar_retorno(
            cenario.tecnico.id,
            cenario.visita,
            "Motivo do retorno.",
            PrioridadeVisita::Normal,
        )
        .await
        .expect("solicitacao criada");

    let erro = suites
        .retorno
        .cancelar_solicitacao(cenario.coordenador.id, pedido.id)
        .await
        .expect_err("coordenador nao cancela pedido alheio");

    assert!(matches!(erro, ErroSolicitacao::SemPermissao));
}

#[tokio::test]
async fn pedido_cancelado_permite_nova_solicitacao_para_a_mesma_visita() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_realizada(&pool, &sufixo_unico()).await;
    let suites = montar(&pool);

    let pedido = suites
        .retorno
        .solicitar_retorno(
            cenario.tecnico.id,
            cenario.visita,
            "Primeiro pedido.",
            PrioridadeVisita::Normal,
        )
        .await
        .expect("solicitacao criada");

    suites
        .retorno
        .cancelar_solicitacao(cenario.tecnico.id, pedido.id)
        .await
        .expect("pedido cancelado");

    let novo = suites
        .retorno
        .solicitar_retorno(
            cenario.tecnico.id,
            cenario.visita,
            "Pedido refeito com dados corretos.",
            PrioridadeVisita::Normal,
        )
        .await
        .expect("nova solicitacao permitida apos cancelamento");

    assert_eq!(novo.status, StatusSolicitacao::Pendente);
}

// =============================================================================
// Listagem por perfil (RN05/RN06)
// =============================================================================

#[tokio::test]
async fn listagem_respeita_a_visibilidade_do_perfil() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_realizada(&pool, &sufixo_unico()).await;
    let suites = montar(&pool);

    let pedido = suites
        .retorno
        .solicitar_retorno(
            cenario.tecnico.id,
            cenario.visita,
            "Motivo do retorno.",
            PrioridadeVisita::Normal,
        )
        .await
        .expect("solicitacao criada");

    // Tecnico responsavel enxerga o pedido da propria agenda.
    let do_tecnico = suites
        .retorno
        .listar_solicitacoes(cenario.tecnico.id, FiltroSolicitacao::default())
        .await
        .expect("listagem do tecnico");
    assert!(do_tecnico.iter().any(|s| s.id == pedido.id));

    // Coordenador da mesma unidade enxerga o pedido.
    let do_coordenador = suites
        .retorno
        .listar_solicitacoes(cenario.coordenador.id, FiltroSolicitacao::default())
        .await
        .expect("listagem do coordenador");
    assert!(do_coordenador.iter().any(|s| s.id == pedido.id));

    // Coordenador de outra unidade nao enxerga o pedido.
    let outro_coordenador = criar_usuario(
        &pool,
        PerfilUsuario::Coordenador,
        StatusUsuario::Ativo,
        false,
    )
    .await;
    let de_outra_unidade = suites
        .retorno
        .listar_solicitacoes(outro_coordenador.id, FiltroSolicitacao::default())
        .await
        .expect("listagem de outra unidade");
    assert!(!de_outra_unidade.iter().any(|s| s.id == pedido.id));
}
