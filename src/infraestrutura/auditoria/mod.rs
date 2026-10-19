// =============================================================================
// Servico de auditoria (UC12) — gravacao de trilhas na tabela `auditoria`
// Utilizado pelo middleware de autenticacao para tentativas de acesso.
// =============================================================================

use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

/// Acoes de auditoria padronizadas do UC01.
pub mod acao {
    /// Tentativa de login bem-sucedida.
    pub const LOGIN_SUCESSO: &str = "LOGIN_SUCESSO";
    /// Tentativa de login com falha (credenciais incorretas).
    pub const LOGIN_FALHA: &str = "LOGIN_FALHA";
    /// Acesso bloqueado por conta inativa.
    pub const LOGIN_BLOQUEADO: &str = "LOGIN_BLOQUEADO";
    /// Troca de senha realizada.
    pub const TROCA_SENHA: &str = "TROCA_SENHA";
    /// Novo agendamento de visita registrado (UC04).
    pub const VISITA_AGENDADA: &str = "VISITA_AGENDADA";
    /// Reagendamento de visita, com troca de data/horario/responsavel (RN18).
    pub const VISITA_REAGENDADA: &str = "VISITA_REAGENDADA";
    /// Cancelamento de agendamento de visita (RN18).
    pub const VISITA_CANCELADA: &str = "VISITA_CANCELADA";
    /// Inicio da execucao da visita (rascunho do registro — UC05/RN12).
    pub const REGISTRO_INICIADO: &str = "REGISTRO_INICIADO";
    /// Finalizacao do registro da visita (UC05/RN13/RN14).
    pub const REGISTRO_FINALIZADO: &str = "REGISTRO_FINALIZADO";
    /// Complemento imutavel em registro finalizado (UC05/RN16/RN17).
    pub const COMPLEMENTO_REGISTRADO: &str = "COMPLEMENTO_REGISTRADO";
    /// Solicitacao de retorno criada pelo tecnico (UC08/RN19).
    pub const RETORNO_SOLICITADO: &str = "RETORNO_SOLICITADO";
    /// Aprovacao do retorno com agendamento da nova visita (UC08/RN19).
    pub const RETORNO_AGENDADO: &str = "RETORNO_AGENDADO";
    /// Rejeicao da solicitacao de retorno, com justificativa (UC08/RN19).
    pub const RETORNO_REJEITADO: &str = "RETORNO_REJEITADO";
    /// Cancelamento da solicitacao de retorno pelo solicitante (UC08/RN20).
    pub const RETORNO_CANCELADO: &str = "RETORNO_CANCELADO";
}

/// Servico que registra eventos na tabela de auditoria.
#[derive(Clone)]
pub struct ServicoAuditoria {
    pool: PgPool,
}

impl ServicoAuditoria {
    /// Cria o servico com o pool do banco central.
    pub fn novo(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Registra um evento de auditoria.
    #[allow(clippy::too_many_arguments)]
    pub async fn registrar(
        &self,
        id_usuario: Option<Uuid>,
        entidade: &str,
        entidade_id: Uuid,
        acao: &str,
        descricao: Option<&str>,
        ip_origem: Option<&str>,
        dispositivo: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO auditoria
                (id_usuario, entidade, entidade_id, acao, descricao, ip_origem, dispositivo, data_hora)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(id_usuario)
        .bind(entidade)
        .bind(entidade_id)
        .bind(acao)
        .bind(descricao)
        .bind(ip_origem)
        .bind(dispositivo)
        .bind(Utc::now())
        .execute(&self.pool)
        .await
        .map(|_| ())
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn constantes_de_acao_estao_definidas() {
        assert_eq!(acao::LOGIN_SUCESSO, "LOGIN_SUCESSO");
        assert_eq!(acao::LOGIN_FALHA, "LOGIN_FALHA");
        assert_eq!(acao::LOGIN_BLOQUEADO, "LOGIN_BLOQUEADO");
        assert_eq!(acao::TROCA_SENHA, "TROCA_SENHA");
        assert_eq!(acao::VISITA_AGENDADA, "VISITA_AGENDADA");
        assert_eq!(acao::VISITA_REAGENDADA, "VISITA_REAGENDADA");
        assert_eq!(acao::VISITA_CANCELADA, "VISITA_CANCELADA");
        assert_eq!(acao::REGISTRO_INICIADO, "REGISTRO_INICIADO");
        assert_eq!(acao::REGISTRO_FINALIZADO, "REGISTRO_FINALIZADO");
        assert_eq!(acao::COMPLEMENTO_REGISTRADO, "COMPLEMENTO_REGISTRADO");
        assert_eq!(acao::RETORNO_SOLICITADO, "RETORNO_SOLICITADO");
        assert_eq!(acao::RETORNO_AGENDADO, "RETORNO_AGENDADO");
        assert_eq!(acao::RETORNO_REJEITADO, "RETORNO_REJEITADO");
        assert_eq!(acao::RETORNO_CANCELADO, "RETORNO_CANCELADO");
    }
}
