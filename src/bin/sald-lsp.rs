// Sald Language Server Protocol (LSP) Binary
// Provides IDE features like autocomplete, diagnostics, go-to-definition

use sald_core::lsp::SaldLanguageServer;
use tower_lsp::{LspService, Server};

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| SaldLanguageServer::new(client));

    Server::new(stdin, stdout, socket).serve(service).await;
}
