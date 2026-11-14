use guandan_desktop::net::ServerHandle;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bind = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "0.0.0.0:3000".into());
    let host = ServerHandle::start(bind.parse()?)?;
    println!(
        "Guandan native-protocol host on {}. Join from the desktop app. Ctrl+C stops it. / 掼蛋房主已启动，请在桌面程序中输入该地址加入，按 Ctrl+C 停止。",
        host.address
    );
    loop {
        std::thread::park();
    }
}
