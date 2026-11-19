mod theme;
mod ui;
#[cfg(test)]
mod native_capture;

fn main() -> eframe::Result {
    match std::env::args().nth(1).as_deref() {
        Some("--help") | Some("-h") => { println!("Imdev — native HTTP workspace\nUsage: imdev [--help | --headless-ui-check]"); return Ok(()); }
        Some("--headless-ui-check") => {
            if ui::headless_check().is_err() { eprintln!("Native UI validation failed"); std::process::exit(1); }
            println!("Native UI validation passed (headless frame, dark theme, typography)");
            return Ok(());
        }
        Some(_) => { eprintln!("Unknown argument; use --help"); std::process::exit(2); }
        None => {}
    }
    eframe::run_native("Imdev", eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default().with_inner_size([1200.0, 850.0]).with_min_inner_size([760.0, 580.0]),
        ..Default::default()
    }, Box::new(|cc| Ok(Box::new(ui::ImdevApp::new(&cc.egui_ctx)))))
}
