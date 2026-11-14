fn main() -> eframe::Result {
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icons/guandan.png"))
        .expect("the bundled Guandan icon must be a valid PNG");
    let initial_size = [1000.0, 700.0];
    // Reproduce small windows in optional screenshot builds without changing
    // normal launch settings or resizing a live desktop window by hand.
    #[cfg(feature = "screenshot")]
    let initial_size = std::env::var("GUANDAN_SCREENSHOT_SIZE")
        .ok()
        .and_then(|size| {
            let (w, h) = size.split_once('x')?;
            Some([w.parse::<f32>().ok()?, h.parse::<f32>().ok()?])
        })
        .filter(|size| size.iter().all(|n| n.is_finite()))
        .map(|[w, h]| [w.max(420.0), h.max(340.0)])
        .unwrap_or(initial_size);
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Guandan 掼蛋 · Native Table 原生牌桌")
            .with_app_id("guandan")
            .with_icon(icon)
            .with_inner_size(initial_size)
            .with_min_inner_size([420.0, 340.0])
            .with_clamp_size_to_monitor_size(true),
        ..Default::default()
    };
    let demo = std::env::args().any(|a| a == "--demo");
    eframe::run_native(
        "guandan-desktop",
        options,
        Box::new(move |cc| Ok(Box::new(guandan_desktop::app::GuandanApp::new(cc, demo)))),
    )
}
