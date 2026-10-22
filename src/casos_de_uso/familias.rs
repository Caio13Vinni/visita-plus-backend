// =============================================================================
// UC03 — Gerenciar familias
// -----------------------------------------------------------------------------
// Regras aplicadas:
//   - Cadastros estritamente ficticios (RN24);
//   - Manutencao (criar, editar, inativar, listar) na tabela `familia`;
//   - Validacao de duplicidade por codigo unico (RN23);
//   - Coordenador opera apenas na propria unidade e o tecnico acessa a
//     familia somente a partir de visita atribuida (RN05/RN06);
//   - Criacao e alteracoes geram trilha de auditoria (RN22).
// =============================================================================

use std::sync::Arc;
use uuid::Uuid;

use crate::dominio::entidades::familia::{CadastroFamilia, Familia};
use crate::dominio::entidades::usuario::Usuario;
use crate::dominio::enums::{PerfilUsuario, StatusFamilia};
use crate::dominio::erros::{ErroCadastro, erro_sql_e_violacao_de_unicidade};
use crate::infraestrutura::auditoria::{ServicoAuditoria, acao};
use crate::infraestrutura::repositorios::familia_repositorio::RepositorioFamilia;
use crate::infraestrutura::repositorios::usuario_repositorio::RepositorioUsuario;
use crate::infraestrutura::repositorios::visita_repositorio::{FiltroVisita, RepositorioVisita};

/// Orquestrador do caso de uso de gerenciamento de familias.
#[derive(Clone)]
pub struct CasoDeUsoFamilias {
    repositorio_familia: Arc<dyn RepositorioFamilia>,
    repositorio_usuario: Arc<dyn RepositorioUsuario>,
    repositorio_visita: Arc<dyn RepositorioVisita>,
    auditoria: ServicoAuditoria,
}

impl CasoDeUsoFamilias {
    /// Cria o caso de uso com suas dependencias.
    pub fn novo(
        repositorio_familia: Arc<dyn RepositorioFamilia>,
        repositorio_usuario: Arc<dyn RepositorioUsuario>,
        repositorio_visita: Arc<dyn RepositorioVisita>,
        auditoria: ServicoAuditoria,
    ) -> Self {
        Self {
            repositorio_familia,
            repositorio_usuario,
            repositorio_visita,
            auditoria,
        }
    }

    /// Verifica se o usuario possui perfil de coordenador e o retorna.
    async fn validar_coordenador(&self, id_usuario_logado: Uuid) -> Result<Usuario, ErroCadastro> {
        let logado = self
            .repositorio_usuario
            .buscar_por_id(id_usuario_logado)
            .await?
            .ok_or(ErroCadastro::NaoEncontrado)?;

        if logado.perfil != PerfilUsuario::Coordenador {
            return Err(ErroCadastro::SemPermissao);
        }
        Ok(logado)
    }

    /// Cadastra uma nova familia ficticia na unidade do coordenador.
    pub async fn cadastrar_familia(
        &self,
        id_usuario_logado: Uuid,
        id_unidade: Uuid,
        cadastro: &CadastroFamilia,
    ) -> Result<Familia, ErroCadastro> {
        let logado = self.validar_coordenador(id_usuario_logado).await?;

        if id_unidade != logado.id_unidade {
            return Err(ErroCadastro::SemPermissao);
        }

        if !Familia::dados_sao_validos(cadastro) {
            return Err(ErroCadastro::DadosInvalidos(
                "todos os campos da familia sao obrigatorios".into(),
            ));
        }

        let familia = Familia {
            id: Uuid::new_v4(),
            id_unidade,
            codigo: cadastro.codigo.trim().to_string(),
            nome_referencia: cadastro.nome_referencia.trim().to_string(),
            contato: cadastro.contato.trim().to_string(),
            endereco: cadastro.endereco.trim().to_string(),
            status: StatusFamilia::Ativa,
            criado_por: Some(id_usuario_logado),
            criado_em: chrono::Utc::now(),
        };

        match self.repositorio_familia.criar(&familia).await {
            Ok(familia) => {
                let _ = self
                    .auditoria
                    .registrar(
                        Some(logado.id),
                        "familia",
                        familia.id,
                        acao::FAMILIA_CRIADA,
                        Some(&format!("Familia {} cadastrada", familia.codigo)),
                        None,
                        None,
                    )
                    .await;
                Ok(familia)
            }
            Err(erro) if erro_sql_e_violacao_de_unicidade(&erro) => {
                Err(ErroCadastro::CodigoJaExistente)
            }
            Err(erro) => Err(ErroCadastro::from(erro)),
        }
    }

    /// Lista as familias conforme o perfil (RN05/RN06).
    ///
    /// O coordenador enxerga as familias da propria unidade; o tecnico,
    /// somente as familias vinculadas a visitas atribuidas a ele — a
    /// familia e acessada a partir de uma visita autorizada, nao por uma
    /// lista geral.
    pub async fn listar_familias(
        &self,
        id_usuario_logado: Uuid,
        id_unidade: Option<Uuid>,
        status: Option<StatusFamilia>,
    ) -> Result<Vec<Familia>, ErroCadastro> {
        let logado = self
            .repositorio_usuario
            .buscar_por_id(id_usuario_logado)
            .await?
            .ok_or(ErroCadastro::NaoEncontrado)?;

        match logado.perfil {
            PerfilUsuario::Coordenador => {
                let unidade = id_unidade.unwrap_or(logado.id_unidade);
                if unidade != logado.id_unidade {
                    return Err(ErroCadastro::SemPermissao);
                }
                Ok(self
                    .repositorio_familia
                    .listar(Some(unidade), status)
                    .await?)
            }
            PerfilUsuario::Tecnico => {
                let visitas = self
                    .repositorio_visita
                    .listar(&FiltroVisita {
                        id_tecnico: Some(logado.id),
                        ..Default::default()
                    })
                    .await?;
                let mut familias = Vec::new();
                for id_familia in visitas
                    .into_iter()
                    .map(|v| v.id_familia)
                    .collect::<std::collections::BTreeSet<_>>()
                {
                    if let Some(familia) =
                        self.repositorio_familia.buscar_por_id(id_familia).await?
                        && status.map(|s| familia.status == s).unwrap_or(true)
                    {
                        familias.push(familia);
                    }
                }
                Ok(familias)
            }
        }
    }

    /// Edita os dados cadastrais de uma familia existente.
    pub async fn editar_familia(
        &self,
        id_usuario_logado: Uuid,
        id_familia: Uuid,
        cadastro: &CadastroFamilia,
    ) -> Result<Familia, ErroCadastro> {
        let logado = self.validar_coordenador(id_usuario_logado).await?;

        if !Familia::dados_sao_validos(cadastro) {
            return Err(ErroCadastro::DadosInvalidos(
                "todos os campos da familia sao obrigatorios".into(),
            ));
        }

        let mut familia = self
            .repositorio_familia
            .buscar_por_id(id_familia)
            .await?
            .ok_or(ErroCadastro::NaoEncontrado)?;

        if familia.id_unidade != logado.id_unidade {
            return Err(ErroCadastro::SemPermissao);
        }

        // Se o codigo mudar, verifica duplicidade
        if familia.codigo != cadastro.codigo
            && self
                .repositorio_familia
                .buscar_por_codigo(&cadastro.codigo)
                .await?
                .is_some()
        {
            return Err(ErroCadastro::CodigoJaExistente);
        }

        familia.codigo = cadastro.codigo.trim().to_string();
        familia.nome_referencia = cadastro.nome_referencia.trim().to_string();
        familia.contato = cadastro.contato.trim().to_string();
        familia.endereco = cadastro.endereco.trim().to_string();

        self.repositorio_familia.atualizar(&familia).await?;

        let _ = self
            .auditoria
            .registrar(
                Some(logado.id),
                "familia",
                familia.id,
                acao::FAMILIA_ALTERADA,
                Some(&format!("Familia {} editada", familia.codigo)),
                None,
                None,
            )
            .await;

        Ok(familia)
    }

    /// Altera o status da familia (ativa/inativa).
    pub async fn alterar_status_familia(
        &self,
        id_usuario_logado: Uuid,
        id_familia: Uuid,
        novo_status: StatusFamilia,
    ) -> Result<Familia, ErroCadastro> {
        let logado = self.validar_coordenador(id_usuario_logado).await?;

        let familia = self
            .repositorio_familia
            .buscar_por_id(id_familia)
            .await?
            .ok_or(ErroCadastro::NaoEncontrado)?;

        if familia.id_unidade != logado.id_unidade {
            return Err(ErroCadastro::SemPermissao);
        }

        self.repositorio_familia
            .alterar_status(id_familia, novo_status)
            .await?;

        let mut atualizada = familia;
        atualizada.status = novo_status;

        let _ = self
            .auditoria
            .registrar(
                Some(logado.id),
                "familia",
                atualizada.id,
                acao::FAMILIA_ALTERADA,
                Some(&format!(
                    "Familia {} teve status alterado para {novo_status:?}",
                    atualizada.codigo
                )),
                None,
                None,
            )
            .await;

        Ok(atualizada)
    }
}
