pub mod archive;
pub mod textures;
pub mod xml;

use crate::AppWindow;
use crate::logger::UiLogger;
use slint::{ComponentHandle, SharedString};
use std::collections::VecDeque;
use std::sync::mpsc;
use std::thread;

pub fn run_gui() -> Result<(), slint::PlatformError> {
    let ui = AppWindow::new()?;
    let ui_handle = ui.as_weak();

    ui.set_log_text("System Ready.\n".into());
    ui.set_status_msg("Ready.".into());

    let (log_tx, log_rx) = mpsc::channel::<String>();
    let logger = UiLogger::new(log_tx);

    let ui_weak_log = ui_handle.clone();
    thread::spawn(move || {
        let mut logs = VecDeque::with_capacity(300);
        while let Ok(msg) = log_rx.recv() {
            logs.push_back(msg.clone());
            let mut last = msg.trim().to_string();
            while let Ok(m) = log_rx.try_recv() {
                last = m.trim().to_string();
                logs.push_back(m);
            }
            while logs.len() > 250 {
                logs.pop_front();
            }
            let combined = logs.iter().cloned().collect::<String>();
            let _ = ui_weak_log.upgrade_in_event_loop(move |ui| {
                ui.set_log_text(combined.into());
                ui.set_status_msg(last.into());
            });
            thread::sleep(std::time::Duration::from_millis(50));
        }
    });

    // File pickers
    ui.on_browse_file(|ext| {
        let ext_str = ext.as_str();
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Target", &[ext_str])
            .pick_file()
        {
            SharedString::from(path.to_string_lossy().into_owned())
        } else {
            SharedString::new()
        }
    });

    ui.on_browse_folder(|| {
        if let Some(path) = rfd::FileDialog::new().pick_folder() {
            SharedString::from(path.to_string_lossy().into_owned())
        } else {
            SharedString::new()
        }
    });

    ui.on_save_file(|ext| {
        let ext_str = ext.as_str();
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Save", &[ext_str])
            .save_file()
        {
            SharedString::from(path.to_string_lossy().into_owned())
        } else {
            SharedString::new()
        }
    });

    archive::register_archive_callbacks(&ui, logger.clone());
    textures::register_textures_callbacks(&ui, logger.clone());
    xml::register_xml_callbacks(&ui, logger);

    ui.run()
}
