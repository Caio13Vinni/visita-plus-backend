// =============================================================================
// Sincronizacao offline/online — reconcilia o banco local (SQLite, simula o
// aplicativo mobile) com o banco central (PostgreSQL).
// -----------------------------------------------------------------------------
// Regras aplicadas:
//   - Envio (push): pendencias da fila local ('registro_visita',
//     'encaminhamento') sao aplicadas ao banco central com as mesmas
//     validacoes de posse: a visita precisa existir e pertencer ao
//     tecnico autenticado; registros ja existentes sao tratados como
//     idempotentes (reenvio nao duplica);
//   - Recebimento (pull): a agenda do perfil e refletida no banco local —
//     tecnico recebe suas visitas; coordenador, as da propria unidade
//     (RN05/RN06) — com as familias correspondentes;
//   - Pendencias invalidas sao marcadas com erro na fila e nao impedem
//     o restante da sincronizacao;
//   - Auditoria da operacao (RN22).
// =============================================================================

use std::sync::Arc;
use uuid::Uuid;

use serde::Deserialize;

use crate::dominio::entidades::encaminhamento::Encaminhamento;
use crate::dominio::entidades::usuario::Usuario;
use crate::dominio::enums::{PerfilUsuario, ResultadoRegistro, StatusEncaminhamento, StatusVisita};
use crate::dominio::erros::ErroSincronizacao;
use crate::infraestrutura::auditoria::{ServicoAuditoria, acao};
use crate::infraestrutura::repositorios::encaminhamento_repositorio::RepositorioEncaminhamento;
use crate::infraestrutura::repositorios::familia_repositorio::RepositorioFamilia;
use crate::infraestrutura::repositorios::registro_repositorio::RepositorioRegistro;
use crate::infraestrutura::repositorios::usuario_repositorio::RepositorioUsuario;
use crate::infraestrutura::repositorios::visita_repositorio::{FiltroVisita, RepositorioVisita};
use crate::infraestrutura::sync::{BancoLocal, FamiliaLocal, VisitaLocal};

/// Tipo de pendencia de envio: registro de visita feito offline.
pub const PENDENCIA_REGISTRO_VISITA: &str = "registro_visita";
/// Tipo de pendencia de envio: encaminhamento definido offline.
pub const PENDENCIA_ENCAMINHAMENTO: &str = "encaminhamento";

/// Payload da pendencia de registro de visita (JSON).
#[derive(Debug, Deserialize)]
pub struct PendenciaRegistroVisita {
    pub id_visita: Uuid,
    pub resultado: ResultadoRegistro,
    pub descricao: Option<String>,
    pub desfecho: Option<String>,
    pub necessita_retorno: bool,
    pub motivo_nao_realizada: Option<String>,
}

/// Payload da pendencia de encaminhamento (JSON).
#[derive(Debug, Deserialize)]
pub struct PendenciaEncaminhamento {
    pub id_visita: Uuid,
    pub destino: String,
    pub motivo: String,
}

/// Resumo da sincronizacao executada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumoSincronizacao {
    /// Pendencias enviadas com sucesso ao banco central.
    pub enviados: u64,
    /// Registros recebidos no espelho local (visitas).
    pub recebidos: u64,
    /// Pendencias marcadas com erro (visita invalida, payload malformado).
    pub erros: u64,
}

/// Orquestrador da sincronizacao offline/online.
#[derive(Clone)]
pub struct CasoDeUsoSincronizacao {
    banco_local: BancoLocal,
    repositorio_visita: Arc<dyn RepositorioVisita>,
    repositorio_familia: Arc<dyn RepositorioFamilia>,
    repositorio_registro: Arc<dyn RepositorioRegistro>,
    repositorio_encaminhamento: Arc<dyn RepositorioEncaminhamento>,
    repositorio_usuario: Arc<dyn RepositorioUsuario>,
    auditoria: ServicoAuditoria,
}

impl CasoDeUsoSincronizacao {
    /// Cria o caso de uso com suas dependencias.
    pub fn novo(
        banco_local: BancoLocal,
        repositorio_visita: Arc<dyn RepositorioVisita>,
        repositorio_familia: Arc<dyn RepositorioFamilia>,
        repositorio_registro: Arc<dyn RepositorioRegistro>,
        repositorio_encaminhamento: Arc<dyn RepositorioEncaminhamento>,
        repositorio_usuario: Arc<dyn RepositorioUsuario>,
        auditoria: ServicoAuditoria,
    ) -> Self {
        Self {
            banco_local,
            repositorio_visita,
            repositorio_familia,
            repositorio_registro,
            repositorio_encaminhamento,
            repositorio_usuario,
            auditoria,
        }
    }

    /// Executa a sincronizacao: envia pendencias locais e recebe a agenda
    /// do perfil no banco local.
    pub async fn sincronizar(
        &self,
        id_usuario: Uuid,
    ) -> Result<ResumoSincronizacao, ErroSincronizacao> {
        let logado = self.usuario_ativo(id_usuario).await?;
        self.banco_local.garantir_esquema().await?;

        // 1. Envio das pendencias acumuladas offline
        let mut enviados = 0u64;
        let mut erros = 0u64;
        for pendencia in self.banco_local.listar_pendentes().await? {
            match self.enviar_pendencia(&logado, &pendencia.tipo, &pendencia.payload).await {
                Ok(()) => {
                    self.banco_local.marcar_sincronizado(pendencia.id).await?;
                    enviados += 1;
                }
                Err(ErroSincronizacao::VisitaInvalida) | Err(ErroSincronizacao::PendenciaInvalida(_)) => {
                    self.banco_local
                        .marcar_erro(pendencia.id, &pendencia.tipo)
                        .await?;
                    erros += 1;
                }
                Err(erro) => return Err(erro),
            }
        }

        // 2. Recebimento da agenda conforme o perfil (RN05/RN06)
        let filtro = match logado.perfil {
            PerfilUsuario::Tecnico => FiltroVisita {
                id_tecnico: Some(logado.id),
                ..Default::default()
            },
            PerfilUsuario::Coordenador => FiltroVisita {
                id_unidade: Some(logado.id_unidade),
                ..Default::default()
            },
        };
        let visitas = self.repositorio_visita.listar(&filtro).await?;
        let recebidos = visitas.len() as u64;

        let mut familias_locais = Vec::new();
        let mut visitas_locais = Vec::new();
        for visita in &visitas {
            if let Some(familia) = self.repositorio_familia.buscar_por_id(visita.id_familia).await? {
                familias_locais.push(FamiliaLocal {
                    id: familia.id,
                    codigo: familia.codigo,
                    nome_referencia: familia.nome_referencia,
                    endereco: familia.endereco,
                    status: familia.status,
                });
            }
            visitas_locais.push(VisitaLocal {
                id: visita.id,
                id_familia: visita.id_familia,
                data_visita: visita.data_visita,
                horario: visita.horario,
                prioridade: visita.prioridade,
                objetivo: visita.objetivo.clone(),
                status: visita.status,
            });
        }

        self.banco_local
            .substituir_agenda(&familias_locais, &visitas_locais)
            .await?;
        self.banco_local.registrar_ultima_sincronizacao().await?;

        // 3. Auditoria da operacao (RN22)
        let _ = self
            .auditoria
            .registrar(
                Some(logado.id),
                "sincronizacao",
                logado.id,
                acao::SINCRONIZACAO_REALIZADA,
                Some(&format!(
                    "sincronizacao executada: {} enviados, {} recebidos, {} erros",
                    enviados, recebidos, erros
                )),
                None,
                None,
            )
            .await;

        Ok(ResumoSincronizacao {
            enviados,
            recebidos,
            erros,
        })
    }

    /// Aplica uma pendencia ao banco central, com as validacoes de
    /// negocio de posse (visita do proprio tecnico).
    async fn enviar_pendencia(
        &self,
        logado: &Usuario,
        tipo: &str,
        payload: &str,
    ) -> Result<(), ErroSincronizacao> {
        match tipo {
            PENDENCIA_REGISTRO_VISITA => {
                let dados: PendenciaRegistroVisita = serde_json::from_str(payload).map_err(|erro| {
                    ErroSincronizacao::PendenciaInvalida(erro.to_string())
                })?;
                self.enviar_registro_visita(logado, &dados).await
            }
            PENDENCIA_ENCAMINHAMENTO => {
                let dados: PendenciaEncaminhamento = serde_json::from_str(payload).map_err(|erro| {
                    ErroSincronizacao::PendenciaInvalida(erro.to_string())
                })?;
                self.enviar_encaminhamento(logado, &dados).await
            }
            outro => Err(ErroSincronizacao::PendenciaInvalida(format!(
                "tipo de pendencia desconhecido: {outro}"
            ))),
        }
    }

    /// Envia um registro de visita feito offline: exige visita da propria
    /// agenda; registros ja existentes sao idempotentes (reenvio nao
    /// duplica, regra 1 para 0..1 do MER).
    async fn enviar_registro_visita(
        &self,
        logado: &Usuario,
        dados: &PendenciaRegistroVisita,
    ) -> Result<(), ErroSincronizacao> {
        let visita = self
            .repositorio_visita
            .buscar_por_id(dados.id_visita)
            .await?
            .ok_or(ErroSincronizacao::VisitaInvalida)?;

        if visita.id_tecnico != logado.id {
            return Err(ErroSincronizacao::VisitaInvalida);
        }

        if self
            .repositorio_registro
            .buscar_por_visita(visita.id)
            .await?
            .is_some()
        {
            return Ok(());
        }

        // Visita ainda nao iniciada no central: abre em andamento (RN11)
        // antes de finalizar o registro feito offline.
        if visita.status == StatusVisita::Agendada {
            self.repositorio_visita
                .atualizar_status(visita.id, StatusVisita::EmAndamento)
                .await?;
        }

        let registro = self
            .repositorio_registro
            .criar(visita.id, logado.id)
            .await?;

        self.repositorio_registro
            .finalizar(
                registro.id,
                dados.resultado,
                dados.descricao.as_deref(),
                dados.desfecho.as_deref(),
                dados.necessita_retorno,
                dados.motivo_nao_realizada.as_deref(),
                chrono::Utc::now(),
            )
            .await?;

        Ok(())
    }

    /// Envia um encaminhamento definido offline: exige visita da propria
    /// agenda e destino/motivo preenchidos (RF17).
    async fn enviar_encaminhamento(
        &self,
        logado: &Usuario,
        dados: &PendenciaEncaminhamento,
    ) -> Result<(), ErroSincronizacao> {
        if dados.destino.trim().is_empty() || dados.motivo.trim().is_empty() {
            return Err(ErroSincronizacao::PendenciaInvalida(
                "destino e motivo do encaminhamento sao obrigatorios".into(),
            ));
        }

        let visita = self
            .repositorio_visita
            .buscar_por_id(dados.id_visita)
            .await?
            .ok_or(ErroSincronizacao::VisitaInvalida)?;

        if visita.id_tecnico != logado.id {
            return Err(ErroSincronizacao::VisitaInvalida);
        }

        let encaminhamento = Encaminhamento {
            id: Uuid::new_v4(),
            id_visita: visita.id,
            destino: dados.destino.trim().to_string(),
            motivo: dados.motivo.trim().to_string(),
            data_encaminhamento: chrono::Utc::now(),
            status: StatusEncaminhamento::Aberto,
            registrado_por: logado.id,
            criado_em: chrono::Utc::now(),
        };

        self.repositorio_encaminhamento
            .criar(&encaminhamento)
            .await?;

        Ok(())
    }

    /// Carrega o usuario autenticado e exige conta ativa.
    async fn usuario_ativo(&self, id: Uuid) -> Result<Usuario, ErroSincronizacao> {
        let usuario = self
            .repositorio_usuario
            .buscar_por_id(id)
            .await?
            .ok_or(ErroSincronizacao::NaoEncontrado)?;

        if !usuario.esta_ativo() {
            return Err(ErroSincronizacao::SemPermissao);
        }
        Ok(usuario)
    }
}
