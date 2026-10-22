// =============================================================================
// UC02 — Gerenciar profissionais
// -----------------------------------------------------------------------------
// Regras aplicadas:
//   - Somente o Coordenador gerencia profissionais (RN02);
//   - Cadastro, edicao, ativacao e desativacao de contas na tabela `usuario`;
//   - No primeiro acesso o novo usuario deve trocar a senha (RN05);
//   - Nao e possivel desativar um tecnico com visitas futuras ativas;
//   - Coordenador opera apenas na propria unidade (RN05/RN06) e as
//     manutencoes geram trilha de auditoria (RN22).
// =============================================================================

use std::sync::Arc;
use uuid::Uuid;

use crate::dominio::entidades::usuario::Usuario;
use crate::dominio::enums::{PerfilUsuario, StatusUsuario};
use crate::dominio::erros::{ErroCadastro, erro_sql_e_violacao_de_unicidade};
use crate::infraestrutura::auditoria::{ServicoAuditoria, acao};
use crate::infraestrutura::repositorios::usuario_repositorio::{NovoUsuario, RepositorioUsuario};
use crate::infraestrutura::repositorios::visita_repositorio::RepositorioVisita;
use crate::infraestrutura::seguranca::senha;

/// Dados de entrada para o cadastro de um profissional (UC02).
#[derive(Debug, Clone)]
pub struct CadastroProfissional {
    pub id_unidade: Uuid,
    pub nome: String,
    pub email: String,
    pub senha: String,
    pub perfil: PerfilUsuario,
}

/// Dados de edicao de um profissional (nome e perfil).
#[derive(Debug, Clone)]
pub struct EdicaoProfissional {
    pub nome: String,
    pub perfil: PerfilUsuario,
}

/// Orquestrador do caso de uso de gerenciamento de profissionais.
#[derive(Clone)]
pub struct CasoDeUsoProfissionais {
    repositorio_usuario: Arc<dyn RepositorioUsuario>,
    repositorio_visita: Arc<dyn RepositorioVisita>,
    auditoria: ServicoAuditoria,
}

impl CasoDeUsoProfissionais {
    /// Cria o caso de uso com suas dependencias.
    pub fn novo(
        repositorio_usuario: Arc<dyn RepositorioUsuario>,
        repositorio_visita: Arc<dyn RepositorioVisita>,
        auditoria: ServicoAuditoria,
    ) -> Self {
        Self {
            repositorio_usuario,
            repositorio_visita,
            auditoria,
        }
    }

    /// Lista os profissionais da unidade do coordenador logado.
    pub async fn listar_profissionais(
        &self,
        id_usuario_logado: Uuid,
        id_unidade: Option<Uuid>,
    ) -> Result<Vec<Usuario>, ErroCadastro> {
        let logado = self
            .repositorio_usuario
            .buscar_por_id(id_usuario_logado)
            .await?
            .ok_or(ErroCadastro::NaoEncontrado)?;

        if logado.perfil != PerfilUsuario::Coordenador {
            return Err(ErroCadastro::SemPermissao);
        }

        let unidade = id_unidade.unwrap_or(logado.id_unidade);
        if unidade != logado.id_unidade {
            return Err(ErroCadastro::SemPermissao);
        }

        Ok(self.repositorio_usuario.listar(Some(unidade)).await?)
    }

    /// Cadastra um novo profissional (somente Coordenador, na propria unidade).
    pub async fn cadastrar_profissional(
        &self,
        id_usuario_logado: Uuid,
        cadastro: CadastroProfissional,
    ) -> Result<Usuario, ErroCadastro> {
        let logado = self
            .repositorio_usuario
            .buscar_por_id(id_usuario_logado)
            .await?
            .ok_or(ErroCadastro::NaoEncontrado)?;

        if logado.perfil != PerfilUsuario::Coordenador {
            return Err(ErroCadastro::SemPermissao);
        }

        if cadastro.id_unidade != logado.id_unidade {
            return Err(ErroCadastro::SemPermissao);
        }

        if !Usuario::email_eh_valido(&cadastro.email) {
            return Err(ErroCadastro::DadosInvalidos(
                "formato de email invalido".into(),
            ));
        }

        let senha_hash = senha::gerar_hash_senha(&cadastro.senha)
            .map_err(|erro| ErroCadastro::Tecnica(erro.to_string()))?;

        let novo = NovoUsuario {
            id_unidade: cadastro.id_unidade,
            nome: cadastro.nome,
            email: cadastro.email,
            senha_hash,
            perfil: cadastro.perfil,
        };

        match self.repositorio_usuario.criar(&novo).await {
            Ok(usuario) => {
                let _ = self
                    .auditoria
                    .registrar(
                        Some(logado.id),
                        "usuario",
                        usuario.id,
                        acao::PROFISSIONAL_CRIADO,
                        Some(&format!(
                            "Profissional {} cadastrado com perfil {}",
                            usuario.nome,
                            usuario.perfil.como_str()
                        )),
                        None,
                        None,
                    )
                    .await;
                Ok(usuario)
            }
            Err(erro) if erro_sql_e_violacao_de_unicidade(&erro) => {
                Err(ErroCadastro::EmailJaExistente)
            }
            Err(erro) => Err(ErroCadastro::from(erro)),
        }
    }

    /// Edita nome e perfil de um profissional existente.
    pub async fn editar_profissional(
        &self,
        id_usuario_logado: Uuid,
        id_profissional: Uuid,
        edicao: EdicaoProfissional,
    ) -> Result<Usuario, ErroCadastro> {
        let logado = self
            .repositorio_usuario
            .buscar_por_id(id_usuario_logado)
            .await?
            .ok_or(ErroCadastro::NaoEncontrado)?;

        if logado.perfil != PerfilUsuario::Coordenador {
            return Err(ErroCadastro::SemPermissao);
        }

        let existente = self
            .repositorio_usuario
            .buscar_por_id(id_profissional)
            .await?
            .ok_or(ErroCadastro::NaoEncontrado)?;

        if existente.id_unidade != logado.id_unidade {
            return Err(ErroCadastro::SemPermissao);
        }

        if edicao.nome.trim().is_empty() {
            return Err(ErroCadastro::DadosInvalidos(
                "nome nao pode ser vazio".into(),
            ));
        }

        self.repositorio_usuario
            .atualizar(id_profissional, &edicao.nome, edicao.perfil)
            .await?;

        let _ = self
            .auditoria
            .registrar(
                Some(logado.id),
                "usuario",
                id_profissional,
                acao::PROFISSIONAL_ALTERADO,
                Some(&format!(
                    "Profissional {} editado (nome e/ou perfil)",
                    edicao.nome
                )),
                None,
                None,
            )
            .await;

        // Devolve a entidade atualizada
        let mut atualizado = existente;
        atualizado.nome = edicao.nome;
        atualizado.perfil = edicao.perfil;
        Ok(atualizado)
    }

    /// Altera o status da conta (ativacao/desativacao).
    pub async fn alterar_status_profissional(
        &self,
        id_usuario_logado: Uuid,
        id_profissional: Uuid,
        novo_status: StatusUsuario,
    ) -> Result<Usuario, ErroCadastro> {
        let logado = self
            .repositorio_usuario
            .buscar_por_id(id_usuario_logado)
            .await?
            .ok_or(ErroCadastro::NaoEncontrado)?;

        if logado.perfil != PerfilUsuario::Coordenador {
            return Err(ErroCadastro::SemPermissao);
        }

        let profissional = self
            .repositorio_usuario
            .buscar_por_id(id_profissional)
            .await?
            .ok_or(ErroCadastro::NaoEncontrado)?;

        if profissional.id_unidade != logado.id_unidade {
            return Err(ErroCadastro::SemPermissao);
        }

        // Regra: nao desativar tecnico com visitas futuras ativas
        if novo_status == StatusUsuario::Inativo
            && profissional.perfil == PerfilUsuario::Tecnico
            && self
                .repositorio_visita
                .existem_visitas_futuras_ativas(id_profissional)
                .await?
        {
            return Err(ErroCadastro::TecnicoComVisitasFuturas);
        }

        self.repositorio_usuario
            .alterar_status(id_profissional, novo_status)
            .await?;

        let _ = self
            .auditoria
            .registrar(
                Some(logado.id),
                "usuario",
                id_profissional,
                acao::PROFISSIONAL_ALTERADO,
                Some(&format!(
                    "Status do profissional {} alterado para {}",
                    profissional.nome,
                    novo_status.como_str()
                )),
                None,
                None,
            )
            .await;

        let mut atualizado = profissional;
        atualizado.status = novo_status;
        Ok(atualizado)
    }
}
