// =============================================================================
// Entidades RegistroVisita e ComplementoRegistro (UC05)
// Mapeadas para as tabelas `registro_visita` e `complemento_registro` do MER.
// =============================================================================

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::dominio::enums::{ResultadoRegistro, StatusRegistro};

/// Registro da visita domiciliar executada (ou tentada) pelo tecnico.
/// Relacao 1 para 0..1 com `visita` (id_visita UNIQUE no MER).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistroVisita {
    pub id: Uuid,
    pub id_visita: Uuid,
    pub status: StatusRegistro,
    /// Resultado da tentativa de visita (definido na finalizacao).
    pub resultado: Option<ResultadoRegistro>,
    /// Resumo/descricao do que foi constatado (RN13).
    pub descricao: Option<String>,
    /// Desfecho da visita (RN13).
    pub desfecho: Option<String>,
    /// Indica necessidade de retorno a familia (alimenta o UC08).
    pub necessita_retorno: bool,
    /// Motivo obrigatorio quando a visita nao e realizada (RN14).
    pub motivo_nao_realizada: Option<String>,
    pub data_hora_inicio: Option<DateTime<Utc>>,
    /// Horario de termino obrigatorio para visitas realizadas (RN13).
    pub data_hora_fim: Option<DateTime<Utc>>,
    pub registrado_por: Uuid,
    pub criado_em: DateTime<Utc>,
}

impl RegistroVisita {
    /// Valida as regras de finalizacao (RN13 e RN14).
    pub fn finalizacao_e_valida(dados: &DadosRegistro) -> Result<(), String> {
        match dados.resultado {
            Some(ResultadoRegistro::Realizada) => {
                if dados
                    .descricao
                    .as_deref()
                    .map(str::trim)
                    .unwrap_or("")
                    .is_empty()
                {
                    return Err("visita realizada exige descricao/resumo (RN13)".into());
                }
                if dados
                    .desfecho
                    .as_deref()
                    .map(str::trim)
                    .unwrap_or("")
                    .is_empty()
                {
                    return Err("visita realizada exige desfecho (RN13)".into());
                }
                Ok(())
            }
            Some(ResultadoRegistro::NaoRealizada) => {
                if dados
                    .motivo_nao_realizada
                    .as_deref()
                    .map(str::trim)
                    .unwrap_or("")
                    .is_empty()
                {
                    return Err("visita nao realizada exige motivo obrigatorio (RN14)".into());
                }
                Ok(())
            }
            None => Err("defina o resultado da visita para finalizar".into()),
        }
    }
}

/// Dados editaveis do registro (rascunho ou finalizacao).
#[derive(Debug, Clone, Default)]
pub struct DadosRegistro {
    pub resultado: Option<ResultadoRegistro>,
    pub descricao: Option<String>,
    pub desfecho: Option<String>,
    pub necessita_retorno: bool,
    pub motivo_nao_realizada: Option<String>,
}

/// Complemento imutavel de um registro finalizado (RN16/RN17).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComplementoRegistro {
    pub id: Uuid,
    pub id_registro: Uuid,
    pub descricao: String,
    pub motivo: String,
    pub registrado_por: Uuid,
    pub criado_em: DateTime<Utc>,
}

/// Dados de entrada de um complemento (RN17: autor e justificativa).
#[derive(Debug, Clone)]
pub struct NovoComplemento {
    pub descricao: String,
    pub motivo: String,
}

#[cfg(test)]
mod testes {
    use super::*;

    fn dados_realizada() -> DadosRegistro {
        DadosRegistro {
            resultado: Some(ResultadoRegistro::Realizada),
            descricao: Some("Familia recebida em casa, todos bem.".into()),
            desfecho: Some("Acompanhamento mensal mantido".into()),
            necessita_retorno: false,
            motivo_nao_realizada: None,
        }
    }

    #[test]
    fn finalizacao_realizada_valida() {
        assert!(RegistroVisita::finalizacao_e_valida(&dados_realizada()).is_ok());
    }

    #[test]
    fn realizada_sem_descricao_e_rejeitada() {
        let mut dados = dados_realizada();
        dados.descricao = Some("   ".into());
        let erro = RegistroVisita::finalizacao_e_valida(&dados).expect_err("RN13");
        assert!(erro.contains("RN13"));
    }

    #[test]
    fn realizada_sem_desfecho_e_rejeitada() {
        let mut dados = dados_realizada();
        dados.desfecho = None;
        assert!(RegistroVisita::finalizacao_e_valida(&dados).is_err());
    }

    #[test]
    fn nao_realizada_sem_motivo_e_rejeitada() {
        let dados = DadosRegistro {
            resultado: Some(ResultadoRegistro::NaoRealizada),
            descricao: None,
            desfecho: None,
            necessita_retorno: false,
            motivo_nao_realizada: None,
        };
        let erro = RegistroVisita::finalizacao_e_valida(&dados).expect_err("RN14");
        assert!(erro.contains("RN14"));
    }

    #[test]
    fn nao_realizada_com_motivo_e_valida() {
        let dados = DadosRegistro {
            resultado: Some(ResultadoRegistro::NaoRealizada),
            descricao: None,
            desfecho: None,
            necessita_retorno: true,
            motivo_nao_realizada: Some("Familia ausente em 3 tentativas".into()),
        };
        assert!(RegistroVisita::finalizacao_e_valida(&dados).is_ok());
    }

    #[test]
    fn finalizar_sem_resultado_e_rejeitado() {
        let dados = DadosRegistro::default();
        assert!(RegistroVisita::finalizacao_e_valida(&dados).is_err());
    }
}
