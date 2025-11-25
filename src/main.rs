#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod types;
mod vjoy;
mod engine;
mod gui;
mod recorder;

fn main() -> eframe::Result<()> {
    env_logger::init();
    
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 700.0])
            // === 修改：正式版本号 ===
            .with_title("QNMDsol demo v0.1"),
        ..Default::default()
    };
    
    eframe::run_native(
        "QNMDsol",
        options,
        Box::new(|_cc| Box::new(gui::QnmdSolApp::default())),
    )
}