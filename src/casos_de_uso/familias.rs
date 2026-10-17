// =============================================================================
// UC03 — Gerenciar familias
// -----------------------------------------------------------------------------
// Regras aplicadas:
//   - Cadastros estritamente ficticios (RN24);
//   - Manutencao (criar, editar, inativar, listar) na tabela `familia`;
//   - Validacao de duplicidade por codigo unico (RN23).
// =============================================================================

use std::sync::Arc;
use uuid::Uuid;

use crate::dominio::entidades::familia::{CadastroFamilia, Familia};
use crate::dominio::enums::{PerfilUsuario, StatusFamilia};
use crate::dominio::erros::{ErroCadastro, erro_sql_e_violacao_de_unicidade};
use crate::infraestrutura::repositorios::familia_repositorio::RepositorioFamilia;
use crate::infraestrutura::repositorios::usuario_repositorio::RepositorioUsuario;

/// Orquestrador do caso de uso de gerenciamento de familias.
#[derive(Clone)]
pub struct CasoDeUsoFamilias {
    repositorio_familia: Arc<dyn RepositorioFamilia>,
    repositorio_usuario: Arc<dyn RepositorioUsuario>,
}

impl CasoDeUsoFamilias {
    /// Cria o caso de uso com suas dependencias.
    pub fn novo(
        repositorio_familia: Arc<dyn RepositorioFamilia>,
        repositorio_usuario: Arc<dyn RepositorioUsuario>,
    ) -> Self {
        Self {
            repositorio_familia,
            repositorio_usuario,
        }
    }

    /// Verifica se o usuario possui perfil de coordenador.
    async fn validar_coordenador(&self, id_usuario_logado: Uuid) -> Result<(), ErroCadastro> {
        let logado = self
            .repositorio_usuario
            .buscar_por_id(id_usuario_logado)
            .await?
            .ok_or(ErroCadastro::NaoEncontrado)?;

        if logado.perfil != PerfilUsuario::Coordenador {
            return Err(ErroCadastro::SemPermissao);
        }
        Ok(())
    }

    /// Cadastra uma nova familia ficticia na unidade.
    pub async fn cadastrar_familia(
        &self,
        id_usuario_logado: Uuid,
        id_unidade: Uuid,
        cadastro: &CadastroFamilia,
    ) -> Result<Familia, ErroCadastro> {
        self.validar_coordenador(id_usuario_logado).await?;

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
            Ok(familia) => Ok(familia),
            Err(erro) if erro_sql_e_violacao_de_unicidade(&erro) => {
                Err(ErroCadastro::CodigoJaExistente)
            }
            Err(erro) => Err(ErroCadastro::from(erro)),
        }
    }

    /// Lista as familias, opcionalmente filtrando por unidade e status.
    pub async fn listar_familias(
        &self,
        id_usuario_logado: Uuid,
        id_unidade: Option<Uuid>,
        status: Option<StatusFamilia>,
    ) -> Result<Vec<Familia>, ErroCadastro> {
        // Coordenador e tecnico consultam familias; apenas o tecnico fica
        // restrito a unidade vinculada (isso e reforcado no UC05).
        let _ = self
            .repositorio_usuario
            .buscar_por_id(id_usuario_logado)
            .await?
            .ok_or(ErroCadastro::NaoEncontrado)?;

        Ok(self.repositorio_familia.listar(id_unidade, status).await?)
    }

    /// Edita os dados cadastrais de uma familia existente.
    pub async fn editar_familia(
        &self,
        id_usuario_logado: Uuid,
        id_familia: Uuid,
        cadastro: &CadastroFamilia,
    ) -> Result<Familia, ErroCadastro> {
        self.validar_coordenador(id_usuario_logado).await?;

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
        Ok(familia)
    }

    /// Altera o status da familia (ativa/inativa).
    pub async fn alterar_status_familia(
        &self,
        id_usuario_logado: Uuid,
        id_familia: Uuid,
        novo_status: StatusFamilia,
    ) -> Result<Familia, ErroCadastro> {
        self.validar_coordenador(id_usuario_logado).await?;

        let familia = self
            .repositorio_familia
            .buscar_por_id(id_familia)
            .await?
            .ok_or(ErroCadastro::NaoEncontrado)?;

        self.repositorio_familia
            .alterar_status(id_familia, novo_status)
            .await?;

        let mut atualizada = familia;
        atualizada.status = novo_status;
        Ok(atualizada)
    }
}
