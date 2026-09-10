# CONTEXTO TÉCNICO OFICIAL - PROJETO VISITA+
## Sistema Móvel de Apoio às Visitas Domiciliares (CRAS / CREAS)

### 1. Escopo, Limites e Restrições Legais
- **Natureza do Sistema:** Protótipo acadêmico para apoiar o planejamento, execução e registro operacional de visitas domiciliares.
- **Proibições Explícitas:** O sistema NÃO substitui o Prontuário Eletrônico do SUAS, NÃO realiza integração com sistemas governamentais, NÃO faz classificação de risco por Inteligência Artificial e NÃO utiliza dados reais (RN24, RN25). Todos os dados gerados em testes ou bancos devem ser estritamente fictícios.

### 2. Governança de Código e Padrão de Idioma (PT-BR)
- **Idioma do Código:** **Todas as declarações de variáveis, funções, structs, enums, módulos, comentários e mensagens de erro do código em Rust DEVEM ser escritas estritamente em Português do Brasil (PT-BR).** O padrão deve refletir os termos exatos do MER e da documentação acadêmica (ex: `tecnico_responsavel`, `data_visita`, `status_visita`).
- **Proteção da Main:** A branch `main` é sagrada e está estritamente bloqueada. **Apenas o usuário humano principal tem permissão para fazer o merge de `develop` para `main`.** O agente de IA opera apenas em branches `feature/...` e abre Pull Requests para a `develop`.
- **Integridade com o Banco:** Todo código que interaja com o banco deve passar por testes automatizados antes de abrir o PR para garantir que o esquema relacional não seja quebrado.

### 3. Stack Tecnológica e Clean Architecture
- **Linguagem:** Rust.
- **Camadas (Clean Architecture):** `dominio` (domain), `casos_de_uso` (use_cases), `infraestrutura` (infrastructure), `interface` (adapters/handlers).
- **Bancos de Dados & Sincronização Bidirecional:**
  - **Banco Central (Online - PostgreSQL):** Contém o modelo relacional exato do MER fornecido.
  - **Banco Local (Mobile - SQLite):** Arquivo SQLite local simulando o dispositivo, com um serviço de sincronização bidirecional na API para reconciliação de dados offline/online.

### 4. Dicionário de Tabelas do MER (PostgreSQL)
O banco central deve conter rigorosamente as seguintes tabelas e relacionamentos mapeados no MER:
1. **`unidade`**: `id_unidade` (PK, UUID), `nome`, `tipo` (CRAS/CREAS), `cidade`, `criado_em`.
2. **`usuario`**: `id_usuario` (PK, UUID), `id_unidade` (FK), `nome`, `email` (UNIQUE), `senha_hash`, `perfil` (Coordenador, Tecnico), `status` (Ativo, Inativo), `exige_troca_senha` (BOOLEAN), `ultimo_acesso_em`, etc.
3. **`sessao` & `recuperacao_acesso`**: Gestão de tokens JWT/sessões ativas e fluxo de recuperação de senha por código.
4. **`familia`**: `id_familia` (PK, UUID), `id_unidade` (FK), `codigo` (UNIQUE), `nome_referencia`, `contato`, `endereco`, `status` (Ativa, Inativa), `criado_por` (FK -> usuario).
5. **`solicitacao_retorno`**: `id_solicitacao` (PK, UUID), `id_visita` (FK), `motivo`, `data_solicitacao`, `status` (Pendente, Aprovada, Rejeitada, Cancelada), `decidido_por` (FK -> usuario).
6. **`visita`**: `id_visita` (PK, UUID), `id_familia` (FK), `id_tecnico` (FK -> usuario), `id_agendado_por` (FK -> usuario), `id_origem_retorno` (FK -> solicitacao_retorno, nulo), `data_visita`, `horario`, `prioridade` (Baixa, Normal, Alta), `objetivo`, `status` (Agendada, Em andamento, Realizada, Nao realizada, Cancelada), `motivo_cancelamento`.
7. **`registro_visita`**: `id_registro` (PK, UUID), `id_visita` (FK, UNIQUE - relação 1 para 0..1), `resultado` (Realizada, Nao realizada), `descricao`, `desfecho`, `necessita_retorno`, `registrado_por` (FK -> usuario).
8. **`complemento_registro`**: `id_complemento` (PK, UUID), `id_registro` (FK), `descricao`, `motivo`, `registrado_por` (FK -> usuario). *Garante a regra de que visitas finalizadas não são reabertas, exigindo complementos imutáveis (RN16, RN17).*
9. **`encaminhamento`**: `id_encaminhamento` (PK, UUID), `id_visita` (FK), `destino`, `motivo`, `data_encaminhamento`, `status` (Aberto, Concluido).
10. **`notificacao`**: `id_notificacao` (PK, UUID), `id_usuario_destino` (FK), `titulo`, `mensagem`, `tipo`, `lida`, `data_hora`. *Regra: Texto externo nunca expõe dados sensíveis (RN21).*
11. **`auditoria`**: `id_auditoria` (PK, UUID), `id_usuario` (FK), `entidade`, `entidade_id`, `acao`, `descricao`, `ip`, `device_info`, `data_hora`. *Registra ações críticas (RN22).*

### 5. Regras de Negócio Críticas (RN01 a RN25)
- **RN01 & RN02:** Contas criadas exclusivamente pelo coordenador (sem cadastro público). Apenas usuários ativos entram.
- **RN03:** Troca de senha obrigatória no primeiro acesso.
- **RN05 & RN06:** O técnico só enxerga visitas e dados de famílias vinculadas à sua própria agenda.
- **RN09 & RN10:** Proibido agendamento retroativo. Sistema deve validar conflito de horário na agenda do técnico antes de salvar.
- **RN12 & RN13:** Rascunhos permitidos. Visitas realizadas exigem resumo/descrição, desfecho e horário de término.
- **RN14:** Visitas não realizadas exigem um motivo obrigatório.
- **RN16 & RN17:** Visitas finalizadas são imutáveis. Correções exigem inserção de `complemento_registro` com autor e justificativa.
- **RN18:** Histórico preserva cancelamentos, reagendamentos e trocas de responsável.
- **RN19 & RN20:** Técnicos solicitam retorno; apenas coordenadores aprovam e agendam a nova visita.
- **RN21:** Notificações em tela bloqueada totalmente genéricas (sem dados sensíveis).
- **RN22 & RNF13:** Auditoria automática em operações críticas.