// =============================================================================
// Testes de integração — UC02 (Gerenciar profissionais) e UC03 (Familias)
// -----------------------------------------------------------------------------
// Exige o PostgreSQL do docker-compose em execucao:
//     docker compose up -d postgres
//     DATABASE_URL=postgres://... cargo test --test uc02_uc03_cadastros
// =============================================================================

mod comum;

use std::sync::Arc;

use comum::*;
use uuid::Uuid;
use visita_plus::casos_de_uso::familias::CasoDeUsoFamilias;
use visita_plus::casos_de_uso::profissionais::{
    CadastroProfissional, CasoDeUsoProfissionais, EdicaoProfissional,
};
use visita_plus::dominio::entidades::familia::CadastroFamilia;
use visita_plus::dominio::enums::{PerfilUsuario, StatusFamilia, StatusUsuario};
use visita_plus::dominio::erros::ErroCadastro;
use visita_plus::infraestrutura::auditoria::{ServicoAuditoria, acao};
use visita_plus::infraestrutura::repositorios::familia_repositorio::RepositorioFamiliaPostgres;
use visita_plus::infraestrutura::repositorios::usuario_repositorio::RepositorioUsuarioPostgres;
use visita_plus::infraestrutura::repositorios::visita_repositorio::RepositorioVisitaPostgres;

// =============================================================================
// Helpers de montagem
// =============================================================================

struct SuitesCadastros {
    profissionais: CasoDeUsoProfissionais,
    familias: CasoDeUsoFamilias,
}

fn montar(pool: &sqlx::PgPool) -> SuitesCadastros {
    let repositorio_usuario = Arc::new(RepositorioUsuarioPostgres::novo(pool.clone()));
    let repositorio_familia = Arc::new(RepositorioFamiliaPostgres::novo(pool.clone()));
    let repositorio_visita = Arc::new(RepositorioVisitaPostgres::novo(pool.clone()));

    let servico_auditoria = ServicoAuditoria::novo(pool.clone());

    SuitesCadastros {
        profissionais: CasoDeUsoProfissionais::novo(
            repositorio_usuario.clone(),
            repositorio_visita.clone(),
            servico_auditoria.clone(),
        ),
        familias: CasoDeUsoFamilias::novo(
            repositorio_familia,
            repositorio_usuario,
            repositorio_visita,
            servico_auditoria,
        ),
    }
}

/// Cria um coordenador logado e devolve seu id e unidade.
async fn coordenador_logado(pool: &sqlx::PgPool) -> (Uuid, Uuid) {
    let usuario = criar_usuario(
        pool,
        PerfilUsuario::Coordenador,
        StatusUsuario::Ativo,
        false,
    )
    .await;
    (usuario.id, usuario.id_unidade)
}

// =============================================================================
// UC02 — Gerenciar profissionais
// =============================================================================

#[tokio::test]
async fn coordenador_cadastra_tecnico_com_primeiro_acesso_exigindo_troca() {
    let pool = preparar_banco().await;
    let (coordenador, unidade) = coordenador_logado(&pool).await;
    let suite = montar(&pool);
    let token = sufixo_unico();

    let cadastro = CadastroProfissional {
        id_unidade: unidade,
        nome: "Tecnico Novo".into(),
        email: format!("tecnico_novo_{token}@teste.com"),
        senha: "SenhaInicial#123".into(),
        perfil: PerfilUsuario::Tecnico,
    };

    let criado = suite
        .profissionais
        .cadastrar_profissional(coordenador, cadastro.clone())
        .await
        .expect("coordenador deveria cadastrar tecnico");

    assert_eq!(criado.perfil, PerfilUsuario::Tecnico);
    assert_eq!(criado.status, StatusUsuario::Ativo);
    assert!(
        criado.exige_troca_senha,
        "novo usuario deve trocar a senha no primeiro acesso (RN05)"
    );
}

#[tokio::test]
async fn tecnico_nao_pode_cadastrar_profissional() {
    let pool = preparar_banco().await;
    let tecnico = criar_usuario(&pool, PerfilUsuario::Tecnico, StatusUsuario::Ativo, false).await;
    let suite = montar(&pool);
    let token = sufixo_unico();

    let cadastro = CadastroProfissional {
        id_unidade: tecnico.id_unidade,
        nome: "Qualquer".into(),
        email: format!("bloqueado_{token}@teste.com"),
        senha: "Senha#123".into(),
        perfil: PerfilUsuario::Tecnico,
    };

    let erro = suite
        .profissionais
        .cadastrar_profissional(tecnico.id, cadastro)
        .await
        .expect_err("tecnico nao deveria cadastrar profissionais");

    assert!(matches!(erro, ErroCadastro::SemPermissao));
}

#[tokio::test]
async fn email_duplicado_no_cadastro_e_rejeitado() {
    let pool = preparar_banco().await;
    let (coordenador, unidade) = coordenador_logado(&pool).await;
    let suite = montar(&pool);
    let token = sufixo_unico();
    let email = format!("duplicado_prof_{token}@teste.com");

    let cadastro = CadastroProfissional {
        id_unidade: unidade,
        nome: "Primeiro".into(),
        email: email.clone(),
        senha: "Senha#123".into(),
        perfil: PerfilUsuario::Tecnico,
    };
    suite
        .profissionais
        .cadastrar_profissional(coordenador, cadastro)
        .await
        .expect("primeiro cadastro deveria ter sucesso");

    let segundo = CadastroProfissional {
        id_unidade: unidade,
        nome: "Segundo".into(),
        email,
        senha: "Senha#123".into(),
        perfil: PerfilUsuario::Coordenador,
    };
    let erro = suite
        .profissionais
        .cadastrar_profissional(coordenador, segundo)
        .await
        .expect_err("email duplicado deveria falhar");

    assert!(matches!(erro, ErroCadastro::EmailJaExistente));
}

#[tokio::test]
async fn coordenador_desativa_tecnico_sem_visitas_futuras() {
    let pool = preparar_banco().await;
    let (coordenador, unidade) = coordenador_logado(&pool).await;
    let tecnico = criar_usuario_na_unidade(
        &pool,
        unidade,
        PerfilUsuario::Tecnico,
        StatusUsuario::Ativo,
        false,
    )
    .await;
    let suite = montar(&pool);

    let atualizado = suite
        .profissionais
        .alterar_status_profissional(coordenador, tecnico.id, StatusUsuario::Inativo)
        .await
        .expect("tecnico sem visitas futuras pode ser desativado");

    assert_eq!(atualizado.status, StatusUsuario::Inativo);
}

#[tokio::test]
async fn nao_desativa_tecnico_com_visita_futura_agendada() {
    let pool = preparar_banco().await;
    let (coordenador, unidade) = coordenador_logado(&pool).await;
    let tecnico = criar_usuario_na_unidade(
        &pool,
        unidade,
        PerfilUsuario::Tecnico,
        StatusUsuario::Ativo,
        false,
    )
    .await;
    let suite = montar(&pool);
    let token = sufixo_unico();

    // Cria uma familia e uma visita futura agendada para o tecnico
    let familia: Uuid = sqlx::query_scalar(
        "INSERT INTO familia (id_unidade, codigo, nome_referencia, contato, endereco, criado_por)
         VALUES ($1, $2, 'Familia Futura', 'contato', 'endereco', $3)
         RETURNING id_familia",
    )
    .bind(unidade)
    .bind(format!("FAM-FUT-{token}"))
    .bind(coordenador)
    .fetch_one(&pool)
    .await
    .expect("falha ao criar familia");

    sqlx::query(
        "INSERT INTO visita (id_familia, id_tecnico, id_agendado_por, data_visita, horario, prioridade, objetivo, status)
         VALUES ($1, $2, $3, CURRENT_DATE + 1, '09:00:00'::time, 'Normal', 'Acompanhamento', 'Agendada')",
    )
    .bind(familia)
    .bind(tecnico.id)
    .bind(coordenador)
    .execute(&pool)
    .await
    .expect("falha ao criar visita futura");

    let erro = suite
        .profissionais
        .alterar_status_profissional(coordenador, tecnico.id, StatusUsuario::Inativo)
        .await
        .expect_err("tecnico com visita futura nao pode ser desativado");

    assert!(matches!(erro, ErroCadastro::TecnicoComVisitasFuturas));
}

#[tokio::test]
async fn coordenador_edita_nome_e_perfil_do_profissional() {
    let pool = preparar_banco().await;
    let (coordenador, unidade) = coordenador_logado(&pool).await;
    let tecnico = criar_usuario_na_unidade(
        &pool,
        unidade,
        PerfilUsuario::Tecnico,
        StatusUsuario::Ativo,
        false,
    )
    .await;
    let suite = montar(&pool);

    let atualizado = suite
        .profissionais
        .editar_profissional(
            coordenador,
            tecnico.id,
            EdicaoProfissional {
                nome: "Tecnico Renomeado".into(),
                perfil: PerfilUsuario::Coordenador,
            },
        )
        .await
        .expect("edicao deveria ter sucesso");

    assert_eq!(atualizado.nome, "Tecnico Renomeado");
    assert_eq!(atualizado.perfil, PerfilUsuario::Coordenador);
}

#[tokio::test]
async fn lista_profissionais_apenas_para_coordenador() {
    let pool = preparar_banco().await;
    let (coordenador, unidade) = coordenador_logado(&pool).await;
    let suite = montar(&pool);

    let profissionais = suite
        .profissionais
        .listar_profissionais(coordenador, Some(unidade))
        .await
        .expect("coordenador deveria listar profissionais");
    assert!(!profissionais.is_empty());
}

// =============================================================================
// UC03 — Gerenciar familias
// =============================================================================

fn cadastro_familia_exemplo(token: &str) -> CadastroFamilia {
    CadastroFamilia {
        codigo: format!("FAM-{token}"),
        nome_referencia: "Familia Exemplar".into(),
        contato: "(11) 98888-7777".into(),
        endereco: "Rua das Flores, 42".into(),
    }
}

#[tokio::test]
async fn coordenador_cadastra_familia_ficticia() {
    let pool = preparar_banco().await;
    let (coordenador, unidade) = coordenador_logado(&pool).await;
    let suite = montar(&pool);
    let token = sufixo_unico();

    let criada = suite
        .familias
        .cadastrar_familia(coordenador, unidade, &cadastro_familia_exemplo(&token))
        .await
        .expect("coordenador deveria cadastrar familia");

    assert_eq!(criada.status, StatusFamilia::Ativa);
    assert_eq!(criada.codigo, format!("FAM-{token}"));
    assert_eq!(criada.criado_por, Some(coordenador));
}

#[tokio::test]
async fn familia_com_codigo_duplicado_e_rejeitada() {
    let pool = preparar_banco().await;
    let (coordenador, unidade) = coordenador_logado(&pool).await;
    let suite = montar(&pool);
    let token = sufixo_unico();
    let cadastro = cadastro_familia_exemplo(&token);

    suite
        .familias
        .cadastrar_familia(coordenador, unidade, &cadastro)
        .await
        .expect("primeira familia deveria ser cadastrada");

    let erro = suite
        .familias
        .cadastrar_familia(coordenador, unidade, &cadastro)
        .await
        .expect_err("codigo duplicado deveria falhar");

    assert!(matches!(erro, ErroCadastro::CodigoJaExistente));
}

#[tokio::test]
async fn tecnico_nao_pode_cadastrar_familia() {
    let pool = preparar_banco().await;
    let tecnico = criar_usuario(&pool, PerfilUsuario::Tecnico, StatusUsuario::Ativo, false).await;
    let suite = montar(&pool);
    let token = sufixo_unico();

    let erro = suite
        .familias
        .cadastrar_familia(
            tecnico.id,
            tecnico.id_unidade,
            &cadastro_familia_exemplo(&token),
        )
        .await
        .expect_err("tecnico nao deveria cadastrar familias");

    assert!(matches!(erro, ErroCadastro::SemPermissao));
}

#[tokio::test]
async fn edicao_de_familia_preserva_status_e_atualiza_dados() {
    let pool = preparar_banco().await;
    let (coordenador, unidade) = coordenador_logado(&pool).await;
    let suite = montar(&pool);
    let token = sufixo_unico();

    let criada = suite
        .familias
        .cadastrar_familia(coordenador, unidade, &cadastro_familia_exemplo(&token))
        .await
        .expect("cadastro inicial");

    let edicao = CadastroFamilia {
        codigo: format!("FAM-{token}"),
        nome_referencia: "Familia Renomeada".into(),
        contato: "(11) 97777-6666".into(),
        endereco: "Rua Nova, 100".into(),
    };

    let atualizada = suite
        .familias
        .editar_familia(coordenador, criada.id, &edicao)
        .await
        .expect("edicao deveria ter sucesso");

    assert_eq!(atualizada.nome_referencia, "Familia Renomeada");
    assert_eq!(atualizada.status, StatusFamilia::Ativa);
    assert_eq!(atualizada.criado_por, Some(coordenador));
}

#[tokio::test]
async fn familia_pode_ser_inativada_pelo_coordenador() {
    let pool = preparar_banco().await;
    let (coordenador, unidade) = coordenador_logado(&pool).await;
    let suite = montar(&pool);
    let token = sufixo_unico();

    let criada = suite
        .familias
        .cadastrar_familia(coordenador, unidade, &cadastro_familia_exemplo(&token))
        .await
        .expect("cadastro inicial");

    let inativada = suite
        .familias
        .alterar_status_familia(coordenador, criada.id, StatusFamilia::Inativa)
        .await
        .expect("inativacao deveria ter sucesso");

    assert_eq!(inativada.status, StatusFamilia::Inativa);
}

#[tokio::test]
async fn listagem_de_familias_aceita_filtro_por_status() {
    let pool = preparar_banco().await;
    let (coordenador, unidade) = coordenador_logado(&pool).await;
    let tecnico = criar_usuario(&pool, PerfilUsuario::Tecnico, StatusUsuario::Ativo, false).await;
    let suite = montar(&pool);
    let token = sufixo_unico();

    let criada = suite
        .familias
        .cadastrar_familia(coordenador, unidade, &cadastro_familia_exemplo(&token))
        .await
        .expect("cadastro inicial");

    // Coordenador filtra por status e enxerga a familia da propria unidade.
    let ativas = suite
        .familias
        .listar_familias(coordenador, Some(unidade), Some(StatusFamilia::Ativa))
        .await
        .expect("listagem deveria funcionar para coordenador");

    assert!(
        ativas.iter().any(|familia| familia.id == criada.id),
        "familia criada deveria aparecer na listagem"
    );

    // RN05/RN06: sem visitas atribuidas, o tecnico nao tem visibilidade
    // sobre nenhuma familia — a familia so e acessada a partir de visita.
    let do_tecnico = suite
        .familias
        .listar_familias(tecnico.id, Some(unidade), Some(StatusFamilia::Ativa))
        .await
        .expect("listagem deveria funcionar para tecnico");

    assert!(
        do_tecnico.is_empty(),
        "tecnico sem visitas nao deveria listar familias (RN05/RN06)"
    );
}

// =============================================================================
// RN05/RN06 — isolamento de unidade nos cadastros (UC02/UC03)
// =============================================================================

#[tokio::test]
async fn coordenador_nao_cadastra_familia_em_outra_unidade() {
    let pool = preparar_banco().await;
    let (coordenador, _) = coordenador_logado(&pool).await;
    let outra_unidade = criar_unidade(&pool).await;
    let suite = montar(&pool);
    let token = sufixo_unico();

    let erro = suite
        .familias
        .cadastrar_familia(
            coordenador,
            outra_unidade,
            &cadastro_familia_exemplo(&token),
        )
        .await
        .expect_err("coordenador nao deveria cadastrar familia fora da propria unidade");

    assert!(matches!(erro, ErroCadastro::SemPermissao));
}

#[tokio::test]
async fn coordenador_nao_edita_familia_de_outra_unidade() {
    let pool = preparar_banco().await;
    let (coordenador, _) = coordenador_logado(&pool).await;
    let outra_unidade = criar_unidade(&pool).await;
    let familia_alheia =
        criar_familia(&pool, outra_unidade, &format!("FAM-ALH-{}", sufixo_unico())).await;
    let suite = montar(&pool);
    let token = sufixo_unico();

    let erro = suite
        .familias
        .editar_familia(
            coordenador,
            familia_alheia,
            &cadastro_familia_exemplo(&token),
        )
        .await
        .expect_err("coordenador nao deveria editar familia de outra unidade");

    assert!(matches!(erro, ErroCadastro::SemPermissao));
}

#[tokio::test]
async fn coordenador_nao_edita_profissional_de_outra_unidade() {
    let pool = preparar_banco().await;
    let (coordenador, _) = coordenador_logado(&pool).await;
    let tecnico_alheio =
        criar_usuario(&pool, PerfilUsuario::Tecnico, StatusUsuario::Ativo, false).await;
    let suite = montar(&pool);

    let erro = suite
        .profissionais
        .editar_profissional(
            coordenador,
            tecnico_alheio.id,
            EdicaoProfissional {
                nome: "Tentativa Ilicita".into(),
                perfil: PerfilUsuario::Tecnico,
            },
        )
        .await
        .expect_err("coordenador nao deveria editar profissional de outra unidade");

    assert!(matches!(erro, ErroCadastro::SemPermissao));
}

#[tokio::test]
async fn coordenador_nao_altera_status_de_profissional_de_outra_unidade() {
    let pool = preparar_banco().await;
    let (coordenador, _) = coordenador_logado(&pool).await;
    let tecnico_alheio =
        criar_usuario(&pool, PerfilUsuario::Tecnico, StatusUsuario::Ativo, false).await;
    let suite = montar(&pool);

    let erro = suite
        .profissionais
        .alterar_status_profissional(coordenador, tecnico_alheio.id, StatusUsuario::Inativo)
        .await
        .expect_err("coordenador nao deveria desativar profissional de outra unidade");

    assert!(matches!(erro, ErroCadastro::SemPermissao));
}

#[tokio::test]
async fn coordenador_nao_lista_familias_de_outra_unidade() {
    let pool = preparar_banco().await;
    let (coordenador, unidade) = coordenador_logado(&pool).await;
    let outra_unidade = criar_unidade(&pool).await;
    criar_familia(&pool, unidade, &format!("FAM-MINHA-{}", sufixo_unico())).await;
    criar_familia(&pool, outra_unidade, &format!("FAM-ALH-{}", sufixo_unico())).await;
    let suite = montar(&pool);

    // Sem filtro: somente a familia da propria unidade aparece.
    let familias = suite
        .familias
        .listar_familias(coordenador, None, None)
        .await
        .expect("listagem sem filtro deveria funcionar");
    assert_eq!(
        familias.len(),
        1,
        "coordenador so enxerga familias da propria unidade (RN05/RN06)"
    );

    // Com filtro de outra unidade: rejeitado.
    let erro = suite
        .familias
        .listar_familias(coordenador, Some(outra_unidade), None)
        .await
        .expect_err("coordenador nao deveria filtrar familias de outra unidade");
    assert!(matches!(erro, ErroCadastro::SemPermissao));
}

#[tokio::test]
async fn tecnico_lista_apenas_familias_vinculadas_as_proprias_visitas() {
    let pool = preparar_banco().await;
    let (coordenador, unidade) = coordenador_logado(&pool).await;
    let tecnico = criar_usuario_na_unidade(
        &pool,
        unidade,
        PerfilUsuario::Tecnico,
        StatusUsuario::Ativo,
        false,
    )
    .await;
    let outro_tecnico = criar_usuario_na_unidade(
        &pool,
        unidade,
        PerfilUsuario::Tecnico,
        StatusUsuario::Ativo,
        false,
    )
    .await;
    let suite = montar(&pool);
    let token = sufixo_unico();

    let familia_com_visita = criar_familia(&pool, unidade, &format!("FAM-VIS-{token}")).await;
    // Familia da unidade sem visita para o tecnico: nao pode aparecer.
    let _familia_sem_visita = criar_familia(&pool, unidade, &format!("FAM-SEM-{token}")).await;
    let familia_de_outro = criar_familia(&pool, unidade, &format!("FAM-OUT-{token}")).await;

    // Visita do tecnico na primeira familia.
    sqlx::query(
        "INSERT INTO visita (id_familia, id_tecnico, id_agendado_por, data_visita, horario, objetivo)
         VALUES ($1, $2, $3, CURRENT_DATE + 1, '09:00:00'::time, 'Acompanhamento')",
    )
    .bind(familia_com_visita)
    .bind(tecnico.id)
    .bind(coordenador)
    .execute(&pool)
    .await
    .expect("falha ao criar visita do tecnico");

    // Visita de OUTRO tecnico na terceira familia.
    sqlx::query(
        "INSERT INTO visita (id_familia, id_tecnico, id_agendado_por, data_visita, horario, objetivo)
         VALUES ($1, $2, $3, CURRENT_DATE + 1, '11:00:00'::time, 'Acompanhamento')",
    )
    .bind(familia_de_outro)
    .bind(outro_tecnico.id)
    .bind(coordenador)
    .execute(&pool)
    .await
    .expect("falha ao criar visita de outro tecnico");

    let familias = suite
        .familias
        .listar_familias(tecnico.id, None, None)
        .await
        .expect("listagem deveria funcionar para tecnico");

    assert_eq!(
        familias.len(),
        1,
        "tecnico so enxerga a familia da propria visita (RN05/RN06)"
    );
    assert_eq!(familias[0].id, familia_com_visita);
}

// =============================================================================
// RN22 — auditoria dos cadastros (UC02/UC03)
// =============================================================================

#[tokio::test]
async fn cadastro_e_manutencao_de_familia_geram_auditoria() {
    let pool = preparar_banco().await;
    let (coordenador, unidade) = coordenador_logado(&pool).await;
    let suite = montar(&pool);
    let token = sufixo_unico();

    let criada = suite
        .familias
        .cadastrar_familia(coordenador, unidade, &cadastro_familia_exemplo(&token))
        .await
        .expect("cadastro deveria ter sucesso");

    let mut edicao = cadastro_familia_exemplo(&token);
    edicao.nome_referencia = "Familia Renomeada".into();
    suite
        .familias
        .editar_familia(coordenador, criada.id, &edicao)
        .await
        .expect("edicao deveria ter sucesso");

    suite
        .familias
        .alterar_status_familia(coordenador, criada.id, StatusFamilia::Inativa)
        .await
        .expect("alteracao de status deveria ter sucesso");

    assert_eq!(
        contar_auditoria(&pool, coordenador, acao::FAMILIA_CRIADA).await,
        1,
        "cadastro deve gerar trilha de auditoria (RN22)"
    );
    assert_eq!(
        contar_auditoria(&pool, coordenador, acao::FAMILIA_ALTERADA).await,
        2,
        "edicao e mudanca de status devem gerar trilha de auditoria (RN22)"
    );
}

#[tokio::test]
async fn cadastro_e_manutencao_de_profissional_geram_auditoria() {
    let pool = preparar_banco().await;
    let (coordenador, unidade) = coordenador_logado(&pool).await;
    let suite = montar(&pool);
    let token = sufixo_unico();

    let criado = suite
        .profissionais
        .cadastrar_profissional(
            coordenador,
            CadastroProfissional {
                id_unidade: unidade,
                nome: "Profissional Auditado".into(),
                email: format!("auditado_{token}@teste.com"),
                senha: "Senha#123".into(),
                perfil: PerfilUsuario::Tecnico,
            },
        )
        .await
        .expect("cadastro deveria ter sucesso");

    suite
        .profissionais
        .editar_profissional(
            coordenador,
            criado.id,
            EdicaoProfissional {
                nome: "Profissional Renomeado".into(),
                perfil: PerfilUsuario::Tecnico,
            },
        )
        .await
        .expect("edicao deveria ter sucesso");

    suite
        .profissionais
        .alterar_status_profissional(coordenador, criado.id, StatusUsuario::Inativo)
        .await
        .expect("alteracao de status deveria ter sucesso");

    assert_eq!(
        contar_auditoria(&pool, coordenador, acao::PROFISSIONAL_CRIADO).await,
        1,
        "cadastro deve gerar trilha de auditoria (RN22)"
    );
    assert_eq!(
        contar_auditoria(&pool, coordenador, acao::PROFISSIONAL_ALTERADO).await,
        2,
        "edicao e mudanca de status devem gerar trilha de auditoria (RN22)"
    );
}
