// Sald Language Server Protocol (LSP) Module
// Provides IDE features: diagnostics, completion, hover, go-to-definition

mod analyzer;
mod backend;
mod completion;
mod import_resolver;
mod symbols;

pub use backend::SaldLanguageServer;
