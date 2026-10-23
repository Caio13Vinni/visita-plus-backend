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
//   - O app mobile tambem sincroniza via HTTP (POST /push com as
//     pendencias e GET /pull para baixar a agenda), com as mesmas regras;
//   - Recebimento (pull): a agenda do perfil e refletida no banco local —
//     tecnico recebe suas visitas; coordenador, as da propria unidade
//     (RN05/RN06) — com as familias correspondentes;
//   - Pendencias invalidas sao marcadas com erro na fila e nao impedem
//     o restante da sincronizacao;
//   - Auditoria da operacao (RN22).
// =============================================================================

use std::collections::BTreeSet;
use std::sync::Arc;
use uuid::Uuid;

use serde::Deserialize;

use crate::dominio::entidades::encaminhamento::Encaminhamento;
use crate::dominio::entidades::familia::Familia;
use crate::dominio::entidades::usuario::Usuario;
use crate::dominio::entidades::visita::Visita;
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

/// Pendencia enviada pelo app mobile no push HTTP.
#[derive(Debug, Deserialize)]
pub struct PendenciaRecebida {
    pub tipo: String,
    pub payload: serde_json::Value,
}

/// Resultado individual de uma pendencia do push HTTP.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultadoItemPush {
    pub tipo: String,
    pub sucesso: bool,
    pub erro: Option<String>,
}

/// Resumo do push de pendencias recebido via HTTP.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResumoPush {
    pub enviados: u64,
    pub erros: u64,
    pub itens: Vec<ResultadoItemPush>,
}

/// Agenda do perfil devolvida no pull HTTP.
#[derive(Debug, Clone)]
pub struct AgendaSincronizacao {
    pub visitas: Vec<Visita>,
    pub familias: Vec<Familia>,
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
            let resultado = match serde_json::from_str::<serde_json::Value>(&pendencia.payload) {
                Ok(payload) => {
                    self.enviar_pendencia(&logado, &pendencia.tipo, &payload)
                        .await
                }
                Err(erro) => Err(ErroSincronizacao::PendenciaInvalida(erro.to_string())),
            };
            match resultado {
                Ok(()) => {
                    self.banco_local.marcar_sincronizado(pendencia.id).await?;
                    enviados += 1;
                }
                Err(ErroSincronizacao::VisitaInvalida)
                | Err(ErroSincronizacao::PendenciaInvalida(_)) => {
                    self.banco_local
                        .marcar_erro(pendencia.id, &pendencia.tipo)
                        .await?;
                    erros += 1;
                }
                Err(erro) => return Err(erro),
            }
        }

        // 2. Recebimento da agenda conforme o perfil (RN05/RN06)
        let (visitas, familias) = self.agenda_do_perfil(&logado).await?;
        let recebidos = visitas.len() as u64;

        let familias_locais = familias
            .into_iter()
            .map(|familia| FamiliaLocal {
                id: familia.id,
                codigo: familia.codigo,
                nome_referencia: familia.nome_referencia,
                endereco: familia.endereco,
                status: familia.status,
            })
            .collect::<Vec<_>>();
        let visitas_locais = visitas
            .iter()
            .map(|visita| VisitaLocal {
                id: visita.id,
                id_familia: visita.id_familia,
                data_visita: visita.data_visita,
                horario: visita.horario,
                prioridade: visita.prioridade,
                objetivo: visita.objetivo.clone(),
                status: visita.status,
            })
            .collect::<Vec<_>>();

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
        payload: &serde_json::Value,
    ) -> Result<(), ErroSincronizacao> {
        match tipo {
            PENDENCIA_REGISTRO_VISITA => {
                let dados: PendenciaRegistroVisita = serde_json::from_value(payload.clone())
                    .map_err(|erro| ErroSincronizacao::PendenciaInvalida(erro.to_string()))?;
                self.enviar_registro_visita(logado, &dados).await
            }
            PENDENCIA_ENCAMINHAMENTO => {
                let dados: PendenciaEncaminhamento = serde_json::from_value(payload.clone())
                    .map_err(|erro| ErroSincronizacao::PendenciaInvalida(erro.to_string()))?;
                self.enviar_encaminhamento(logado, &dados).await
            }
            outro => Err(ErroSincronizacao::PendenciaInvalida(format!(
                "tipo de pendencia desconhecido: {outro}"
            ))),
        }
    }

    /// Aplica pendencias enviadas pelo app mobile (push HTTP): mesmas
    /// validacoes da fila local; itens invalidos viram erro individual sem
    /// abortar o lote; operacao auditada (RN22).
    pub async fn receber_pendencias(
        &self,
        id_usuario: Uuid,
        pendencias: &[PendenciaRecebida],
    ) -> Result<ResumoPush, ErroSincronizacao> {
        let logado = self.usuario_ativo(id_usuario).await?;

        let mut resumo = ResumoPush::default();
        for pendencia in pendencias {
            let resultado = self
                .enviar_pendencia(&logado, &pendencia.tipo, &pendencia.payload)
                .await;
            match resultado {
                Ok(()) => {
                    resumo.enviados += 1;
                    resumo.itens.push(ResultadoItemPush {
                        tipo: pendencia.tipo.clone(),
                        sucesso: true,
                        erro: None,
                    });
                }
                Err(erro @ ErroSincronizacao::VisitaInvalida)
                | Err(erro @ ErroSincronizacao::PendenciaInvalida(_)) => {
                    resumo.erros += 1;
                    resumo.itens.push(ResultadoItemPush {
                        tipo: pendencia.tipo.clone(),
                        sucesso: false,
                        erro: Some(erro.to_string()),
                    });
                }
                Err(erro) => return Err(erro),
            }
        }

        let _ = self
            .auditoria
            .registrar(
                Some(logado.id),
                "sincronizacao",
                logado.id,
                acao::SINCRONIZACAO_REALIZADA,
                Some(&format!(
                    "push recebido: {} enviados, {} erros",
                    resumo.enviados, resumo.erros
                )),
                None,
                None,
            )
            .await;

        Ok(resumo)
    }

    /// Devolve a agenda do perfil para o app mobile baixar (pull HTTP):
    /// o tecnico recebe as proprias visitas; o coordenador, as da propria
    /// unidade (RN05/RN06), com as familias correspondentes. Operacao
    /// auditada (RN22).
    pub async fn baixar_agenda(
        &self,
        id_usuario: Uuid,
    ) -> Result<AgendaSincronizacao, ErroSincronizacao> {
        let logado = self.usuario_ativo(id_usuario).await?;
        let (visitas, familias) = self.agenda_do_perfil(&logado).await?;

        let _ = self
            .auditoria
            .registrar(
                Some(logado.id),
                "sincronizacao",
                logado.id,
                acao::SINCRONIZACAO_REALIZADA,
                Some(&format!(
                    "pull executado: {} visitas recebidas",
                    visitas.len()
                )),
                None,
                None,
            )
            .await;

        Ok(AgendaSincronizacao { visitas, familias })
    }

    /// Agenda do perfil conforme RN05/RN06, com as familias correspondentes
    /// (sem duplicidade).
    async fn agenda_do_perfil(
        &self,
        logado: &Usuario,
    ) -> Result<(Vec<Visita>, Vec<Familia>), ErroSincronizacao> {
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

        let mut familias = Vec::new();
        for id_familia in visitas
            .iter()
            .map(|visita| visita.id_familia)
            .collect::<BTreeSet<_>>()
        {
            if let Some(familia) = self.repositorio_familia.buscar_por_id(id_familia).await? {
                familias.push(familia);
            }
        }

        Ok((visitas, familias))
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

        // Fecha a visita no central com o mesmo status do fluxo online
        // (UC05), para que indicadores e historico nao fiquem divergentes.
        let novo_status_visita = match dados.resultado {
            ResultadoRegistro::Realizada => StatusVisita::Realizada,
            ResultadoRegistro::NaoRealizada => StatusVisita::NaoRealizada,
        };
        self.repositorio_visita
            .atualizar_status(visita.id, novo_status_visita)
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
