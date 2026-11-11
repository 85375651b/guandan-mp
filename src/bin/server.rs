use guandan_desktop::net::ServerHandle;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bind = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "0.0.0.0:3000".into());
    let host = ServerHandle::start(bind.parse()?)?;
    println!(
        "Guandan native-protocol host on {}. Join from the desktop app. Ctrl+C stops it.",
        host.address
    );
    loop {
        std::thread::park();
    }
}
