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

    SuitesCadastros {
        profissionais: CasoDeUsoProfissionais::novo(
            repositorio_usuario.clone(),
            repositorio_visita,
        ),
        familias: CasoDeUsoFamilias::novo(repositorio_familia, repositorio_usuario),
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
    let (coordenador, _) = coordenador_logado(&pool).await;
    let tecnico = criar_usuario(&pool, PerfilUsuario::Tecnico, StatusUsuario::Ativo, false).await;
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
    let tecnico = criar_usuario(&pool, PerfilUsuario::Tecnico, StatusUsuario::Ativo, false).await;
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
    let (coordenador, _) = coordenador_logado(&pool).await;
    let tecnico = criar_usuario(&pool, PerfilUsuario::Tecnico, StatusUsuario::Ativo, false).await;
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

    // Tecnico tambem consulta a listagem
    let ativas = suite
        .familias
        .listar_familias(tecnico.id, Some(unidade), Some(StatusFamilia::Ativa))
        .await
        .expect("listagem deveria funcionar para tecnico");

    assert!(
        ativas.iter().any(|familia| familia.id == criada.id),
        "familia criada deveria aparecer na listagem"
    );
}
