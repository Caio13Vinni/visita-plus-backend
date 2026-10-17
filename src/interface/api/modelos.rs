// Modelos de resposta e requisição compartilhados da API.

use serde::Serialize;

/// Resposta padrão de erro da API.
#[derive(Debug, Serialize)]
pub struct ErroApi {
    pub mensagem: String,
}

impl ErroApi {
    /// Cria um erro de API a partir de uma mensagem.
    pub fn nova(mensagem: impl Into<String>) -> ErroApi {
        ErroApi {
            mensagem: mensagem.into(),
        }
    }
}
