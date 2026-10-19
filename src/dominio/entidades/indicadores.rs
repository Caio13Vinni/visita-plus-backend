// =============================================================================
// Modelo de leitura dos indicadores operacionais (UC11)
// Quantidades agregadas, sem dados sensiveis (RN23).
// =============================================================================

use chrono::NaiveDate;

/// Quantidades operacionais da unidade no periodo consultado.
///
/// Contagem anonima de visitas por status e de retornos pendentes — nenhum
/// dado identificavel de familias ou usuarios e exposto (RN23).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndicadoresOperacionais {
    pub periodo_inicio: NaiveDate,
    pub periodo_fim: NaiveDate,
    pub visitas_agendadas: i64,
    pub visitas_em_andamento: i64,
    pub visitas_realizadas: i64,
    pub visitas_nao_realizadas: i64,
    pub visitas_canceladas: i64,
    pub retornos_pendentes: i64,
}

impl IndicadoresOperacionais {
    /// Total de visitas do periodo, independentemente do status.
    pub fn total_visitas(&self) -> i64 {
        self.visitas_agendadas
            + self.visitas_em_andamento
            + self.visitas_realizadas
            + self.visitas_nao_realizadas
            + self.visitas_canceladas
    }

    /// Indica se o periodo nao possui nenhum dado (para a mensagem
    /// explicativa prevista no fluxo do UC11).
    pub fn periodo_sem_dados(&self) -> bool {
        self.total_visitas() == 0 && self.retornos_pendentes == 0
    }
}
