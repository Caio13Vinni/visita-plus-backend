// =============================================================================
// Estado compartilhado da aplicacao (pool, SQLite, servicos e casos de uso)
// =============================================================================

use sqlx::PgPool;
use std::env;
use std::sync::Arc;

use crate::casos_de_uso::agendamento::CasoDeUsoAgendamento;
use crate::casos_de_uso::auditoria::CasoDeUsoAuditoria;
use crate::casos_de_uso::autenticacao::CasoDeUsoAutenticacao;
use crate::casos_de_uso::familias::CasoDeUsoFamilias;
use crate::casos_de_uso::historico::CasoDeUsoHistorico;
use crate::casos_de_uso::indicadores::CasoDeUsoIndicadores;
use crate::casos_de_uso::notificacoes::{CasoDeUsoNotificacoes, ServicoNotificacoes};
use crate::casos_de_uso::profissionais::CasoDeUsoProfissionais;
use crate::casos_de_uso::recuperacao_acesso::CasoDeUsoRecuperacaoAcesso;
use crate::casos_de_uso::registro::CasoDeUsoRegistro;
use crate::casos_de_uso::solicitacao_retorno::CasoDeUsoSolicitacaoRetorno;
use crate::infraestrutura::auditoria::ServicoAuditoria;
use crate::infraestrutura::repositorios::auditoria_repositorio::RepositorioAuditoriaPostgres;
use crate::infraestrutura::repositorios::familia_repositorio::RepositorioFamiliaPostgres;
use crate::infraestrutura::repositorios::indicadores_repositorio::RepositorioIndicadoresPostgres;
use crate::infraestrutura::repositorios::notificacao_repositorio::RepositorioNotificacaoPostgres;
use crate::infraestrutura::repositorios::recuperacao_repositorio::RepositorioRecuperacaoPostgres;
use crate::infraestrutura::repositorios::registro_repositorio::RepositorioRegistroPostgres;
use crate::infraestrutura::repositorios::solicitacao_repositorio::RepositorioSolicitacaoPostgres;
use crate::infraestrutura::repositorios::usuario_repositorio::RepositorioUsuarioPostgres;
use crate::infraestrutura::repositorios::visita_repositorio::RepositorioVisitaPostgres;
use crate::infraestrutura::seguranca::jwt::ServicoJwt;

/// Estado transportado pelos handlers da API.
#[derive(Clone)]
pub struct EstadoAplicacao {
    /// Pool de conexoes com o banco central (PostgreSQL).
    pub pool: PgPool,
    /// Endereco do arquivo SQLite que simula o banco local mobile.
    pub endereco_sqlite: String,
    /// Caso de uso UC01 (autenticacao) compartilhado pelos handlers.
    pub autenticacao: CasoDeUsoAutenticacao,
    /// Caso de uso UC01/RF03 (recuperacao de acesso) compartilhado pelos handlers.
    pub recuperacao: CasoDeUsoRecuperacaoAcesso,
    /// Caso de uso UC02 (profissionais) compartilhado pelos handlers.
    pub profissionais: CasoDeUsoProfissionais,
    /// Caso de uso UC03 (familias) compartilhado pelos handlers.
    pub familias: CasoDeUsoFamilias,
    /// Caso de uso UC04 (agendamento de visitas) compartilhado pelos handlers.
    pub agendamento: CasoDeUsoAgendamento,
    /// Caso de uso UC05 (registro da visita) compartilhado pelos handlers.
    pub registro: CasoDeUsoRegistro,
    /// Caso de uso UC08 (solicitacao de retorno) compartilhado pelos handlers.
    pub solicitacao_retorno: CasoDeUsoSolicitacaoRetorno,
    /// Caso de uso UC09 (consulta ao historico) compartilhado pelos handlers.
    pub historico: CasoDeUsoHistorico,
    /// Caso de uso UC10 (notificacoes) compartilhado pelos handlers.
    pub notificacoes: CasoDeUsoNotificacoes,
    /// Caso de uso UC11 (indicadores) compartilhado pelos handlers.
    pub indicadores: CasoDeUsoIndicadores,
    /// Caso de uso UC12 (consulta a auditoria) compartilhado pelos handlers.
    pub auditoria: CasoDeUsoAuditoria,
    /// Servico de validacao de tokens JWT.
    pub servico_jwt: ServicoJwt,
}

impl EstadoAplicacao {
    /// Cria um novo estado compartilhado, montando os servicos da aplicacao.
    pub fn novo(pool: PgPool, endereco_sqlite: String) -> Arc<EstadoAplicacao> {
        let segredo = env::var("JWT_SEGREDO")
            .unwrap_or_else(|_| "segredo_de_desenvolvimento_visita_plus".to_string());
        let expiracao_minutos = env::var("JWT_EXPIRACAO_MINUTOS")
            .ok()
            .and_then(|valor| valor.parse().ok())
            .unwrap_or(1440);

        let servico_jwt = ServicoJwt::novo(segredo, expiracao_minutos);
        let servico_auditoria = ServicoAuditoria::novo(pool.clone());
        let servico_auditoria_notificacoes = servico_auditoria.clone();

        let repositorio_notificacao = Arc::new(RepositorioNotificacaoPostgres::novo(pool.clone()));
        let servico_notificacoes = ServicoNotificacoes::novo(repositorio_notificacao.clone());

        let repositorio_usuario = Arc::new(RepositorioUsuarioPostgres::novo(pool.clone()));
        let repositorio_familia = Arc::new(RepositorioFamiliaPostgres::novo(pool.clone()));
        let repositorio_visita = Arc::new(RepositorioVisitaPostgres::novo(pool.clone()));

        let autenticacao = CasoDeUsoAutenticacao::novo(
            repositorio_usuario.clone(),
            servico_jwt.clone(),
            servico_auditoria.clone(),
        );
        let recuperacao = CasoDeUsoRecuperacaoAcesso::novo(
            Arc::new(RepositorioRecuperacaoPostgres::novo(pool.clone())),
            repositorio_usuario.clone(),
            servico_auditoria.clone(),
        );
        let profissionais =
            CasoDeUsoProfissionais::novo(repositorio_usuario.clone(), repositorio_visita.clone());
        let familias =
            CasoDeUsoFamilias::novo(repositorio_familia.clone(), repositorio_usuario.clone());
        let agendamento = CasoDeUsoAgendamento::novo(
            repositorio_visita.clone(),
            repositorio_familia.clone(),
            repositorio_usuario.clone(),
            servico_auditoria.clone(),
        )
        .com_notificador(servico_notificacoes.clone());
        let registro = CasoDeUsoRegistro::novo(
            Arc::new(RepositorioRegistroPostgres::novo(pool.clone())),
            repositorio_visita.clone(),
            repositorio_familia.clone(),
            repositorio_usuario.clone(),
            servico_auditoria.clone(),
        );
        let solicitacao_retorno = CasoDeUsoSolicitacaoRetorno::novo(
            Arc::new(RepositorioSolicitacaoPostgres::novo(pool.clone())),
            repositorio_visita.clone(),
            repositorio_familia.clone(),
            repositorio_usuario.clone(),
            servico_auditoria.clone(),
        )
        .com_notificador(servico_notificacoes);
        let historico = CasoDeUsoHistorico::novo(
            repositorio_visita,
            repositorio_familia,
            Arc::new(RepositorioRegistroPostgres::novo(pool.clone())),
            repositorio_usuario.clone(),
            servico_auditoria,
        );
        let notificacoes = CasoDeUsoNotificacoes::novo(
            repositorio_notificacao,
            repositorio_usuario.clone(),
            servico_auditoria_notificacoes.clone(),
        );
        let indicadores = CasoDeUsoIndicadores::novo(
            Arc::new(RepositorioIndicadoresPostgres::novo(pool.clone())),
            repositorio_usuario.clone(),
        );
        let auditoria = CasoDeUsoAuditoria::novo(
            Arc::new(RepositorioAuditoriaPostgres::novo(pool.clone())),
            repositorio_usuario,
            servico_auditoria_notificacoes,
        );

        Arc::new(EstadoAplicacao {
            pool,
            endereco_sqlite,
            autenticacao,
            recuperacao,
            profissionais,
            familias,
            agendamento,
            registro,
            solicitacao_retorno,
            historico,
            notificacoes,
            indicadores,
            auditoria,
            servico_jwt,
        })
    }
}
