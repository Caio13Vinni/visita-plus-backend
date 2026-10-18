// =============================================================================
// UC05 — Registrar visita
// -----------------------------------------------------------------------------
// Regras aplicadas:
//   - Somente o tecnico responsavel executa e registra a visita (RN05/RN06);
//   - Rascunhos permitidos: a visita pode ser iniciada e editada antes do
//     fechamento (RN12);
//   - Visitas realizadas exigem descricao, desfecho e horario de termino
//     (RN13); visitas nao realizadas exigem motivo obrigatorio (RN14);
//   - Registros finalizados sao imutaveis: correcoes exigem complemento com
//     autor e justificativa (RN16/RN17);
//   - Ao finalizar, o status da visita reflete o resultado (Realizada /
//     Nao realizada).
// =============================================================================

use std::sync::Arc;
use uuid::Uuid;

use crate::dominio::entidades::registro_visita::{
    ComplementoRegistro, DadosRegistro, NovoComplemento, RegistroVisita,
};
use crate::dominio::entidades::usuario::Usuario;
use crate::dominio::entidades::visita::Visita;
use crate::dominio::enums::{PerfilUsuario, ResultadoRegistro, StatusRegistro, StatusVisita};
use crate::dominio::erros::ErroRegistro;
use crate::infraestrutura::auditoria::{ServicoAuditoria, acao};
use crate::infraestrutura::repositorios::familia_repositorio::RepositorioFamilia;
use crate::infraestrutura::repositorios::registro_repositorio::RepositorioRegistro;
use crate::infraestrutura::repositorios::usuario_repositorio::RepositorioUsuario;
use crate::infraestrutura::repositorios::visita_repositorio::RepositorioVisita;

/// Orquestrador do caso de uso de registro da visita.
#[derive(Clone)]
pub struct CasoDeUsoRegistro {
    repositorio_registro: Arc<dyn RepositorioRegistro>,
    repositorio_visita: Arc<dyn RepositorioVisita>,
    repositorio_familia: Arc<dyn RepositorioFamilia>,
    repositorio_usuario: Arc<dyn RepositorioUsuario>,
    auditoria: ServicoAuditoria,
}

impl CasoDeUsoRegistro {
    /// Cria o caso de uso com suas dependencias.
    pub fn novo(
        repositorio_registro: Arc<dyn RepositorioRegistro>,
        repositorio_visita: Arc<dyn RepositorioVisita>,
        repositorio_familia: Arc<dyn RepositorioFamilia>,
        repositorio_usuario: Arc<dyn RepositorioUsuario>,
        auditoria: ServicoAuditoria,
    ) -> Self {
        Self {
            repositorio_registro,
            repositorio_visita,
            repositorio_familia,
            repositorio_usuario,
            auditoria,
        }
    }

    /// Inicia a execucao da visita, criando o registro em rascunho (RN12).
    ///
    /// A visita passa de 'Agendada' para 'Em andamento'. Somente o tecnico
    /// responsavel pela agenda pode executa-la (RN05/RN06).
    pub async fn iniciar_visita(
        &self,
        id_usuario: Uuid,
        id_visita: Uuid,
    ) -> Result<RegistroVisita, ErroRegistro> {
        let logado = self.usuario_ativo(id_usuario).await?;
        let visita = self.visita_existente(id_visita).await?;

        if visita.status != StatusVisita::Agendada {
            return Err(ErroRegistro::VisitaNaoAgendada);
        }
        self.exigir_tecnico_responsavel(&logado, &visita)?;

        self.repositorio_visita
            .atualizar_status(id_visita, StatusVisita::EmAndamento)
            .await?;

        let registro = match self.repositorio_registro.criar(id_visita, logado.id).await {
            Ok(registro) => registro,
            Err(erro) => {
                // Melhor esforco: sem registro rascunho, a visita nao pode
                // ficar 'Em andamento' (MER exige o par visita/registro).
                let _ = self
                    .repositorio_visita
                    .atualizar_status(id_visita, StatusVisita::Agendada)
                    .await;
                return Err(ErroRegistro::Tecnica(erro.to_string()));
            }
        };

        let _ = self
            .auditoria
            .registrar(
                Some(logado.id),
                "registro_visita",
                registro.id,
                acao::REGISTRO_INICIADO,
                Some(&format!("Visita {} iniciada em rascunho", visita.id)),
                None,
                None,
            )
            .await;

        Ok(registro)
    }

    /// Salva o rascunho do registro sem finalizar (RN12).
    pub async fn salvar_rascunho(
        &self,
        id_usuario: Uuid,
        id_registro: Uuid,
        dados: &DadosRegistro,
    ) -> Result<RegistroVisita, ErroRegistro> {
        let logado = self.usuario_ativo(id_usuario).await?;
        let registro = self.registro_existente(id_registro).await?;
        self.exigir_rascunho_editavel(&logado, &registro)?;

        self.repositorio_registro
            .salvar_rascunho(
                id_registro,
                dados.resultado,
                dados.descricao.as_deref(),
                dados.desfecho.as_deref(),
                dados.necessita_retorno,
                dados.motivo_nao_realizada.as_deref(),
            )
            .await?;

        self.repositorio_registro
            .buscar_por_id(id_registro)
            .await?
            .ok_or(ErroRegistro::NaoEncontrado)
    }

    /// Finaliza o registro da visita (RN13/RN14) e reflete o resultado no
    /// status da visita (Realizada / Nao realizada).
    pub async fn finalizar_registro(
        &self,
        id_usuario: Uuid,
        id_registro: Uuid,
        dados: &DadosRegistro,
    ) -> Result<RegistroVisita, ErroRegistro> {
        let logado = self.usuario_ativo(id_usuario).await?;
        let registro = self.registro_existente(id_registro).await?;
        self.exigir_rascunho_editavel(&logado, &registro)?;

        if let Err(mensagem) = RegistroVisita::finalizacao_e_valida(dados) {
            return Err(ErroRegistro::DadosInvalidos(mensagem));
        }
        let resultado = dados.resultado.expect("validado acima");

        let data_hora_fim = chrono::Utc::now();
        self.repositorio_registro
            .finalizar(
                id_registro,
                resultado,
                dados.descricao.as_deref(),
                dados.desfecho.as_deref(),
                dados.necessita_retorno,
                dados.motivo_nao_realizada.as_deref(),
                data_hora_fim,
            )
            .await?;

        let novo_status_visita = match resultado {
            ResultadoRegistro::Realizada => StatusVisita::Realizada,
            ResultadoRegistro::NaoRealizada => StatusVisita::NaoRealizada,
        };
        self.repositorio_visita
            .atualizar_status(registro.id_visita, novo_status_visita)
            .await?;

        let _ = self
            .auditoria
            .registrar(
                Some(logado.id),
                "registro_visita",
                id_registro,
                acao::REGISTRO_FINALIZADO,
                Some(&format!(
                    "Registro finalizado com resultado {} em {}",
                    resultado.como_str(),
                    data_hora_fim
                )),
                None,
                None,
            )
            .await;

        self.repositorio_registro
            .buscar_por_id(id_registro)
            .await?
            .ok_or(ErroRegistro::NaoEncontrado)
    }

    /// Acrescenta complemento a um registro finalizado (RN16/RN17).
    ///
    /// Registros finalizados sao imutaveis: a correcao e feita por um novo
    /// complemento com autor e justificativa. Permite o tecnico responsavel
    /// ou um coordenador da mesma unidade.
    pub async fn adicionar_complemento(
        &self,
        id_usuario: Uuid,
        id_registro: Uuid,
        complemento: &NovoComplemento,
    ) -> Result<ComplementoRegistro, ErroRegistro> {
        let logado = self.usuario_ativo(id_usuario).await?;
        let registro = self.registro_existente(id_registro).await?;

        if registro.status != StatusRegistro::Finalizado {
            return Err(ErroRegistro::RegistroNaoFinalizado);
        }

        let visita = self.visita_existente(registro.id_visita).await?;
        if !self.pode_supervisionar(&logado, &visita).await {
            return Err(ErroRegistro::SemPermissao);
        }

        if complemento.descricao.trim().is_empty() || complemento.motivo.trim().is_empty() {
            return Err(ErroRegistro::DadosInvalidos(
                "complemento exige descricao e motivo (RN17)".into(),
            ));
        }

        let criado = self
            .repositorio_registro
            .criar_complemento(
                id_registro,
                complemento.descricao.trim(),
                complemento.motivo.trim(),
                logado.id,
            )
            .await?;

        let _ = self
            .auditoria
            .registrar(
                Some(logado.id),
                "complemento_registro",
                criado.id,
                acao::COMPLEMENTO_REGISTRADO,
                Some(&format!("Complemento ao registro {id_registro}")),
                None,
                None,
            )
            .await;

        Ok(criado)
    }

    /// Detalha o registro de uma visita, respeitando a visibilidade (RN05).
    pub async fn detalhar_por_visita(
        &self,
        id_usuario: Uuid,
        id_visita: Uuid,
    ) -> Result<RegistroVisita, ErroRegistro> {
        let logado = self.usuario_ativo(id_usuario).await?;
        let visita = self.visita_existente(id_visita).await?;

        if !self.pode_supervisionar(&logado, &visita).await {
            return Err(ErroRegistro::NaoEncontrado);
        }

        self.repositorio_registro
            .buscar_por_visita(id_visita)
            .await?
            .ok_or(ErroRegistro::NaoEncontrado)
    }

    /// Lista os complementos de um registro finalizado (RN16/RN17).
    pub async fn listar_complementos(
        &self,
        id_usuario: Uuid,
        id_registro: Uuid,
    ) -> Result<Vec<ComplementoRegistro>, ErroRegistro> {
        let logado = self.usuario_ativo(id_usuario).await?;
        let registro = self.registro_existente(id_registro).await?;
        let visita = self.visita_existente(registro.id_visita).await?;

        if !self.pode_supervisionar(&logado, &visita).await {
            return Err(ErroRegistro::NaoEncontrado);
        }

        Ok(self
            .repositorio_registro
            .listar_complementos(id_registro)
            .await?)
    }

    /// Carrega o usuario autenticado e exige conta ativa.
    async fn usuario_ativo(&self, id: Uuid) -> Result<Usuario, ErroRegistro> {
        let usuario = self
            .repositorio_usuario
            .buscar_por_id(id)
            .await?
            .ok_or(ErroRegistro::NaoEncontrado)?;

        if !usuario.esta_ativo() {
            return Err(ErroRegistro::SemPermissao);
        }
        Ok(usuario)
    }

    async fn visita_existente(&self, id: Uuid) -> Result<Visita, ErroRegistro> {
        self.repositorio_visita
            .buscar_por_id(id)
            .await?
            .ok_or(ErroRegistro::NaoEncontrado)
    }

    async fn registro_existente(&self, id: Uuid) -> Result<RegistroVisita, ErroRegistro> {
        self.repositorio_registro
            .buscar_por_id(id)
            .await?
            .ok_or(ErroRegistro::NaoEncontrado)
    }

    /// Quem executa a visita e edita o rascunho: apenas o tecnico responsavel
    /// com a visita 'Em andamento' (RN05/RN06, RN12).
    fn exigir_rascunho_editavel(
        &self,
        logado: &Usuario,
        registro: &RegistroVisita,
    ) -> Result<(), ErroRegistro> {
        if registro.status != StatusRegistro::Rascunho {
            return Err(ErroRegistro::RegistroNaoRascunho);
        }
        if registro.registrado_por != logado.id {
            return Err(ErroRegistro::SemPermissao);
        }
        Ok(())
    }

    fn exigir_tecnico_responsavel(
        &self,
        logado: &Usuario,
        visita: &Visita,
    ) -> Result<(), ErroRegistro> {
        if logado.perfil != PerfilUsuario::Tecnico || visita.id_tecnico != logado.id {
            return Err(ErroRegistro::SemPermissao);
        }
        Ok(())
    }

    /// Visibilidade do registro: tecnico responsavel ou coordenador da
    /// mesma unidade da familia visitada.
    async fn pode_supervisionar(&self, logado: &Usuario, visita: &Visita) -> bool {
        if visita.id_tecnico == logado.id {
            return true;
        }
        if logado.perfil != PerfilUsuario::Coordenador {
            return false;
        }
        match self
            .repositorio_familia
            .buscar_por_id(visita.id_familia)
            .await
        {
            Ok(Some(familia)) => familia.id_unidade == logado.id_unidade,
            _ => false,
        }
    }
}
