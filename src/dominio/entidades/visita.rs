// =============================================================================
// Entidade Visita — agendamento domiciliar (UC04)
// Mapeada para a tabela `visita` do MER.
// =============================================================================

use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use uuid::Uuid;

use crate::dominio::enums::{PrioridadeVisita, StatusVisita};

/// Representa um agendamento de visita domiciliar na agenda de um tecnico.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Visita {
    pub id: Uuid,
    pub id_familia: Uuid,
    pub id_tecnico: Uuid,
    /// Profissional (coordenador ou tecnico) que registrou o agendamento.
    pub id_agendado_por: Uuid,
    /// Solicitacao de retorno que originou a visita (quando houver — RN19).
    pub id_origem_retorno: Option<Uuid>,
    pub data_visita: NaiveDate,
    pub horario: NaiveTime,
    pub prioridade: PrioridadeVisita,
    pub objetivo: String,
    pub status: StatusVisita,
    pub motivo_cancelamento: Option<String>,
    pub criado_em: DateTime<Utc>,
}

impl Visita {
    /// Valida os dados minimos de um agendamento.
    pub fn dados_sao_validos(cadastro: &CadastroVisita) -> bool {
        !cadastro.objetivo.trim().is_empty()
    }

    /// Verifica se a data e o horario estao estritamente no futuro (RN09).
    pub fn data_horario_sao_futuros(data: NaiveDate, horario: NaiveTime) -> bool {
        let agora = Utc::now();
        let hoje = agora.date_naive();
        let agora_horario = agora.time();

        data > hoje || (data == hoje && horario > agora_horario)
    }
}

/// Dados de entrada para agendar uma visita (UC04).
#[derive(Debug, Clone)]
pub struct CadastroVisita {
    pub id_familia: Uuid,
    pub id_tecnico: Uuid,
    pub data_visita: NaiveDate,
    pub horario: NaiveTime,
    pub prioridade: PrioridadeVisita,
    pub objetivo: String,
}

/// Dados de entrada para reagendar uma visita (UC04 / RN18).
#[derive(Debug, Clone)]
pub struct ReagendamentoVisita {
    pub data_visita: NaiveDate,
    pub horario: NaiveTime,
    /// Nova responsabilidade pela visita; `None` mantem o tecnico atual.
    pub id_tecnico: Option<Uuid>,
}

#[cfg(test)]
mod testes {
    use super::*;
    use chrono::Duration;

    fn cadastro_exemplo() -> CadastroVisita {
        CadastroVisita {
            id_familia: Uuid::new_v4(),
            id_tecnico: Uuid::new_v4(),
            data_visita: Utc::now().date_naive() + Duration::days(3),
            horario: NaiveTime::from_hms_opt(9, 0, 0).expect("horario valido"),
            prioridade: PrioridadeVisita::Normal,
            objetivo: "Acompanhamento familiar mensal".into(),
        }
    }

    #[test]
    fn cadastro_visita_valido() {
        assert!(Visita::dados_sao_validos(&cadastro_exemplo()));
    }

    #[test]
    fn cadastro_visita_sem_objetivo_e_invalido() {
        let mut cadastro = cadastro_exemplo();
        cadastro.objetivo = "   ".into();
        assert!(!Visita::dados_sao_validos(&cadastro));
    }

    #[test]
    fn data_futura_e_aceita() {
        let amanha = Utc::now().date_naive() + Duration::days(1);
        let horario = NaiveTime::from_hms_opt(8, 0, 0).expect("horario valido");
        assert!(Visita::data_horario_sao_futuros(amanha, horario));
    }

    #[test]
    fn data_retroativa_e_rejeitada() {
        let ontem = Utc::now().date_naive() - Duration::days(1);
        let horario = NaiveTime::from_hms_opt(8, 0, 0).expect("horario valido");
        assert!(!Visita::data_horario_sao_futuros(ontem, horario));
    }

    #[test]
    fn horario_passado_no_mesmo_dia_e_rejeitado() {
        let passado = Utc::now() - Duration::hours(2);
        let data = passado.date_naive();
        let horario = passado.time();
        assert!(!Visita::data_horario_sao_futuros(data, horario));
    }

    #[test]
    fn horario_futuro_no_mesmo_dia_e_aceito() {
        let futuro = Utc::now() + Duration::hours(2);
        let data = futuro.date_naive();
        let horario = futuro.time();
        assert!(Visita::data_horario_sao_futuros(data, horario));
    }
}
