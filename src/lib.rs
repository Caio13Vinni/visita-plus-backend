// =============================================================================
// VISITA+ — Biblioteca da aplicacao
// -----------------------------------------------------------------------------
// Organizacao em Clean Architecture com nomes em PT-BR:
//   - dominio        : entidades, valores e regras de negocio
//   - casos_de_uso   : orquestracao dos fluxos de aplicacao (UC01 a UC12)
//   - infraestrutura : adaptadores tecnicos (banco, seguranca, repositorios)
//   - interface      : camada externa (API HTTP, DTOs, estado da aplicacao)
// =============================================================================

pub mod dominio;
pub mod casos_de_uso;
pub mod infraestrutura;
pub mod interface;