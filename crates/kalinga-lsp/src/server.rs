//! The protocol side: messages in, the right feature called, answers and diagnostics out.

use std::error::Error;
use std::panic::{AssertUnwindSafe, catch_unwind};

use lsp_server::{Connection, ErrorCode, Message, Notification, Request, Response};
use lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, DidSaveTextDocument,
    Notification as LspNotification, PublishDiagnostics,
};
use lsp_types::request::{
    CodeActionRequest, Completion, DocumentSymbolRequest, Formatting, GotoDefinition, HoverRequest,
    PrepareRenameRequest, References, Rename, Request as LspRequest, SignatureHelpRequest,
};
use lsp_types::*;

use crate::world::World;
use crate::{actions, completion, formatting, hover, navigation, signature};

type Fallible<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// What the server can do, as told to the editor.
pub fn capabilities() -> ServerCapabilities {
    ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        completion_provider: Some(CompletionOptions {
            trigger_characters: Some(vec![".".into()]),
            ..CompletionOptions::default()
        }),
        signature_help_provider: Some(SignatureHelpOptions {
            trigger_characters: Some(vec!["(".into(), ",".into()]),
            ..SignatureHelpOptions::default()
        }),
        definition_provider: Some(OneOf::Left(true)),
        references_provider: Some(OneOf::Left(true)),
        rename_provider: Some(OneOf::Right(RenameOptions {
            prepare_provider: Some(true),
            work_done_progress_options: WorkDoneProgressOptions::default(),
        })),
        document_symbol_provider: Some(OneOf::Left(true)),
        code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
        document_formatting_provider: Some(OneOf::Left(true)),
        ..ServerCapabilities::default()
    }
}

/// Serves the editor on stdin / stdout until it asks the server to exit.
pub fn run() -> Fallible<()> {
    let (connection, io_threads) = Connection::stdio();
    connection.initialize(serde_json::to_value(capabilities())?)?;
    let mut world = World::default();
    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                if connection.handle_shutdown(&req)? {
                    break;
                }
                let response = respond(&mut world, req);
                connection.sender.send(Message::Response(response))?;
            }
            Message::Notification(n) => {
                if notify(&mut world, n)? {
                    publish(&connection, &mut world)?;
                }
            }
            Message::Response(_) => {}
        }
    }
    drop(connection);
    io_threads.join()?;
    Ok(())
}

/// Applies a notification; `true` when the documents changed.
fn notify(world: &mut World, n: Notification) -> Fallible<bool> {
    match n.method.as_str() {
        DidOpenTextDocument::METHOD => {
            let p: DidOpenTextDocumentParams = serde_json::from_value(n.params)?;
            world.open(p.text_document.uri, p.text_document.text);
        }
        DidChangeTextDocument::METHOD => {
            let p: DidChangeTextDocumentParams = serde_json::from_value(n.params)?;
            if let Some(change) = p.content_changes.into_iter().last() {
                world.change(&p.text_document.uri, change.text);
            }
        }
        DidCloseTextDocument::METHOD => {
            let p: DidCloseTextDocumentParams = serde_json::from_value(n.params)?;
            world.close(&p.text_document.uri);
        }
        DidSaveTextDocument::METHOD => {}
        _ => return Ok(false),
    }
    Ok(true)
}

/// Sends the diagnostics of every open document (an edit can affect the files importing it).
fn publish(connection: &Connection, world: &mut World) -> Fallible<()> {
    for uri in world.uris() {
        // A bug in the checker must not stop the server: the file just gets no diagnostics.
        let diagnostics = catch_unwind(AssertUnwindSafe(|| world.diagnostics(&uri))).unwrap_or_default();
        let params = PublishDiagnosticsParams { uri, diagnostics, version: None };
        let n = Notification::new(PublishDiagnostics::METHOD.into(), params);
        connection.sender.send(Message::Notification(n))?;
    }
    Ok(())
}

/// Answers `req`. A panic in a feature answers this one request with an error instead of
/// stopping the server.
fn respond(world: &mut World, req: Request) -> Response {
    let id = req.id.clone();
    let method = req.method.clone();
    let result = catch_unwind(AssertUnwindSafe(|| dispatch(world, req))).unwrap_or_else(|_| {
        Err((ErrorCode::InternalError, format!("the Kalinga language server failed on `{}`", method)))
    });
    match result {
        Ok(value) => Response::new_ok(id, value),
        Err((code, message)) => Response::new_err(id, code as i32, message),
    }
}

fn dispatch(world: &mut World, req: Request) -> Result<serde_json::Value, (ErrorCode, String)> {
    match req.method.as_str() {
        HoverRequest::METHOD => answer::<HoverRequest>(req, |p| hover::hover(world, p)),
        Completion::METHOD => answer::<Completion>(req, |p| completion::complete(world, p)),
        SignatureHelpRequest::METHOD => answer::<SignatureHelpRequest>(req, |p| signature::help(world, p)),
        GotoDefinition::METHOD => answer::<GotoDefinition>(req, |p| navigation::definition(world, p)),
        References::METHOD => answer::<References>(req, |p| navigation::references(world, p)),
        PrepareRenameRequest::METHOD => answer::<PrepareRenameRequest>(req, |p| navigation::prepare_rename(world, p)),
        Rename::METHOD => answer::<Rename>(req, |p| navigation::rename(world, p)),
        DocumentSymbolRequest::METHOD => answer::<DocumentSymbolRequest>(req, |p| navigation::symbols(world, p)),
        CodeActionRequest::METHOD => answer::<CodeActionRequest>(req, |p| actions::actions(world, p)),
        Formatting::METHOD => answer::<Formatting>(req, |p| formatting::format(world, p)),
        _ => Err((ErrorCode::MethodNotFound, format!("unknown request `{}`", req.method))),
    }
}

/// Runs `f` on the request's parameters and turns its answer into JSON.
fn answer<R: LspRequest>(
    req: Request,
    f: impl FnOnce(R::Params) -> R::Result,
) -> Result<serde_json::Value, (ErrorCode, String)> {
    let params: R::Params =
        serde_json::from_value(req.params).map_err(|e| (ErrorCode::InvalidParams, e.to_string()))?;
    serde_json::to_value(f(params)).map_err(|e| (ErrorCode::InternalError, e.to_string()))
}
