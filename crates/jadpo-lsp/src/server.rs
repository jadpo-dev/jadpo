//! The server's state and what it does with each message. It keeps one
//! database for its whole life (docs/compiler-runtime.md §2.4): an edit
//! sets one file's text and the next check recomputes only what that
//! changes. Diagnostics are the same as `jadpo check`'s, from the same
//! queries, published once a burst of edits has been read.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use jadpo_check::graph::{Found, graph, lookup};
use jadpo_db::Loaded;
use lsp_server::{ErrorCode, Message, Notification, Request, Response};
use lsp_types::notification::{
    DidChangeTextDocument, DidChangeWatchedFiles, DidCloseTextDocument, DidOpenTextDocument,
    Initialized, Notification as _, PublishDiagnostics,
};
use lsp_types::request::{GotoDefinition, HoverRequest, Request as _};
use lsp_types::{
    DidChangeTextDocumentParams, DidChangeWatchedFilesParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, FileChangeType, GotoDefinitionParams, GotoDefinitionResponse, Hover,
    HoverContents, HoverParams, HoverProviderCapability, InitializeParams, InitializeResult,
    Location, MarkupContent, MarkupKind, OneOf, PositionEncodingKind, PublishDiagnosticsParams,
    ServerCapabilities, ServerInfo, TextDocumentSyncCapability, TextDocumentSyncKind,
    TextDocumentSyncOptions, Uri,
};

use crate::convert::{self, Encoding};
use crate::hover;

/// A language server over one project.
#[derive(Debug)]
pub struct Server {
    loaded: Option<Loaded>,
    encoding: Encoding,
    /// What each file was last published with.
    published: BTreeMap<String, Vec<lsp_types::Diagnostic>>,
    /// Files the editor has open, whose text the editor owns.
    open: BTreeSet<String>,
    /// Whether anything changed since diagnostics were last published.
    dirty: bool,
}

impl Server {
    /// The server for an editor's `initialize` request, and its answer.
    /// The project is the one holding the workspace's first folder, found
    /// by walking up to `app.jadpo`, or that folder's files on their own.
    pub fn initialize(params: &InitializeParams, cwd: &Path) -> (Server, InitializeResult) {
        #[allow(deprecated)]
        let folder = params
            .workspace_folders
            .as_ref()
            .and_then(|folders| folders.first())
            .and_then(|folder| convert::path_of(&folder.uri))
            .or_else(|| params.root_uri.as_ref().and_then(convert::path_of))
            .or_else(|| params.root_path.as_ref().map(PathBuf::from))
            .unwrap_or_else(|| cwd.to_path_buf());
        let folder = folder.canonicalize().unwrap_or(folder);
        let offered = params
            .capabilities
            .general
            .as_ref()
            .and_then(|general| general.position_encodings.as_ref());
        let encoding = if offered.is_some_and(|kinds| kinds.contains(&PositionEncodingKind::UTF8)) {
            Encoding::Utf8
        } else {
            Encoding::Utf16
        };
        let loaded = folder.is_dir().then(|| jadpo_db::load(&folder));
        let mut server = Server::new(loaded, encoding);
        // The first flush publishes what the project already has wrong.
        server.dirty = true;
        (server, capabilities(encoding))
    }

    /// A server over an already loaded project.
    pub fn new(loaded: Option<Loaded>, encoding: Encoding) -> Server {
        Server {
            loaded,
            encoding,
            published: BTreeMap::new(),
            open: BTreeSet::new(),
            dirty: false,
        }
    }

    pub fn loaded(&self) -> Option<&Loaded> {
        self.loaded.as_ref()
    }

    /// Handles one message from the editor; what to send back.
    pub fn handle(&mut self, message: Message) -> Vec<Message> {
        match message {
            Message::Request(request) => vec![Message::Response(self.request(request))],
            Message::Notification(notification) => {
                self.notification(notification);
                Vec::new()
            }
            Message::Response(_) => Vec::new(),
        }
    }

    fn request(&mut self, request: Request) -> Response {
        let id = request.id.clone();
        match request.method.as_str() {
            HoverRequest::METHOD => match serde_json::from_value::<HoverParams>(request.params) {
                Ok(params) => Response::new_ok(id, self.hover(&params)),
                Err(error) => invalid(id, &error),
            },
            GotoDefinition::METHOD => {
                match serde_json::from_value::<GotoDefinitionParams>(request.params) {
                    Ok(params) => Response::new_ok(id, self.definition(&params)),
                    Err(error) => invalid(id, &error),
                }
            }
            method => Response::new_err(
                id,
                ErrorCode::MethodNotFound as i32,
                format!("jadpo lsp doesn't answer `{method}` yet"),
            ),
        }
    }

    fn notification(&mut self, notification: Notification) {
        let params = notification.params;
        match notification.method.as_str() {
            Initialized::METHOD => self.dirty = true,
            DidOpenTextDocument::METHOD => {
                if let Ok(params) = serde_json::from_value::<DidOpenTextDocumentParams>(params)
                    && let Some(path) = self.relative(&params.text_document.uri)
                {
                    self.open.insert(path.clone());
                    self.edit(|loaded| loaded.set_text(&path, params.text_document.text));
                }
            }
            DidChangeTextDocument::METHOD => {
                if let Ok(params) = serde_json::from_value::<DidChangeTextDocumentParams>(params)
                    && let Some(path) = self.relative(&params.text_document.uri)
                    && let Some(change) = params.content_changes.into_iter().last()
                {
                    // The server asks for whole texts, so the last change is
                    // the document.
                    self.edit(|loaded| loaded.set_text(&path, change.text));
                }
            }
            DidCloseTextDocument::METHOD => {
                if let Ok(params) = serde_json::from_value::<DidCloseTextDocumentParams>(params)
                    && let Some(path) = self.relative(&params.text_document.uri)
                {
                    // Unsaved changes go with the editor: the disk is the
                    // text now.
                    self.open.remove(&path);
                    self.edit(|loaded| loaded.reload(&path));
                }
            }
            DidChangeWatchedFiles::METHOD => {
                if let Ok(params) = serde_json::from_value::<DidChangeWatchedFilesParams>(params) {
                    for change in params.changes {
                        let Some(path) = self.relative(&change.uri) else {
                            continue;
                        };
                        if self.open.contains(&path) {
                            continue;
                        }
                        if change.typ == FileChangeType::DELETED {
                            self.edit(|loaded| loaded.remove(&path));
                        } else {
                            self.edit(|loaded| loaded.reload(&path));
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn edit(&mut self, change: impl FnOnce(&mut Loaded)) {
        if let Some(loaded) = self.loaded.as_mut() {
            change(loaded);
            self.dirty = true;
        }
    }

    /// The project-relative path of a document inside the project.
    fn relative(&self, uri: &Uri) -> Option<String> {
        let loaded = self.loaded.as_ref()?;
        loaded.relative(&convert::path_of(uri)?)
    }

    /// Publishes every file's diagnostics that changed since the last time,
    /// when anything was edited.
    pub fn flush(&mut self) -> Vec<Message> {
        if !self.dirty {
            return Vec::new();
        }
        self.dirty = false;
        let Some(loaded) = self.loaded.as_ref() else {
            return Vec::new();
        };
        let db = &loaded.db;
        let texts: BTreeMap<String, String> = loaded
            .project
            .files(db)
            .iter()
            .map(|file| (file.path(db).clone(), file.text(db).clone()))
            .collect();
        let mut by_file: BTreeMap<String, Vec<lsp_types::Diagnostic>> = BTreeMap::new();
        for problem in jadpo_check::check(db, loaded.project) {
            let Some(label) = &problem.primary else {
                continue;
            };
            let Some(text) = texts.get(&label.span.path) else {
                continue;
            };
            let source_of = |path: &str| texts.get(path).cloned();
            by_file
                .entry(label.span.path.clone())
                .or_default()
                .push(convert::diagnostic(
                    &problem,
                    text,
                    &loaded.root,
                    &source_of,
                    self.encoding,
                ));
        }
        let paths: BTreeSet<String> = by_file
            .keys()
            .chain(self.published.keys())
            .cloned()
            .collect();
        let mut out = Vec::new();
        for path in paths {
            let now = by_file.remove(&path).unwrap_or_default();
            let before = self.published.get(&path);
            if before == Some(&now) || (before.is_none() && now.is_empty()) {
                continue;
            }
            let Some(uri) = convert::uri_of(&loaded.root.join(&path)) else {
                continue;
            };
            out.push(Message::Notification(Notification::new(
                PublishDiagnostics::METHOD.to_string(),
                PublishDiagnosticsParams {
                    uri,
                    diagnostics: now.clone(),
                    version: None,
                },
            )));
            if now.is_empty() && !texts.contains_key(&path) {
                self.published.remove(&path);
            } else {
                self.published.insert(path, now);
            }
        }
        out
    }

    /// The node or local under the cursor, with the file's text and the
    /// name's range.
    fn found(
        &self,
        uri: &Uri,
        position: lsp_types::Position,
    ) -> Option<(Found, String, (u32, u32))> {
        let loaded = self.loaded.as_ref()?;
        let path = self.relative(uri)?;
        let file = loaded.file(&path)?;
        let text = file.text(&loaded.db).clone();
        let offset = convert::offset(&text, position, self.encoding)?;
        let (found, range) = lookup(&loaded.db, loaded.project, file, offset)?;
        Some((found, text, range))
    }

    fn hover(&self, params: &HoverParams) -> Option<Hover> {
        let position = &params.text_document_position_params;
        let (found, text, (start, end)) =
            self.found(&position.text_document.uri, position.position)?;
        let loaded = self.loaded.as_ref()?;
        let graph = graph(&loaded.db, loaded.project);
        Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: hover::render(graph, &found)?,
            }),
            range: Some(convert::range(&text, start, end, self.encoding)),
        })
    }

    fn definition(&self, params: &GotoDefinitionParams) -> Option<GotoDefinitionResponse> {
        let position = &params.text_document_position_params;
        let (Found::Node(key), _, _) =
            self.found(&position.text_document.uri, position.position)?
        else {
            return None;
        };
        let loaded = self.loaded.as_ref()?;
        let graph = graph(&loaded.db, loaded.project);
        let place = graph.get(&key)?.place.clone()?;
        let file = loaded.file(&place.file)?;
        let text = file.text(&loaded.db);
        Some(GotoDefinitionResponse::Scalar(Location {
            uri: convert::uri_of(&loaded.root.join(&place.file))?,
            range: convert::range(text, place.start, place.end, self.encoding),
        }))
    }
}

fn invalid(id: lsp_server::RequestId, error: &serde_json::Error) -> Response {
    Response::new_err(
        id,
        ErrorCode::InvalidParams as i32,
        format!("the request's parameters aren't valid: {error}"),
    )
}

/// What the server can do, told to the editor once.
fn capabilities(encoding: Encoding) -> InitializeResult {
    InitializeResult {
        capabilities: ServerCapabilities {
            position_encoding: Some(match encoding {
                Encoding::Utf8 => PositionEncodingKind::UTF8,
                Encoding::Utf16 => PositionEncodingKind::UTF16,
            }),
            text_document_sync: Some(TextDocumentSyncCapability::Options(
                TextDocumentSyncOptions {
                    open_close: Some(true),
                    change: Some(TextDocumentSyncKind::FULL),
                    ..TextDocumentSyncOptions::default()
                },
            )),
            hover_provider: Some(HoverProviderCapability::Simple(true)),
            definition_provider: Some(OneOf::Left(true)),
            ..ServerCapabilities::default()
        },
        server_info: Some(ServerInfo {
            name: "jadpo".to_string(),
            version: Some(env!("CARGO_PKG_VERSION").to_string()),
        }),
    }
}
