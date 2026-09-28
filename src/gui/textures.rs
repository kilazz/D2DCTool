use crate::AppWindow;
use crate::dds;
use crate::logger::UiLogger;
use byteorder::{LittleEndian, ReadBytesExt};
use rayon::prelude::*;
use slint::{ComponentHandle, Image};
use std::io::{Cursor, Read};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;

struct PreviewTask {
    generation: u64,
    path: PathBuf,
}

pub fn register_textures_callbacks(ui: &AppWindow, logger: UiLogger) {
    let (preview_tx, preview_rx) = mpsc::channel::<PreviewTask>();
    let preview_generation = Arc::new(AtomicU64::new(0));

    let preview_gen_worker = preview_generation.clone();
    let ui_w_worker = ui.as_weak();

    thread::spawn(move || {
        while let Ok(mut task) = preview_rx.recv() {
            while let Ok(newer) = preview_rx.try_recv() {
                task = newer;
            }

            if task.generation != preview_gen_worker.load(Ordering::SeqCst) {
                continue;
            }

            let res = decode_texture_preview(&task.path);

            if task.generation == preview_gen_worker.load(Ordering::SeqCst) {
                let ui_w = ui_w_worker.clone();
                let _ = ui_w.upgrade_in_event_loop(move |ui| {
                    if let Some((w, h, rgba)) = res {
                        ui.set_preview_texture(dds::raw_rgba_to_slint(w, h, &rgba));
                        ui.set_preview_info(format!("{}x{} Texture", w, h).into());
                    } else {
                        ui.set_preview_texture(Image::default());
                        ui.set_preview_info("No texture preview available.".into());
                    }
                });
            }
        }
    });

    let p_tx = preview_tx.clone();
    let p_gen = preview_generation;
    ui.on_request_texture_preview(move |path_str| {
        let p = PathBuf::from(path_str.as_str());
        let task_gen = p_gen.fetch_add(1, Ordering::SeqCst) + 1;
        let _ = p_tx.send(PreviewTask {
            generation: task_gen,
            path: p,
        });
    });

    let log_repair = logger.clone();
    ui.on_repair_dds_file(move |dds_path| {
        let log = log_repair.clone();
        let p = PathBuf::from(dds_path.as_str());
        thread::spawn(move || match dds::repair_single_dds(&p) {
            Ok(true) => log.log(&format!("[+] Successfully repaired DDS header: {:?}", p)),
            Ok(false) => log.log("[*] DDS header is valid or unmodified."),
            Err(e) => log.log(&format!("[!] Error repairing DDS: {}", e)),
        });
    });

    let log_brepair = logger.clone();
    ui.on_batch_repair_dds(move |folder| {
        let log = log_brepair.clone();
        let p = PathBuf::from(folder.as_str());
        thread::spawn(
            move || match dds::batch_repair_dds(&p, |msg| log.log(msg)) {
                Ok(n) => log.log(&format!("[+] Batch repair complete. Fixed {} headers.", n)),
                Err(e) => log.log(&format!("[!] Batch repair error: {}", e)),
            },
        );
    });

    let log_conv = logger.clone();
    ui.on_convert_dds_to_nif(move |dds_path, nif_path| {
        let log = log_conv.clone();
        let d = PathBuf::from(dds_path.as_str());
        let n = PathBuf::from(nif_path.as_str());
        thread::spawn(move || {
            if let Err(e) = dds::convert_dds_to_nif(&d, &n, |msg| log.log(msg)) {
                log.log(&format!("[!] DDS Conversion Error: {}", e));
            }
        });
    });

    let log_ext = logger.clone();
    ui.on_extract_nif_to_dds(move |nif_path, dds_path| {
        let log = log_ext.clone();
        let n = PathBuf::from(nif_path.as_str());
        let d = PathBuf::from(dds_path.as_str());
        thread::spawn(
            move || match dds::extract_nif_to_dds(&n, &d, |msg| log.log(msg)) {
                Ok(true) => log.log("[+] Texture extracted successfully."),
                Ok(false) => {
                    log.log("[!] NIF does not contain NiPersistentSrcTextureRendererData.")
                }
                Err(e) => log.log(&format!("[!] Extraction Error: {}", e)),
            },
        );
    });

    // Parallel batch DDS -> NIF
    let log_bconv = logger.clone();
    ui.on_batch_dds_to_nif(move |folder| {
        let log = log_bconv.clone();
        let p = PathBuf::from(folder.as_str());
        thread::spawn(move || {
            let files: Vec<_> = walkdir::WalkDir::new(&p)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|entry| {
                    entry.path().is_file()
                        && entry
                            .path()
                            .extension()
                            .and_then(|s| s.to_str())
                            .map(|ext| ext.eq_ignore_ascii_case("dds"))
                            .unwrap_or(false)
                })
                .collect();

            let total = files.len();
            log.log(&format!(
                "[*] Converting {} DDS files in parallel on all CPU cores...",
                total
            ));
            let success_count = AtomicUsize::new(0);

            files.par_iter().for_each(|entry| {
                let out_nif = entry.path().with_extension("nif");
                if dds::convert_dds_to_nif(entry.path(), &out_nif, |_| {}).is_ok() {
                    success_count.fetch_add(1, Ordering::Relaxed);
                }
            });

            log.log(&format!(
                "[+] Batch conversion finished: {}/{} converted.",
                success_count.load(Ordering::Relaxed),
                total
            ));
        });
    });

    // Parallel batch NIF -> DDS
    let log_bext = logger;
    ui.on_batch_nif_to_dds(move |folder| {
        let log = log_bext.clone();
        let p = PathBuf::from(folder.as_str());
        thread::spawn(move || {
            let files: Vec<_> = walkdir::WalkDir::new(&p)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|entry| {
                    entry.path().is_file()
                        && entry
                            .path()
                            .extension()
                            .and_then(|s| s.to_str())
                            .map(|ext| ext.eq_ignore_ascii_case("nif"))
                            .unwrap_or(false)
                })
                .collect();

            let total = files.len();
            log.log(&format!(
                "[*] Extracting {} NIF files in parallel on all CPU cores...",
                total
            ));
            let success_count = AtomicUsize::new(0);

            files.par_iter().for_each(|entry| {
                let out_dds = entry.path().with_extension("dds");
                if let Ok(true) = dds::extract_nif_to_dds(entry.path(), &out_dds, |_| {}) {
                    success_count.fetch_add(1, Ordering::Relaxed);
                }
            });

            log.log(&format!(
                "[+] Batch extraction finished: {} textures extracted.",
                success_count.load(Ordering::Relaxed)
            ));
        });
    });
}

fn decode_texture_preview(path: &std::path::Path) -> Option<(u32, u32, Vec<u8>)> {
    let data = std::fs::read(path).ok()?;
    if data.starts_with(b"DDS ") && data.len() >= 128 {
        let mut cur = Cursor::new(&data);
        cur.set_position(12);
        let h = cur.read_u32::<LittleEndian>().ok()?;
        let w = cur.read_u32::<LittleEndian>().ok()?;
        cur.set_position(84);
        let mut four_cc = [0u8; 4];
        cur.read_exact(&mut four_cc).ok()?;

        let rgba = dds::decode_dxt_to_rgba(w, h, &data[128..], &four_cc);
        Some((w, h, rgba))
    } else {
        None
    }
}
