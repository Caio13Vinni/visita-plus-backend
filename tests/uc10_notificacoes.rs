// =============================================================================
// Testes de integração — UC10 (Receber notificacoes)
// -----------------------------------------------------------------------------
// Exige o PostgreSQL do docker-compose em execucao:
//     docker compose up -d postgres
//     DATABASE_URL=postgres://... cargo test --test uc10_notificacoes
// =============================================================================

mod comum;

use std::sync::Arc;

use chrono::{Duration, NaiveTime, Utc};
use comum::*;
use visita_plus::casos_de_uso::agendamento::CasoDeUsoAgendamento;
use visita_plus::casos_de_uso::notificacoes::{CasoDeUsoNotificacoes, ServicoNotificacoes};
use visita_plus::casos_de_uso::solicitacao_retorno::CasoDeUsoSolicitacaoRetorno;
use visita_plus::dominio::entidades::solicitacao_retorno::AprovacaoRetorno;
use visita_plus::dominio::entidades::visita::CadastroVisita;
use visita_plus::dominio::enums::{
    PerfilUsuario, PrioridadeVisita, StatusUsuario, StatusVisita, TipoNotificacao,
};
use visita_plus::dominio::erros::ErroNotificacao;
use visita_plus::infraestrutura::auditoria::{ServicoAuditoria, acao};
use visita_plus::infraestrutura::repositorios::familia_repositorio::RepositorioFamiliaPostgres;
use visita_plus::infraestrutura::repositorios::notificacao_repositorio::RepositorioNotificacaoPostgres;
use visita_plus::infraestrutura::repositorios::solicitacao_repositorio::RepositorioSolicitacaoPostgres;
use visita_plus::infraestrutura::repositorios::usuario_repositorio::RepositorioUsuarioPostgres;
use visita_plus::infraestrutura::repositorios::visita_repositorio::{
    RepositorioVisita, RepositorioVisitaPostgres,
};

// =============================================================================
// Helpers de montagem
// =============================================================================

struct SuitesNotificacoes {
    agendamento: CasoDeUsoAgendamento,
    retorno: CasoDeUsoSolicitacaoRetorno,
    notificacoes: CasoDeUsoNotificacoes,
}

fn montar(pool: &sqlx::PgPool) -> SuitesNotificacoes {
    let repositorio_usuario = Arc::new(RepositorioUsuarioPostgres::novo(pool.clone()));
    let repositorio_familia = Arc::new(RepositorioFamiliaPostgres::novo(pool.clone()));
    let repositorio_visita = Arc::new(RepositorioVisitaPostgres::novo(pool.clone()));
    let repositorio_notificacao = Arc::new(RepositorioNotificacaoPostgres::novo(pool.clone()));
    let notificador = ServicoNotificacoes::novo(repositorio_notificacao.clone());
    let auditoria = ServicoAuditoria::novo(pool.clone());

    SuitesNotificacoes {
        agendamento: CasoDeUsoAgendamento::novo(
            repositorio_visita.clone(),
            repositorio_familia.clone(),
            repositorio_usuario.clone(),
            auditoria.clone(),
        )
        .com_notificador(notificador.clone()),
        retorno: CasoDeUsoSolicitacaoRetorno::novo(
            Arc::new(RepositorioSolicitacaoPostgres::novo(pool.clone())),
            repositorio_visita.clone(),
            repositorio_familia,
            repositorio_usuario.clone(),
            auditoria.clone(),
        )
        .com_notificador(notificador),
        notificacoes: CasoDeUsoNotificacoes::novo(
            repositorio_notificacao,
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
    visita: uuid::Uuid,
}

async fn cenario_com_visita_realizada(pool: &sqlx::PgPool) -> Cenario {
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

    RepositorioVisitaPostgres::novo(pool.clone())
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

fn hora(h: u32, m: u32) -> NaiveTime {
    NaiveTime::from_hms_opt(h, m, 0).expect("horario valido")
}

/// RN21: texto generico nunca expõe dados da familia.
fn assert_texto_generico(titulo: &str, mensagem: &str) {
    for sensivel in ["Familia Teste", "Rua de Teste", "98888-7777"] {
        assert!(!titulo.contains(sensivel), "titulo expõe '{sensivel}'");
        assert!(!mensagem.contains(sensivel), "mensagem expõe '{sensivel}'");
    }
}

// =============================================================================
// Gatilhos: agenda (UC04)
// =============================================================================

#[tokio::test]
async fn agendamento_cria_lembrete_generico_para_o_tecnico() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_realizada(&pool).await;
    let suites = montar(&pool);

    // O cenario ja agenda uma visita: deve existir um lembrete para o tecnico.
    let do_tecnico = suites
        .notificacoes
        .listar_notificacoes(cenario.tecnico.id, false)
        .await
        .expect("notificacoes do tecnico");

    assert_eq!(do_tecnico.len(), 1);
    assert_eq!(do_tecnico[0].tipo, TipoNotificacao::Lembrete);
    assert!(!do_tecnico[0].lida);
    assert_texto_generico(&do_tecnico[0].titulo, &do_tecnico[0].mensagem);

    // Coordenador e tecnico de outra unidade nao recebem nada.
    let do_coordenador = suites
        .notificacoes
        .listar_notificacoes(cenario.coordenador.id, false)
        .await
        .expect("notificacoes do coordenador");
    assert!(do_coordenador.is_empty());
}

#[tokio::test]
async fn reagendamento_avisa_o_tecnico_sobre_a_mudanca() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_realizada(&pool).await;
    let suites = montar(&pool);

    // Visita em status que admite reagendamento: agendar outra e alterar.
    let outra = suites
        .agendamento
        .agendar_visita(
            cenario.coordenador.id,
            CadastroVisita {
                id_familia: cenario.familia,
                id_tecnico: cenario.tecnico.id,
                data_visita: Utc::now().date_naive() + Duration::days(6),
                horario: hora(10, 0),
                prioridade: PrioridadeVisita::Normal,
                objetivo: "Visita para reagendar".into(),
            },
        )
        .await
        .expect("segunda visita agendada");

    suites
        .agendamento
        .reagendar_visita(
            cenario.coordenador.id,
            outra.id,
            visita_plus::dominio::entidades::visita::ReagendamentoVisita {
                data_visita: Utc::now().date_naive() + Duration::days(7),
                horario: hora(15, 30),
                id_tecnico: None,
            },
        )
        .await
        .expect("visita reagendada");

    let do_tecnico = suites
        .notificacoes
        .listar_notificacoes(cenario.tecnico.id, false)
        .await
        .expect("notificacoes do tecnico");

    assert_eq!(do_tecnico.len(), 3, "dois lembretes + aviso da mudanca");
    assert_eq!(do_tecnico[0].tipo, TipoNotificacao::Aviso);
    assert_texto_generico(&do_tecnico[0].titulo, &do_tecnico[0].mensagem);
}

#[tokio::test]
async fn cancelamento_avisa_o_tecnico_responsavel() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_realizada(&pool).await;
    let suites = montar(&pool);

    // Visita em status que admite cancelamento: agendar outra e cancelar.
    let outra = suites
        .agendamento
        .agendar_visita(
            cenario.coordenador.id,
            CadastroVisita {
                id_familia: cenario.familia,
                id_tecnico: cenario.tecnico.id,
                data_visita: Utc::now().date_naive() + Duration::days(6),
                horario: hora(10, 0),
                prioridade: PrioridadeVisita::Normal,
                objetivo: "Visita para cancelar".into(),
            },
        )
        .await
        .expect("segunda visita agendada");

    suites
        .agendamento
        .cancelar_visita(cenario.coordenador.id, outra.id, "Conflito de equipe")
        .await
        .expect("visita cancelada");

    let do_tecnico = suites
        .notificacoes
        .listar_notificacoes(cenario.tecnico.id, false)
        .await
        .expect("notificacoes do tecnico");

    assert_eq!(
        do_tecnico.len(),
        3,
        "dois lembretes + aviso de cancelamento"
    );
    let cancelamento = do_tecnico
        .iter()
        .find(|n| n.titulo == "Visita cancelada")
        .expect("aviso de cancelamento presente");
    assert_eq!(cancelamento.tipo, TipoNotificacao::Aviso);
    assert_texto_generico(&cancelamento.titulo, &cancelamento.mensagem);
}

// =============================================================================
// Gatilhos: pedido de retorno (UC08)
// =============================================================================

#[tokio::test]
async fn pedido_de_retorno_avisa_o_coordenador_da_unidade() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_realizada(&pool).await;
    let suites = montar(&pool);

    suites
        .retorno
        .solicitar_retorno(
            cenario.tecnico.id,
            cenario.visita,
            "Acompanhar encaminhamento de saude.",
            PrioridadeVisita::Alta,
        )
        .await
        .expect("pedido criado");

    let do_coordenador = suites
        .notificacoes
        .listar_notificacoes(cenario.coordenador.id, false)
        .await
        .expect("notificacoes do coordenador");

    assert_eq!(do_coordenador.len(), 1);
    assert_eq!(do_coordenador[0].tipo, TipoNotificacao::Aviso);
    assert_texto_generico(&do_coordenador[0].titulo, &do_coordenador[0].mensagem);

    // O tecnico solicitante so tem o lembrete do agendamento do cenario.
    let do_tecnico = suites
        .notificacoes
        .listar_notificacoes(cenario.tecnico.id, false)
        .await
        .expect("notificacoes do tecnico");
    assert_eq!(do_tecnico.len(), 1, "apenas o lembrete da visita");
}

#[tokio::test]
async fn aprovacao_do_retorno_avisa_o_solicitante() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_realizada(&pool).await;
    let suites = montar(&pool);

    let pedido = suites
        .retorno
        .solicitar_retorno(
            cenario.tecnico.id,
            cenario.visita,
            "Retorno necessario.",
            PrioridadeVisita::Normal,
        )
        .await
        .expect("pedido criado");

    suites
        .retorno
        .aprovar_retorno(
            cenario.coordenador.id,
            pedido.id,
            AprovacaoRetorno {
                id_tecnico: cenario.tecnico.id,
                data_visita: Utc::now().date_naive() + Duration::days(7),
                horario: hora(14, 0),
            },
        )
        .await
        .expect("retorno aprovado");

    let do_tecnico = suites
        .notificacoes
        .listar_notificacoes(cenario.tecnico.id, false)
        .await
        .expect("notificacoes do tecnico");

    assert_eq!(do_tecnico.len(), 2, "lembrete + aviso de aprovacao");
    assert_eq!(do_tecnico[0].titulo, "Retorno aprovado");
    assert_eq!(do_tecnico[0].tipo, TipoNotificacao::Aviso);
    assert_texto_generico(&do_tecnico[0].titulo, &do_tecnico[0].mensagem);
}

#[tokio::test]
async fn rejeicao_do_retorno_avisa_o_solicitante() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_realizada(&pool).await;
    let suites = montar(&pool);

    let pedido = suites
        .retorno
        .solicitar_retorno(
            cenario.tecnico.id,
            cenario.visita,
            "Retorno necessario.",
            PrioridadeVisita::Normal,
        )
        .await
        .expect("pedido criado");

    suites
        .retorno
        .rejeitar_retorno(
            cenario.coordenador.id,
            pedido.id,
            "Ja atendido em outro fluxo.",
        )
        .await
        .expect("pedido rejeitado");

    let do_tecnico = suites
        .notificacoes
        .listar_notificacoes(cenario.tecnico.id, false)
        .await
        .expect("notificacoes do tecnico");

    assert_eq!(do_tecnico.len(), 2, "lembrete + aviso de rejeicao");
    assert_eq!(do_tecnico[0].titulo, "Retorno nao aprovado");
    assert_texto_generico(&do_tecnico[0].titulo, &do_tecnico[0].mensagem);
}

// =============================================================================
// Consumo: listagem e abertura
// =============================================================================

#[tokio::test]
async fn listagem_e_por_destinatario_e_ordena_da_mais_recente() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_realizada(&pool).await;
    let outro_tecnico = criar_usuario_na_unidade(
        &pool,
        cenario.coordenador.id_unidade,
        PerfilUsuario::Tecnico,
        StatusUsuario::Ativo,
        false,
    )
    .await;
    let suites = montar(&pool);

    let notificador =
        ServicoNotificacoes::novo(Arc::new(RepositorioNotificacaoPostgres::novo(pool.clone())));
    notificador
        .avisar_nova_visita(outro_tecnico.id)
        .await
        .expect("notificacao para outro tecnico");

    let do_tecnico = suites
        .notificacoes
        .listar_notificacoes(cenario.tecnico.id, false)
        .await
        .expect("notificacoes do tecnico");
    let do_outro = suites
        .notificacoes
        .listar_notificacoes(outro_tecnico.id, false)
        .await
        .expect("notificacoes do outro tecnico");

    assert_eq!(do_tecnico.len(), 1, "tecnico nao recebe aviso alheio");
    assert_eq!(do_outro.len(), 1);

    // Ordenacao da mais recente para a mais antiga (duas emissões sequenciais).
    notificador
        .avisar_mudanca_agenda(cenario.tecnico.id)
        .await
        .expect("segunda notificacao");
    let atualizadas = suites
        .notificacoes
        .listar_notificacoes(cenario.tecnico.id, false)
        .await
        .expect("notificacoes atualizadas");
    assert_eq!(atualizadas.len(), 2);
    assert!(atualizadas[0].data_hora >= atualizadas[1].data_hora);
}

#[tokio::test]
async fn filtro_somente_nao_lidas() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_realizada(&pool).await;
    let suites = montar(&pool);

    let todas = suites
        .notificacoes
        .listar_notificacoes(cenario.tecnico.id, false)
        .await
        .expect("todas as notificacoes");
    assert_eq!(todas.len(), 1);

    let pendente = &todas[0];
    suites
        .notificacoes
        .abrir_notificacao(cenario.tecnico.id, pendente.id)
        .await
        .expect("notificacao aberta");

    let nao_lidas = suites
        .notificacoes
        .listar_notificacoes(cenario.tecnico.id, true)
        .await
        .expect("somente nao lidas");
    assert!(nao_lidas.is_empty());

    let lidas = suites
        .notificacoes
        .listar_notificacoes(cenario.tecnico.id, false)
        .await
        .expect("todas");
    assert_eq!(lidas.len(), 1);
    assert!(lidas[0].lida);
}

#[tokio::test]
async fn abrir_notificacao_marca_como_lida() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_realizada(&pool).await;
    let suites = montar(&pool);

    let antes = suites
        .notificacoes
        .listar_notificacoes(cenario.tecnico.id, false)
        .await
        .expect("notificacoes");
    let id = antes[0].id;

    let aberta = suites
        .notificacoes
        .abrir_notificacao(cenario.tecnico.id, id)
        .await
        .expect("notificacao aberta");

    assert!(aberta.lida);
    assert!(aberta.lida_em.is_some());

    // Reabrir uma notificacao ja lida nao gera erro nem altera o horario.
    let reaberta = suites
        .notificacoes
        .abrir_notificacao(cenario.tecnico.id, id)
        .await
        .expect("reabertura idempotente");
    assert_eq!(reaberta.lida_em, aberta.lida_em);
}

#[tokio::test]
async fn abrir_notificacao_alheia_nega_e_registra_tentativa() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_realizada(&pool).await;
    let outro_tecnico = criar_usuario_na_unidade(
        &pool,
        cenario.coordenador.id_unidade,
        PerfilUsuario::Tecnico,
        StatusUsuario::Ativo,
        false,
    )
    .await;
    let suites = montar(&pool);

    let alvo = suites
        .notificacoes
        .listar_notificacoes(cenario.tecnico.id, false)
        .await
        .expect("notificacoes do tecnico")[0]
        .id;

    let erro = suites
        .notificacoes
        .abrir_notificacao(outro_tecnico.id, alvo)
        .await
        .expect_err("notificacao alheia nao abre");

    assert!(matches!(erro, ErroNotificacao::NaoEncontrado));

    // A tentativa fica registrada na auditoria (RN22) e o aviso segue nao lido.
    assert_eq!(
        contar_auditoria(&pool, outro_tecnico.id, acao::NOTIFICACAO_ACESSO_NEGADO).await,
        1
    );
    let pendente = suites
        .notificacoes
        .listar_notificacoes(cenario.tecnico.id, true)
        .await
        .expect("nao lidas do tecnico");
    assert_eq!(pendente.len(), 1, "notificacao original segue nao lida");
}

#[tokio::test]
async fn abrir_notificacao_inexistente_retorna_nao_encontrado() {
    let pool = preparar_banco().await;
    let cenario = cenario_com_visita_realizada(&pool).await;
    let suites = montar(&pool);

    let erro = suites
        .notificacoes
        .abrir_notificacao(cenario.tecnico.id, uuid::Uuid::new_v4())
        .await
        .expect_err("notificacao inexistente");

    assert!(matches!(erro, ErroNotificacao::NaoEncontrado));
}
