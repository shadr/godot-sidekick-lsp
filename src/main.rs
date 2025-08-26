mod extract_into_function;
mod filedb;
mod inlay_hints;
mod symbol_table;
mod typedb;
pub mod utils;

use std::ops::ControlFlow;

use async_lsp::client_monitor::ClientProcessMonitorLayer;
use async_lsp::concurrency::ConcurrencyLayer;
use async_lsp::lsp_types::*;
use async_lsp::panic::CatchUnwindLayer;
use async_lsp::router::Router;
use async_lsp::server::LifecycleLayer;
use async_lsp::tracing::TracingLayer;
use async_lsp::{ClientSocket, LanguageServer, ResponseError};
use filedb::FileDatabase;
use futures::future::BoxFuture;
use inlay_hints::make_inlay_hints;

use extract_into_function::extract_into_function_action;
use tower::ServiceBuilder;
use tracing::Level;
use typedb::TypeDatabase;

struct Backend {
    client: ClientSocket,
    typedb: TypeDatabase,
    filedb: FileDatabase,
}

impl LanguageServer for Backend {
    type Error = ResponseError;
    type NotifyResult = ControlFlow<async_lsp::Result<()>>;

    fn initialize(
        &mut self,
        _: InitializeParams,
    ) -> BoxFuture<'static, Result<InitializeResult, Self::Error>> {
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
        Box::pin(async move { Ok(result) })
    }

    fn code_action(
        &mut self,
        params: CodeActionParams,
    ) -> BoxFuture<'static, Result<Option<CodeActionResponse>, Self::Error>> {
        Box::pin(async move {
            let mut actions = Vec::new();

            if let Some(action) = extract_into_function_action(&params) {
                actions.push(action);
            }

            Ok(Some(actions))
        })
    }

    // fn initialized(&mut self, _: InitializedParams) {
    //     self.client
    //         .log_message(MessageType::INFO, "server initialized!")
    //         .await;
    // }

    fn inlay_hint(
        &mut self,
        params: InlayHintParams,
    ) -> BoxFuture<'static, Result<Option<Vec<InlayHint>>, Self::Error>> {
        let vec = make_inlay_hints(
            params.range,
            params.text_document.uri.path(),
            &self.typedb,
            &self.filedb,
        );
        Box::pin(async move {
            if vec.is_empty() {
                Ok(None)
            } else {
                Ok(Some(vec))
            }
        })
    }

    fn did_open(
        &mut self,
        params: DidOpenTextDocumentParams,
    ) -> ControlFlow<Result<(), async_lsp::Error>> {
        let file_path = params.text_document.uri.path();
        self.filedb
            .file_opened(file_path, params.text_document.text);
        ControlFlow::Continue(())
    }

    fn did_close(
        &mut self,
        _params: DidCloseTextDocumentParams,
    ) -> ControlFlow<Result<(), async_lsp::Error>> {
        ControlFlow::Continue(())
    }

    fn did_change(
        &mut self,
        params: DidChangeTextDocumentParams,
    ) -> ControlFlow<Result<(), async_lsp::Error>> {
        let file_path = params.text_document.uri.path();
        self.filedb.file_changed(file_path, params.content_changes);
        ControlFlow::Continue(())
    }

    fn did_save(
        &mut self,
        _params: DidSaveTextDocumentParams,
    ) -> ControlFlow<Result<(), async_lsp::Error>> {
        ControlFlow::Continue(())
    }
}

impl Backend {
    fn new_router(client: ClientSocket) -> Router<Self> {
        const TYPE_INFO: &str = include_str!("../assets/type_info.json");
        let typedb = TypeDatabase::from_str(TYPE_INFO).unwrap();

        Router::from_language_server(Self {
            client,
            typedb,
            filedb: FileDatabase::default(),
        })
    }
}

#[tokio::main]
async fn main() {
    let (server, _) = async_lsp::MainLoop::new_server(|client| {
        ServiceBuilder::new()
            .layer(TracingLayer::default())
            .layer(LifecycleLayer::default())
            .layer(CatchUnwindLayer::default())
            .layer(ConcurrencyLayer::default())
            .layer(ClientProcessMonitorLayer::new(client.clone()))
            .service(Backend::new_router(client))
    });

    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .with_ansi(false)
        .with_writer(std::io::stderr)
        .init();

    // Prefer truly asynchronous piped stdin/stdout without blocking tasks.
    #[cfg(unix)]
    let (stdin, stdout) = (
        async_lsp::stdio::PipeStdin::lock_tokio().unwrap(),
        async_lsp::stdio::PipeStdout::lock_tokio().unwrap(),
    );
    // Fallback to spawn blocking read/write otherwise.
    #[cfg(not(unix))]
    let (stdin, stdout) = (
        tokio_util::compat::TokioAsyncReadCompatExt::compat(tokio::io::stdin()),
        tokio_util::compat::TokioAsyncWriteCompatExt::compat_write(tokio::io::stdout()),
    );

    server.run_buffered(stdin, stdout).await.unwrap();
}
