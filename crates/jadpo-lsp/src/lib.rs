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
//! disagree. Code actions: "Make this a setting" on a constant runs the
//! code `jadpo config move` runs (chunk 5.3f), and "Decline with a reason"
//! on a declinable finding and "Remove decline" on a `decline` line run the
//! code `jadpo declines` runs (chunk 5.4b); "Swap for the standard ..." on
//! `type.standard_clash` and `type.standard_exists` runs the code `jadpo
//! fix` runs (chunk 5.5a2). Completion (chunk 5.5a3) offers what
//! `jadpo_check::completion` reads from the same tables and the checked
//! body at the cursor. Find references resolves every candidate name as go
//! to definition does (`jadpo_check::graph::references`); formatting is
//! `jadpo fmt`'s (`jadpo_check::formatting`); every error whose next step
//! carries edits is a quick fix, promotion made as `jadpo fmt --layout`
//! makes it; and a query's hover shows the SQL `jadpo sql` prints. Rename
//! waits for `jadpo rename` (area 14), and checking runs between messages
//! rather than in the background with salsa's cancellation.

mod completion;
mod convert;
mod declines;
mod fixes;
mod hover;
mod make_setting;
mod quick_fixes;
mod server;
mod sql;

use std::path::Path;

use lsp_server::{Connection, Message};

pub use convert::Encoding;
pub use declines::{DECLINE, REMOVE as REMOVE_DECLINE};
pub use fixes::SWAP;
pub use make_setting::TITLE as MAKE_SETTING;
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
