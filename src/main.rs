mod extract_into_function;
pub mod utils;

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use extract_into_function::extract_into_function_action;

#[derive(Debug)]
struct Backend {
    client: Client,
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
}

impl Backend {
    fn code_actions(&self, params: CodeActionParams) -> CodeActionResponse {
        let mut actions = Vec::new();

        if let Some(action) = extract_into_function_action(&params) {
            actions.push(action);
        }

        actions
    }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| Backend { client });
    Server::new(stdin, stdout, socket).serve(service).await;
}
