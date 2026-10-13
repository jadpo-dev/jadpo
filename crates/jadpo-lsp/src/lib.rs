//! The language server (`jadpo lsp`, area 14 "Editor support"): the
//! compiler's own view for any editor, over the Language Server Protocol
//! on standard input and output.
//!
//! It gives the diagnostics `jadpo check` gives, from the same queries on
//! one database kept for the server's life, so an edit recomputes only
//! what it changes; hover with a declaration's signature or definition,
//! its `///` description, the failures a callable can produce and who
//! calls or uses it; and go to definition. Everything is read from the
//! graph the manifest is written from, so the editor and the build never
//! disagree. Completion, references, rename, formatting and quick fixes
//! come later (area 14), and checking runs between messages rather than
//! in the background with salsa's cancellation.

mod convert;
mod hover;
mod server;

use std::path::Path;

use lsp_server::{Connection, Message};

pub use convert::Encoding;
pub use server::Server;

/// Serves one editor over `connection` until it asks to exit.
pub fn serve(connection: &Connection, cwd: &Path) -> Result<(), String> {
    let (id, params) = connection
        .initialize_start()
        .map_err(|error| error.to_string())?;
    let params: lsp_types::InitializeParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let (mut server, result) = Server::initialize(&params, cwd);
    let result = serde_json::to_value(result).map_err(|error| error.to_string())?;
    // Waits for the editor's `initialized`.
    connection
        .initialize_finish(id, result)
        .map_err(|error| error.to_string())?;
    let send = |messages: Vec<Message>| -> Result<(), String> {
        for message in messages {
            connection
                .sender
                .send(message)
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    };
    send(server.started())?;
    send(server.flush())?;
    for message in &connection.receiver {
        if let Message::Request(request) = &message
            && connection
                .handle_shutdown(request)
                .map_err(|error| error.to_string())?
        {
            return Ok(());
        }
        send(server.handle(message))?;
        // Check once a burst of edits has been read, not after each key.
        if connection.receiver.is_empty() {
            send(server.flush())?;
        }
    }
    Ok(())
}

/// Serves the editor on standard input and output.
pub fn stdio(cwd: &Path) -> Result<(), String> {
    let (connection, threads) = Connection::stdio();
    serve(&connection, cwd)?;
    drop(connection);
    threads.join().map_err(|error| error.to_string())
}
