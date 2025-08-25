mod extract_into_function;
mod filedb;
mod inlay_hints;
mod symbol_table;
mod typedb;
pub mod utils;

use filedb::FileDatabase;
use inlay_hints::make_inlay_hints;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use extract_into_function::extract_into_function_action;
use typedb::TypeDatabase;

struct Backend {
    client: Client,
    typedb: TypeDatabase,
    filedb: FileDatabase,
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        let mut result = InitializeResult::default();
        let code_action_options = CodeActionOptions {
            code_action_kinds: Some(vec![CodeActionKind::REFACTOR_EXTRACT]),
            work_done_progress_options: WorkDoneProgressOptions {
                work_done_progress: None,
            },
            resolve_provider: None,
        };
        result.capabilities = ServerCapabilities {
            code_action_provider: Some(CodeActionProviderCapability::Options(code_action_options)),
            inlay_hint_provider: Some(OneOf::Left(true)),
            text_document_sync: Some(TextDocumentSyncCapability::Kind(
                TextDocumentSyncKind::INCREMENTAL,
            )),
            ..Default::default()
        };
        Ok(result)
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let actions = self.code_actions(params);
        Ok(Some(actions))
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "server initialized!")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        self.inlay_hints(params)
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.did_open(params);
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.did_close(params);
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        self.did_change(params);
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        self.did_save(params);
    }
}

impl Backend {
    fn code_actions(&self, params: CodeActionParams) -> CodeActionResponse {
        let mut actions = Vec::new();

        if let Some(action) = extract_into_function_action(&params) {
            actions.push(action);
        }

        actions
    }

    fn inlay_hints(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        let vec = make_inlay_hints(
            params.range,
            params.text_document.uri.path(),
            &self.typedb,
            &self.filedb,
        );
        if vec.is_empty() {
            Ok(None)
        } else {
            Ok(Some(vec))
        }
    }

    fn did_open(&self, params: DidOpenTextDocumentParams) {
        let file_path = params.text_document.uri.path();
        self.filedb
            .file_opened(file_path, params.text_document.text);
    }

    fn did_close(&self, params: DidCloseTextDocumentParams) {}

    fn did_change(&self, params: DidChangeTextDocumentParams) {
        let file_path = params.text_document.uri.path();
        self.filedb.file_changed(file_path, params.content_changes);
    }

    fn did_save(&self, params: DidSaveTextDocumentParams) {}
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    const TYPE_INFO: &str = include_str!("../assets/type_info.json");
    let typedb = TypeDatabase::from_str(TYPE_INFO).unwrap();
    let (service, socket) = LspService::new(|client| Backend {
        client,
        typedb,
        filedb: FileDatabase::default(),
    });
    Server::new(stdin, stdout, socket).serve(service).await;
}
