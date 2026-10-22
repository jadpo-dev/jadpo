//! A transaction-mode PostgreSQL pooler standing in for Hyperdrive under
//! workerd (tools/bench/README.md). On Cloudflare a Worker opens a
//! connection to Hyperdrive for each request and Hyperdrive answers its
//! start-up itself, handing its statements to connections to the database
//! it keeps open; workerd's own Hyperdrive binding instead hands the
//! Worker's `connect()` straight to the database, which starts a backend
//! process for every request. This does what Hyperdrive does, enough for
//! the measurement: it accepts a client's start-up without a password
//! (the database trusts this machine), lends it a connection from the
//! pool from its first message, and takes the connection back once every
//! `Sync` or simple query it was sent is answered and the server is idle
//! outside a transaction. No TLS, no cancelling, no caching.
//!
//! `pool --listen 127.0.0.1:P --server 127.0.0.1:Q --user U --database D --size N`

use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// A stream read a whole message at a time, cancel-safely: what has been
/// read stays in `buf` until a message is complete.
struct Framed {
    stream: TcpStream,
    buf: Vec<u8>,
}

impl Framed {
    fn new(stream: TcpStream) -> Framed {
        Framed {
            stream,
            buf: Vec::with_capacity(8192),
        }
    }

    /// The next whole message, its type byte first, or `None` at the end.
    async fn message(&mut self) -> Result<Option<Vec<u8>>> {
        loop {
            if self.buf.len() >= 5 {
                let len = u32::from_be_bytes([self.buf[1], self.buf[2], self.buf[3], self.buf[4]]) as usize;
                if self.buf.len() > len {
                    return Ok(Some(self.buf.drain(..=len).collect()));
                }
            }
            if self.stream.read_buf(&mut self.buf).await? == 0 {
                return Ok(None);
            }
        }
    }

    /// The start-up packet, which has no type byte.
    async fn startup(&mut self) -> Result<Option<Vec<u8>>> {
        loop {
            if self.buf.len() >= 4 {
                let len = u32::from_be_bytes([self.buf[0], self.buf[1], self.buf[2], self.buf[3]]) as usize;
                if self.buf.len() >= len {
                    return Ok(Some(self.buf.drain(..len).collect()));
                }
            }
            if self.stream.read_buf(&mut self.buf).await? == 0 {
                return Ok(None);
            }
        }
    }
}

struct Pool {
    server: String,
    user: String,
    database: String,
    idle: Mutex<Vec<Framed>>,
    slots: Arc<Semaphore>,
    /// The server's ParameterStatus messages, replayed to each client.
    parameters: Mutex<Vec<u8>>,
}

impl Pool {
    async fn open(&self) -> Result<Framed> {
        let stream = TcpStream::connect(&self.server).await?;
        stream.set_nodelay(true)?;
        let mut body = Vec::new();
        body.extend_from_slice(&196_608u32.to_be_bytes());
        for (k, v) in [("user", &self.user), ("database", &self.database)] {
            body.extend_from_slice(k.as_bytes());
            body.push(0);
            body.extend_from_slice(v.as_bytes());
            body.push(0);
        }
        body.push(0);
        let mut packet = ((body.len() + 4) as u32).to_be_bytes().to_vec();
        packet.extend_from_slice(&body);
        let mut server = Framed::new(stream);
        server.stream.write_all(&packet).await?;
        let mut parameters = Vec::new();
        loop {
            let message = server.message().await?.ok_or("the server closed during start-up")?;
            match message[0] {
                b'R' if message[5..9] == [0, 0, 0, 0] => {}
                b'R' => return Err("the server asked for a password: start it with trust".into()),
                b'S' => parameters.extend_from_slice(&message),
                b'K' => {}
                b'Z' => break,
                b'E' => return Err("the server refused the start-up".into()),
                _ => {}
            }
        }
        let mut known = self.parameters.lock().expect("lock");
        if known.is_empty() {
            *known = parameters;
        }
        Ok(server)
    }

    async fn lend(self: &Arc<Self>) -> Result<(Framed, OwnedSemaphorePermit)> {
        let permit = self.slots.clone().acquire_owned().await?;
        let idle = self.idle.lock().expect("lock").pop();
        let server = match idle {
            Some(server) => server,
            None => self.open().await?,
        };
        Ok((server, permit))
    }

    fn give_back(&self, server: Framed) {
        self.idle.lock().expect("lock").push(server);
    }
}

async fn session(pool: Arc<Pool>, client: TcpStream) -> Result<()> {
    client.set_nodelay(true)?;
    let mut client = Framed::new(client);
    // SSLRequest and GSSENCRequest are answered `N`; then the start-up.
    loop {
        let Some(packet) = client.startup().await? else {
            return Ok(());
        };
        let code = u32::from_be_bytes([packet[4], packet[5], packet[6], packet[7]]);
        match code {
            80_877_103 | 80_877_104 => client.stream.write_all(b"N").await?,
            196_608 => break,
            _ => return Ok(()),
        }
    }
    let mut hello = vec![b'R', 0, 0, 0, 8, 0, 0, 0, 0];
    hello.extend_from_slice(&pool.parameters.lock().expect("lock"));
    hello.extend_from_slice(&[b'K', 0, 0, 0, 12, 0, 0, 0, 1, 0, 0, 0, 1]);
    hello.extend_from_slice(&[b'Z', 0, 0, 0, 5, b'I']);
    client.stream.write_all(&hello).await?;

    let mut lent: Option<(Framed, OwnedSemaphorePermit)> = None;
    let mut waiting = 0usize;
    loop {
        match lent.as_mut() {
            None => {
                let Some(message) = client.message().await? else {
                    return Ok(());
                };
                if message[0] == b'X' {
                    return Ok(());
                }
                let (mut server, permit) = pool.lend().await?;
                if matches!(message[0], b'S' | b'Q') {
                    waiting += 1;
                }
                server.stream.write_all(&message).await?;
                lent = Some((server, permit));
            }
            Some((server, _)) => {
                tokio::select! {
                    from_client = client.message() => {
                        let Some(message) = from_client? else {
                            // The client went away mid-transaction: the
                            // connection is dropped, not reused.
                            return Ok(());
                        };
                        if message[0] == b'X' {
                            return Ok(());
                        }
                        if matches!(message[0], b'S' | b'Q') {
                            waiting += 1;
                        }
                        server.stream.write_all(&message).await?;
                    }
                    from_server = server.message() => {
                        let message = from_server?.ok_or("the server closed")?;
                        client.stream.write_all(&message).await?;
                        if message[0] == b'Z' {
                            waiting = waiting.saturating_sub(1);
                            if waiting == 0 && message[5] == b'I' {
                                let (server, permit) = lent.take().expect("lent");
                                pool.give_back(server);
                                drop(permit);
                            }
                        }
                    }
                }
            }
        }
    }
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let (mut listen, mut server, mut user, mut database, mut size) =
        (String::new(), String::new(), String::new(), String::new(), 32usize);
    while let Some(flag) = args.next() {
        let value = args.next().ok_or("a flag without a value")?;
        match flag.as_str() {
            "--listen" => listen = value,
            "--server" => server = value,
            "--user" => user = value,
            "--database" => database = value,
            "--size" => size = value.parse()?,
            _ => return Err(format!("unknown flag {flag}").into()),
        }
    }
    let pool = Arc::new(Pool {
        server,
        user,
        database,
        idle: Mutex::new(Vec::new()),
        slots: Arc::new(Semaphore::new(size)),
        parameters: Mutex::new(Vec::new()),
    });
    // One connection first, so its parameters are known before a client.
    let first = pool.open().await?;
    pool.give_back(first);
    let listener = TcpListener::bind(&listen).await?;
    println!("pool listening on {}", listener.local_addr()?);
    loop {
        let (client, _) = listener.accept().await?;
        let pool = pool.clone();
        tokio::spawn(async move {
            if let Err(error) = session(pool, client).await {
                eprintln!("pool: a session ended: {error}");
            }
        });
    }
}
