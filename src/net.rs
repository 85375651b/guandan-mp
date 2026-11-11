//! Newline-delimited JSON over TCP. Tailscale provides the private network.
//! Socket I/O is nonblocking and runs off the GUI thread. No browser or Node runtime.
use crate::table::{Command, Message, Peer, Table, PROTOCOL_VERSION};
use serde::{de::DeserializeOwned, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    io::{self, Read, Write},
    net::{Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError},
        Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const MAX_FRAME: usize = 65536;
const MAX_QUEUED: usize = 262144;
const HEARTBEAT: Duration = Duration::from_secs(4);
const TIMEOUT: Duration = Duration::from_secs(20);

struct Wire {
    stream: TcpStream,
    input: Vec<u8>,
    output: VecDeque<Vec<u8>>,
    offset: usize,
    last_seen: Instant,
}
impl Wire {
    fn new(stream: TcpStream) -> io::Result<Self> {
        stream.set_nonblocking(true)?;
        stream.set_nodelay(true)?;
        Ok(Self {
            stream,
            input: Vec::new(),
            output: VecDeque::new(),
            offset: 0,
            last_seen: Instant::now(),
        })
    }
    fn queue(&mut self, message: &impl Serialize) -> io::Result<()> {
        let mut data = serde_json::to_vec(message).map_err(io::Error::other)?;
        if data.len() > MAX_FRAME {
            return Err(io::Error::other("Network message is too large"));
        }
        if self.output.iter().map(Vec::len).sum::<usize>() + data.len() > MAX_QUEUED {
            return Err(io::Error::other("Connection is too slow"));
        }
        data.push(b'\n');
        self.output.push_back(data);
        Ok(())
    }
    fn flush(&mut self) -> io::Result<()> {
        while let Some(data) = self.output.front() {
            match self.stream.write(&data[self.offset..]) {
                Ok(0) => return Err(io::Error::other("Connection closed")),
                Ok(n) => {
                    self.offset += n;
                    if self.offset == data.len() {
                        self.output.pop_front();
                        self.offset = 0;
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
    fn read<T: DeserializeOwned>(&mut self) -> io::Result<Vec<T>> {
        let mut buf = [0; 4096];
        for _ in 0..8 {
            match self.stream.read(&mut buf) {
                Ok(0) => return Err(io::Error::other("Connection closed")),
                Ok(n) => {
                    self.input.extend_from_slice(&buf[..n]);
                    self.last_seen = Instant::now();
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            }
        }
        let mut messages = Vec::new();
        while let Some(end) = self.input.iter().position(|b| *b == b'\n') {
            if end > MAX_FRAME {
                return Err(io::Error::other("Oversized network frame"));
            }
            let message = serde_json::from_slice(&self.input[..end]).map_err(io::Error::other)?;
            self.input.drain(..=end);
            messages.push(message);
            if messages.len() > 64 {
                return Err(io::Error::other("Too many commands"));
            }
        }
        if self.input.len() > MAX_FRAME {
            return Err(io::Error::other("Oversized network frame"));
        }
        if self.last_seen.elapsed() > TIMEOUT {
            return Err(io::Error::other(
                "Connection timed out. Check Tailscale and reconnect.",
            ));
        }
        Ok(messages)
    }
}
impl Drop for Wire {
    fn drop(&mut self) {
        let _ = self.stream.shutdown(Shutdown::Both);
    }
}

pub struct ServerHandle {
    pub address: SocketAddr,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}
impl ServerHandle {
    pub fn start(address: SocketAddr) -> io::Result<Self> {
        let listener = TcpListener::bind(address)?;
        let address = listener.local_addr()?;
        listener.set_nonblocking(true)?;
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let worker = thread::Builder::new()
            .name("guandan-host".into())
            .spawn(move || serve(listener, flag))?;
        Ok(Self {
            address,
            stop,
            worker: Some(worker),
        })
    }
}
impl Drop for ServerHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn serve(listener: TcpListener, stop: Arc<AtomicBool>) {
    let mut table = Table::default();
    let mut clients: HashMap<Peer, Wire> = HashMap::new();
    let mut next_peer = 0;
    let mut dirty = false;
    while !stop.load(Ordering::Relaxed) {
        for _ in 0..16 {
            match listener.accept() {
                Ok((stream, _)) => {
                    if clients.len() >= 16 {
                        let _ = stream.shutdown(Shutdown::Both);
                        continue;
                    }
                    if let Ok(mut wire) = Wire::new(stream) {
                        next_peer += 1;
                        let _ = wire.queue(&Message::State(Box::new(table.view(next_peer))));
                        clients.insert(next_peer, wire);
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(_) => break,
            }
        }
        let mut gone = Vec::new();
        for (&id, wire) in &mut clients {
            match wire.read::<Command>() {
                Ok(commands) => {
                    for command in commands {
                        if matches!(command, Command::Ping) {
                            if wire.queue(&Message::Pong).is_err() {
                                gone.push(id);
                            }
                            continue;
                        }
                        match table.apply(id, command, Instant::now()) {
                            Ok(()) => dirty = true,
                            Err(error) => {
                                if wire.queue(&Message::Error(error)).is_err() {
                                    gone.push(id);
                                }
                            }
                        }
                    }
                }
                Err(_) => gone.push(id),
            }
        }
        for id in gone.drain(..) {
            clients.remove(&id);
            dirty |= table.disconnect(id, Instant::now());
        }
        dirty |= table.tick(Instant::now());
        if dirty {
            for (&id, wire) in &mut clients {
                if wire
                    .queue(&Message::State(Box::new(table.view(id))))
                    .is_err()
                {
                    gone.push(id);
                }
            }
            dirty = false;
        }
        for (&id, wire) in &mut clients {
            if wire.flush().is_err() {
                gone.push(id);
            }
        }
        for id in gone {
            clients.remove(&id);
            dirty |= table.disconnect(id, Instant::now());
        }
        thread::sleep(Duration::from_millis(10));
    }
}

#[derive(Debug)]
pub enum Event {
    Connected,
    Message(Message),
    Disconnected(String),
}

pub struct Connection {
    sender: SyncSender<Command>,
    pub events: Receiver<Event>,
    stop: Arc<AtomicBool>,
}
impl Connection {
    pub fn connect(address: String, name: String) -> Self {
        let (sender, commands) = mpsc::sync_channel(32);
        let (events, receiver) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        thread::spawn(move || {
            let result = (|| -> io::Result<()> {
                let address = normalize_address(&address)?;
                let addresses: Vec<_> = address.to_socket_addrs()?.collect();
                let mut connected = None;
                for addr in addresses.into_iter().take(4) {
                    if flag.load(Ordering::Relaxed) {
                        return Ok(());
                    }
                    if let Ok(stream) = TcpStream::connect_timeout(&addr, Duration::from_secs(3)) {
                        connected = Some(stream);
                        break;
                    }
                }
                let mut wire=Wire::new(connected.ok_or_else(||io::Error::other("Could not reach the host. Check its Tailscale address, port, and that the game is running."))?)?;
                wire.queue(&Command::Join {
                    version: PROTOCOL_VERSION,
                    name,
                })?;
                let _ = events.send(Event::Connected);
                let mut heartbeat = Instant::now();
                while !flag.load(Ordering::Relaxed) {
                    for _ in 0..32 {
                        match commands.try_recv() {
                            Ok(command) => wire.queue(&command)?,
                            Err(TryRecvError::Empty) => break,
                            Err(TryRecvError::Disconnected) => return Ok(()),
                        }
                    }
                    if heartbeat.elapsed() >= HEARTBEAT {
                        wire.queue(&Command::Ping)?;
                        heartbeat = Instant::now();
                    }
                    wire.flush()?;
                    for message in wire.read::<Message>()? {
                        if !matches!(message, Message::Pong)
                            && events.send(Event::Message(message)).is_err()
                        {
                            return Ok(());
                        }
                    }
                    thread::sleep(Duration::from_millis(10));
                }
                Ok(())
            })();
            if let Err(error) = result {
                let _ = events.send(Event::Disconnected(error.to_string()));
            }
        });
        Self {
            sender,
            events: receiver,
            stop,
        }
    }
    pub fn send(&self, command: Command) -> Result<(), String> {
        self.sender
            .try_send(command)
            .map_err(|_| "Connection is busy or closed. Reconnect to the table.".into())
    }
}
impl Drop for Connection {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

pub fn normalize_address(address: &str) -> io::Result<String> {
    let raw = address
        .trim()
        .strip_prefix("guandan://")
        .unwrap_or(address.trim())
        .trim_end_matches('/');
    if raw.is_empty() || raw.contains('/') || raw.chars().any(char::is_whitespace) {
        return Err(io::Error::other(
            "Enter a Tailscale IP or hostname, optionally followed by :3000.",
        ));
    }
    if raw.parse::<std::net::IpAddr>().is_ok() && raw.contains(':') {
        return Ok(format!("[{raw}]:3000"));
    }
    if raw.contains(':') {
        Ok(raw.into())
    } else {
        Ok(format!("{raw}:3000"))
    }
}
