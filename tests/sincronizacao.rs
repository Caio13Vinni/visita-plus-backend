// =============================================================================
// Testes de integração — Sincronização offline/online
// -----------------------------------------------------------------------------
// Exige o PostgreSQL do docker-compose em execucao:
//     docker compose up -d postgres
//     DATABASE_URL=postgres://... cargo test --test sincronizacao
//
// Cobre o envio de pendencias acumuladas no SQLite local (registros de
// visita e encaminhamentos feitos offline) e o recebimento da agenda do
// perfil no espelho local, com as regras de posse (RN05/RN06).
// =============================================================================

mod comum;

use std::sync::Arc;

use chrono::{Duration, NaiveTime, Utc};
use comum::*;
use visita_plus::casos_de_uso::agendamento::CasoDeUsoAgendamento;
use visita_plus::casos_de_uso::sincronizacao::CasoDeUsoSincronizacao;
use visita_plus::dominio::entidades::visita::CadastroVisita;
use visita_plus::dominio::enums::{PerfilUsuario, PrioridadeVisita, StatusUsuario};
use visita_plus::dominio::erros::ErroSincronizacao;
use visita_plus::infraestrutura::auditoria::{ServicoAuditoria, acao};
use visita_plus::infraestrutura::repositorios::encaminhamento_repositorio::RepositorioEncaminhamentoPostgres;
use visita_plus::infraestrutura::repositorios::familia_repositorio::RepositorioFamiliaPostgres;
use visita_plus::infraestrutura::repositorios::registro_repositorio::RepositorioRegistroPostgres;
use visita_plus::infraestrutura::repositorios::usuario_repositorio::RepositorioUsuarioPostgres;
use visita_plus::infraestrutura::repositorios::visita_repositorio::RepositorioVisitaPostgres;
use visita_plus::infraestrutura::sync::BancoLocal;

// =============================================================================
// Helpers de montagem
// =============================================================================

struct SuitesSync {
    agendamento: CasoDeUsoAgendamento,
    sincronizacao: CasoDeUsoSincronizacao,
}

fn montar(pool: &sqlx::PgPool, endereco_sqlite: &str) -> (SuitesSync, BancoLocal) {
    let repositorio_usuario = Arc::new(RepositorioUsuarioPostgres::novo(pool.clone()));
    let repositorio_familia = Arc::new(RepositorioFamiliaPostgres::novo(pool.clone()));
    let repositorio_visita = Arc::new(RepositorioVisitaPostgres::novo(pool.clone()));
    let auditoria = ServicoAuditoria::novo(pool.clone());
    let banco_local = BancoLocal::novo(endereco_sqlite);

    (
        SuitesSync {
            agendamento: CasoDeUsoAgendamento::novo(
                repositorio_visita.clone(),
                repositorio_familia.clone(),
                repositorio_usuario.clone(),
                auditoria.clone(),
            ),
            sincronizacao: CasoDeUsoSincronizacao::novo(
                banco_local.clone(),
                repositorio_visita,
                repositorio_familia,
                Arc::new(RepositorioRegistroPostgres::novo(pool.clone())),
                Arc::new(RepositorioEncaminhamentoPostgres::novo(pool.clone())),
                repositorio_usuario,
                auditoria,
            ),
        },
        banco_local,
    )
}

/// Caminho de arquivo SQLite unico por teste.
fn caminho_sqlite() -> String {
    format!("/tmp/visita_plus_sync_{}.db", uuid::Uuid::new_v4())
}

/// Banco local com esquema garantido (permite enfileirar antes da sync).
async fn banco_local_pronto(caminho: &str) -> BancoLocal {
    let banco = BancoLocal::novo(caminho);
    banco.garantir_esquema().await.expect("esquema local");
    banco
}

/// Coordenador + tecnico na mesma unidade.
async fn dupla(pool: &sqlx::PgPool) -> (UsuarioDeTeste, UsuarioDeTeste) {
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
    (coordenador, tecnico)
}

/// Agenda uma visita futura do tecnico na unidade; devolve (familia, visita).
/// O deslocamento em dias evita conflito de horario na agenda (RN10).
async fn agenda_visita_em(
    pool: &sqlx::PgPool,
    agendamento: &CasoDeUsoAgendamento,
    coordenador: &UsuarioDeTeste,
    tecnico: &UsuarioDeTeste,
    daqui_a_dias: i64,
) -> (uuid::Uuid, uuid::Uuid) {
    let familia = criar_familia(
        pool,
        coordenador.id_unidade,
        &format!("FAM-{}", sufixo_unico()),
    )
    .await;
    let visita = agendamento
        .agendar_visita(
            coordenador.id,
            CadastroVisita {
                id_familia: familia,
                id_tecnico: tecnico.id,
                data_visita: Utc::now().date_naive() + Duration::days(daqui_a_dias),
                horario: NaiveTime::from_hms_opt(9, 0, 0).expect("horario valido"),
                prioridade: PrioridadeVisita::Normal,
                objetivo: "Acompanhamento familiar".into(),
            },
        )
        .await
        .expect("visita agendada");
    (familia, visita.id)
}

/// Atalho para agenda daqui a 2 dias (sem conflito entre chamadas unicas).
async fn agenda_visita(
    pool: &sqlx::PgPool,
    agendamento: &CasoDeUsoAgendamento,
    coordenador: &UsuarioDeTeste,
    tecnico: &UsuarioDeTeste,
) -> (uuid::Uuid, uuid::Uuid) {
    agenda_visita_em(pool, agendamento, coordenador, tecnico, 2).await
}

fn payload_registro_realizada(id_visita: uuid::Uuid) -> String {
    serde_json::json!({
        "id_visita": id_visita,
        "resultado": "Realizada",
        "descricao": "Visita realizada com a familia presente.",
        "desfecho": "Familia orientada sobre os encaminhamentos.",
        "necessita_retorno": true,
        "motivo_nao_realizada": null,
    })
    .to_string()
}

fn payload_registro_nao_realizada(id_visita: uuid::Uuid) -> String {
    serde_json::json!({
        "id_visita": id_visita,
        "resultado": "Não realizada",
        "descricao": null,
        "desfecho": null,
        "necessita_retorno": false,
        "motivo_nao_realizada": "Familia ausente em tres tentativas.",
    })
    .to_string()
}

fn payload_encaminhamento(id_visita: uuid::Uuid, destino: &str, motivo: &str) -> String {
    serde_json::json!({
        "id_visita": id_visita,
        "destino": destino,
        "motivo": motivo,
    })
    .to_string()
}

/// Conexao direta ao arquivo SQLite do teste para inspecao do espelho/fila.
async fn inspecionar_sqlite(caminho: &str) -> sqlx::SqlitePool {
    sqlx::SqlitePool::connect(&format!("sqlite:{caminho}"))
        .await
        .expect("abrir sqlite de inspecao")
}

// =============================================================================
// Envio de pendencias (push)
// =============================================================================

#[tokio::test]
async fn tecnico_envia_pendencias_e_recebe_a_propria_agenda() {
    let pool = preparar_banco().await;
    let caminho = caminho_sqlite();
    let (suites, banco_local) = montar(&pool, &caminho);
    let (coordenador, tecnico) = dupla(&pool).await;
    let (_, visita) = agenda_visita(&pool, &suites.agendamento, &coordenador, &tecnico).await;

    let fila = banco_local_pronto(&caminho).await;
    fila.enfileirar("registro_visita", &payload_registro_realizada(visita))
        .await
        .expect("pendencia de registro enfileirada");
    fila.enfileirar(
        "encaminhamento",
        &payload_encaminhamento(visita, "UBS Central", "Avaliacao medica da crianca."),
    )
    .await
    .expect("pendencia de encaminhamento enfileirada");

    let resumo = suites
        .sincronizacao
        .sincronizar(tecnico.id)
        .await
        .expect("sincronizacao do tecnico");

    assert_eq!(resumo.enviados, 2);
    assert_eq!(resumo.erros, 0);
    assert_eq!(resumo.recebidos, 1);

    // Registro finalizado no central com o resultado enviado.
    let (status, resultado): (String, String) = sqlx::query_as(
        "SELECT status::text, resultado::text FROM registro_visita WHERE id_visita = $1",
    )
    .bind(visita)
    .fetch_one(&pool)
    .await
    .expect("registro no central");
    assert_eq!(status, "Finalizado");
    assert_eq!(resultado, "Realizada");

    // Visita fechada com o mesmo status do fluxo online (UC05).
    let status_visita: String =
        sqlx::query_scalar("SELECT status::text FROM visita WHERE id_visita = $1")
            .bind(visita)
            .fetch_one(&pool)
            .await
            .expect("status da visita");
    assert_eq!(status_visita, "Realizada");

    // Encaminhamento aplicado ao central (RF17).
    let destino: String =
        sqlx::query_scalar("SELECT destino FROM encaminhamento WHERE id_visita = $1")
            .bind(visita)
            .fetch_one(&pool)
            .await
            .expect("encaminhamento no central");
    assert_eq!(destino, "UBS Central");

    // Auditoria da operacao (RN22).
    assert_eq!(
        contar_auditoria(&pool, tecnico.id, acao::SINCRONIZACAO_REALIZADA).await,
        1
    );

    // Fila local: pendencias marcadas como enviadas.
    let pendentes = fila.listar_pendentes().await.expect("listar fila");
    assert!(pendentes.is_empty());

    // Espelho local recebeu a agenda e a familia correspondente.
    let sqlite = inspecionar_sqlite(&caminho).await;
    let visitas_locais: i64 = sqlx::query_scalar("SELECT count(*) FROM visita_local")
        .fetch_one(&sqlite)
        .await
        .expect("contar visitas locais");
    assert_eq!(visitas_locais, 1);
    let familias_locais: i64 = sqlx::query_scalar("SELECT count(*) FROM familia_local")
        .fetch_one(&sqlite)
        .await
        .expect("contar familias locais");
    assert_eq!(familias_locais, 1);
    assert!(
        banco_local
            .ultima_sincronizacao()
            .await
            .expect("meta")
            .is_some()
    );

    sqlite.close().await;
    std::fs::remove_file(&caminho).ok();
}

#[tokio::test]
async fn registro_nao_realizada_fecha_visita_correspondente() {
    let pool = preparar_banco().await;
    let caminho = caminho_sqlite();
    let (suites, _) = montar(&pool, &caminho);
    let (coordenador, tecnico) = dupla(&pool).await;
    let (_, visita) = agenda_visita(&pool, &suites.agendamento, &coordenador, &tecnico).await;

    let fila = banco_local_pronto(&caminho).await;
    fila.enfileirar("registro_visita", &payload_registro_nao_realizada(visita))
        .await
        .expect("pendencia enfileirada");

    let resumo = suites
        .sincronizacao
        .sincronizar(tecnico.id)
        .await
        .expect("sincronizacao");
    assert_eq!(resumo.enviados, 1);
    assert_eq!(resumo.erros, 0);

    let (resultado, motivo): (String, Option<String>) = sqlx::query_as(
        "SELECT resultado::text, motivo_nao_realizada FROM registro_visita WHERE id_visita = $1",
    )
    .bind(visita)
    .fetch_one(&pool)
    .await
    .expect("registro no central");
    assert_eq!(resultado, "Não realizada");
    assert_eq!(
        motivo.as_deref(),
        Some("Familia ausente em tres tentativas.")
    );

    let status_visita: String =
        sqlx::query_scalar("SELECT status::text FROM visita WHERE id_visita = $1")
            .bind(visita)
            .fetch_one(&pool)
            .await
            .expect("status da visita");
    assert_eq!(status_visita, "Não realizada");

    std::fs::remove_file(&caminho).ok();
}

#[tokio::test]
async fn reenvio_do_mesmo_registro_nao_duplica_no_central() {
    let pool = preparar_banco().await;
    let caminho = caminho_sqlite();
    let (suites, _) = montar(&pool, &caminho);
    let (coordenador, tecnico) = dupla(&pool).await;
    let (_, visita) = agenda_visita(&pool, &suites.agendamento, &coordenador, &tecnico).await;

    let fila = banco_local_pronto(&caminho).await;
    // Simula reenvio: a mesma pendencia duas vezes na fila.
    for _ in 0..2 {
        fila.enfileirar("registro_visita", &payload_registro_realizada(visita))
            .await
            .expect("pendencia enfileirada");
    }

    let resumo = suites
        .sincronizacao
        .sincronizar(tecnico.id)
        .await
        .expect("sincronizacao");

    // Ambas as pendencias sao consumidas (a segunda e idempotente).
    assert_eq!(resumo.enviados, 2);
    let total: i64 =
        sqlx::query_scalar("SELECT count(*) FROM registro_visita WHERE id_visita = $1")
            .bind(visita)
            .fetch_one(&pool)
            .await
            .expect("contar registros");
    assert_eq!(total, 1, "reenvio nao deve duplicar o registro");

    std::fs::remove_file(&caminho).ok();
}

#[tokio::test]
async fn pendencia_de_visita_de_outro_tecnico_vai_para_erro_sem_bloquear_a_fila() {
    let pool = preparar_banco().await;
    let caminho = caminho_sqlite();
    let (suites, _) = montar(&pool, &caminho);
    let (coordenador, tecnico) = dupla(&pool).await;
    let (outro_coordenador, outro_tecnico) = dupla(&pool).await;
    let (_, visita_do_tecnico) =
        agenda_visita(&pool, &suites.agendamento, &coordenador, &tecnico).await;
    let (_, visita_alheia) = agenda_visita(
        &pool,
        &suites.agendamento,
        &outro_coordenador,
        &outro_tecnico,
    )
    .await;

    let fila = banco_local_pronto(&caminho).await;
    fila.enfileirar(
        "registro_visita",
        &payload_registro_realizada(visita_alheia),
    )
    .await
    .expect("pendencia alheia enfileirada");
    fila.enfileirar(
        "registro_visita",
        &payload_registro_realizada(visita_do_tecnico),
    )
    .await
    .expect("pendencia propria enfileirada");

    let resumo = suites
        .sincronizacao
        .sincronizar(tecnico.id)
        .await
        .expect("sincronizacao");

    // A pendencia valida foi enviada; a alheia foi marcada com erro.
    assert_eq!(resumo.enviados, 1);
    assert_eq!(resumo.erros, 1);

    let total_alheia: i64 =
        sqlx::query_scalar("SELECT count(*) FROM registro_visita WHERE id_visita = $1")
            .bind(visita_alheia)
            .fetch_one(&pool)
            .await
            .expect("contar registros alheios");
    assert_eq!(total_alheia, 0);

    // A pendencia com erro permanece na fila para inspecao, sem reenvio.
    let sqlite = inspecionar_sqlite(&caminho).await;
    let com_erro: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM fila_sincronizacao WHERE sincronizado_em IS NULL AND erro IS NOT NULL",
    )
    .fetch_one(&sqlite)
    .await
    .expect("contar pendencias com erro");
    assert_eq!(com_erro, 1);
    sqlite.close().await;

    std::fs::remove_file(&caminho).ok();
}

#[tokio::test]
async fn payload_malformado_e_tipo_desconhecido_vao_para_erro() {
    let pool = preparar_banco().await;
    let caminho = caminho_sqlite();
    let (suites, _) = montar(&pool, &caminho);
    let (_, tecnico) = dupla(&pool).await;

    let fila = banco_local_pronto(&caminho).await;
    fila.enfileirar("registro_visita", "{json invalido")
        .await
        .expect("pendencia malformada enfileirada");
    fila.enfileirar("tipo_inexistente", "{}")
        .await
        .expect("pendencia de tipo desconhecido enfileirada");

    let resumo = suites
        .sincronizacao
        .sincronizar(tecnico.id)
        .await
        .expect("sincronizacao");

    assert_eq!(resumo.enviados, 0);
    assert_eq!(resumo.erros, 2);

    let sqlite = inspecionar_sqlite(&caminho).await;
    let com_erro: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM fila_sincronizacao WHERE sincronizado_em IS NULL AND erro IS NOT NULL",
    )
    .fetch_one(&sqlite)
    .await
    .expect("contar pendencias com erro");
    assert_eq!(com_erro, 2);
    sqlite.close().await;

    std::fs::remove_file(&caminho).ok();
}

#[tokio::test]
async fn encaminhamento_sem_destino_ou_motivo_e_rejeitado() {
    let pool = preparar_banco().await;
    let caminho = caminho_sqlite();
    let (suites, _) = montar(&pool, &caminho);
    let (coordenador, tecnico) = dupla(&pool).await;
    let (_, visita) = agenda_visita(&pool, &suites.agendamento, &coordenador, &tecnico).await;

    let fila = banco_local_pronto(&caminho).await;
    fila.enfileirar(
        "encaminhamento",
        &payload_encaminhamento(visita, "   ", "Motivo valido."),
    )
    .await
    .expect("pendencia sem destino enfileirada");

    let resumo = suites
        .sincronizacao
        .sincronizar(tecnico.id)
        .await
        .expect("sincronizacao");

    assert_eq!(resumo.enviados, 0);
    assert_eq!(resumo.erros, 1);

    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM encaminhamento WHERE id_visita = $1")
        .bind(visita)
        .fetch_one(&pool)
        .await
        .expect("contar encaminhamentos");
    assert_eq!(total, 0);

    std::fs::remove_file(&caminho).ok();
}

#[tokio::test]
async fn visita_inexistente_vai_para_erro() {
    let pool = preparar_banco().await;
    let caminho = caminho_sqlite();
    let (suites, _) = montar(&pool, &caminho);
    let (_, tecnico) = dupla(&pool).await;

    let fila = banco_local_pronto(&caminho).await;
    fila.enfileirar(
        "registro_visita",
        &payload_registro_realizada(uuid::Uuid::new_v4()),
    )
    .await
    .expect("pendencia enfileirada");

    let resumo = suites
        .sincronizacao
        .sincronizar(tecnico.id)
        .await
        .expect("sincronizacao");

    assert_eq!(resumo.enviados, 0);
    assert_eq!(resumo.erros, 1);

    std::fs::remove_file(&caminho).ok();
}

// =============================================================================
// Recebimento da agenda (pull) conforme o perfil (RN05/RN06)
// =============================================================================

#[tokio::test]
async fn tecnico_recebe_apenas_a_propria_agenda_no_espelho() {
    let pool = preparar_banco().await;
    let caminho = caminho_sqlite();
    let (suites, _) = montar(&pool, &caminho);
    let (coordenador, tecnico) = dupla(&pool).await;
    // Visita do tecnico logado e visita de outro tecnico da mesma unidade.
    let (_, visita_propria) =
        agenda_visita(&pool, &suites.agendamento, &coordenador, &tecnico).await;
    let (_, tecnico2) = dupla_na_unidade(&pool, coordenador.id_unidade).await;
    agenda_visita(&pool, &suites.agendamento, &coordenador, &tecnico2).await;

    let banco_local = banco_local_pronto(&caminho).await;
    let resumo = suites
        .sincronizacao
        .sincronizar(tecnico.id)
        .await
        .expect("sincronizacao");

    assert_eq!(resumo.recebidos, 1);
    assert_eq!(banco_local.contar_visitas().await.expect("contar"), 1);

    let sqlite = inspecionar_sqlite(&caminho).await;
    let espelhada: Option<String> = sqlx::query_scalar("SELECT id FROM visita_local WHERE id = $1")
        .bind(visita_propria.to_string())
        .fetch_optional(&sqlite)
        .await
        .expect("consultar espelho");
    assert!(espelhada.is_some(), "visita propria deve estar no espelho");
    sqlite.close().await;

    std::fs::remove_file(&caminho).ok();
}

#[tokio::test]
async fn coordenador_recebe_agenda_de_toda_a_unidade() {
    let pool = preparar_banco().await;
    let caminho = caminho_sqlite();
    let (suites, _) = montar(&pool, &caminho);
    let (coordenador, tecnico) = dupla(&pool).await;
    let (_, tecnico2) = dupla_na_unidade(&pool, coordenador.id_unidade).await;
    agenda_visita(&pool, &suites.agendamento, &coordenador, &tecnico).await;
    agenda_visita(&pool, &suites.agendamento, &coordenador, &tecnico2).await;

    let banco_local = banco_local_pronto(&caminho).await;
    let resumo = suites
        .sincronizacao
        .sincronizar(coordenador.id)
        .await
        .expect("sincronizacao do coordenador");

    assert_eq!(resumo.recebidos, 2);
    assert_eq!(banco_local.contar_visitas().await.expect("contar"), 2);

    std::fs::remove_file(&caminho).ok();
}

#[tokio::test]
async fn segunda_sincronizacao_atualiza_o_espelho_local() {
    let pool = preparar_banco().await;
    let caminho = caminho_sqlite();
    let (suites, _) = montar(&pool, &caminho);
    let (coordenador, tecnico) = dupla(&pool).await;
    agenda_visita(&pool, &suites.agendamento, &coordenador, &tecnico).await;

    let banco_local = banco_local_pronto(&caminho).await;
    let primeiro = suites
        .sincronizacao
        .sincronizar(tecnico.id)
        .await
        .expect("primeira sincronizacao");
    assert_eq!(primeiro.recebidos, 1);

    // Nova visita agendada apos a primeira sincronizacao (outro dia, RN10).
    agenda_visita_em(&pool, &suites.agendamento, &coordenador, &tecnico, 3).await;

    let segundo = suites
        .sincronizacao
        .sincronizar(tecnico.id)
        .await
        .expect("segunda sincronizacao");
    assert_eq!(segundo.recebidos, 2);
    assert_eq!(banco_local.contar_visitas().await.expect("contar"), 2);

    std::fs::remove_file(&caminho).ok();
}

// =============================================================================
// Permissões
// =============================================================================

#[tokio::test]
async fn conta_inativa_ou_inexistente_nao_sincroniza() {
    let pool = preparar_banco().await;
    let caminho = caminho_sqlite();
    let (suites, _) = montar(&pool, &caminho);

    let inativo = criar_usuario(&pool, PerfilUsuario::Tecnico, StatusUsuario::Inativo, false).await;
    let erro = suites
        .sincronizacao
        .sincronizar(inativo.id)
        .await
        .expect_err("conta inativa deve ser negada");
    assert!(matches!(erro, ErroSincronizacao::SemPermissao));

    let erro = suites
        .sincronizacao
        .sincronizar(uuid::Uuid::new_v4())
        .await
        .expect_err("usuario inexistente deve ser negado");
    assert!(matches!(erro, ErroSincronizacao::NaoEncontrado));

    std::fs::remove_file(&caminho).ok();
}

/// Segundo tecnico vinculado a uma unidade ja existente.
async fn dupla_na_unidade(
    pool: &sqlx::PgPool,
    id_unidade: uuid::Uuid,
) -> (UsuarioDeTeste, UsuarioDeTeste) {
    let coordenador = criar_usuario_na_unidade(
        pool,
        id_unidade,
        PerfilUsuario::Coordenador,
        StatusUsuario::Ativo,
        false,
    )
    .await;
    let tecnico = criar_usuario_na_unidade(
        pool,
        id_unidade,
        PerfilUsuario::Tecnico,
        StatusUsuario::Ativo,
        false,
    )
    .await;
    (coordenador, tecnico)
}
