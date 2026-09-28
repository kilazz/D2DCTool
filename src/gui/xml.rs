use crate::AppWindow;
use crate::diff;
use crate::hash_scanner;
use crate::logger::UiLogger;
use crate::xml_converter;
use slint::ComponentHandle;
use std::collections::HashMap;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;

pub fn register_xml_callbacks(ui: &AppWindow, logger: UiLogger) {
    let mut hash_to_string = HashMap::new();
    let dict_candidates = [
        Path::new("xml_hashes/hash_dictionary.txt"),
        Path::new("hash_dictionary.txt"),
    ];

    for dict_path in &dict_candidates {
        if dict_path.exists()
            && let Ok(file) = std::fs::File::open(dict_path)
        {
            let reader = std::io::BufReader::new(file);
            for line in reader.lines().map_while(Result::ok) {
                let trimmed = line.trim().to_string();
                if !trimmed.is_empty() {
                    let h = xml_converter::get_hash_value(&trimmed);
                    hash_to_string.insert(h, trimmed);
                }
            }
            break;
        }
    }

    let hash_db = Arc::new(Mutex::new(hash_to_string));

    let log_diff = logger.clone();
    ui.on_create_diff_patch(move |base, mod_dir, out_patch| {
        let log = log_diff.clone();
        let b = PathBuf::from(base.as_str());
        let m = PathBuf::from(mod_dir.as_str());
        let p = PathBuf::from(out_patch.as_str());
        thread::spawn(move || {
            if let Err(e) = diff::create_diff(&b, &m, &p, &log) {
                log.log(&format!("[!] Diff Error: {}", e));
            }
        });
    });

    let log_patch = logger.clone();
    ui.on_apply_diff_patch(move |target, patch_file| {
        let log = log_patch.clone();
        let t = PathBuf::from(target.as_str());
        let p = PathBuf::from(patch_file.as_str());
        thread::spawn(move || {
            if let Err(e) = diff::apply_patch(&t, &p, &log) {
                log.log(&format!("[!] Patch Error: {}", e));
            }
        });
    });

    let log_extract = logger.clone();
    let hashes_ext = hash_db.clone();
    let ui_w_ext = ui.as_weak();
    ui.on_extract_binary_xml(move |binary_xml, text_xml| {
        let log = log_extract.clone();
        let b = PathBuf::from(binary_xml.as_str());
        let t = PathBuf::from(text_xml.as_str());
        let hashes = hashes_ext.lock().unwrap().clone();
        let ui_w = ui_w_ext.clone();

        thread::spawn(move || {
            log.log(&format!(
                "[*] Extracting binary XML: {:?}",
                b.file_name().unwrap_or_default()
            ));
            if let Err(e) = xml_converter::extract_binary_xml_to_real_xml(&b, &t, &hashes) {
                log.log(&format!("[!] XML Extract Error: {}", e));
            } else {
                log.log("[+] Text XML generated successfully.");
                if let Ok(content) = std::fs::read_to_string(&t) {
                    let _ = ui_w.upgrade_in_event_loop(move |ui| {
                        ui.set_xml_preview_content(content.into());
                    });
                }
            }
        });
    });

    let log_repack = logger.clone();
    ui.on_repack_readable_xml(move |orig_xml, text_xml, out_xml| {
        let log = log_repack.clone();
        let o = PathBuf::from(orig_xml.as_str());
        let t = PathBuf::from(text_xml.as_str());
        let out = PathBuf::from(out_xml.as_str());
        thread::spawn(move || {
            log.log("[*] Repacking text XML into Gamebryo binary NIF format...");
            if let Err(e) = xml_converter::repack_real_xml_to_binary_xml(&o, &t, &out) {
                log.log(&format!("[!] XML Repack Error: {}", e));
            } else {
                log.log("[+] Binary XML recompiled successfully.");
            }
        });
    });

    let log_scan = logger.clone();
    ui.on_scan_hashes(move |xml_dir| {
        let log = log_scan.clone();
        let p = PathBuf::from(xml_dir.as_str());
        thread::spawn(move || {
            log.log("[*] Scanning XML files for unknown hashes...");
            if let Err(e) = hash_scanner::scan_and_verify(&p, |msg| log.log(msg)) {
                log.log(&format!("[!] Scanner Error: {}", e));
            }
        });
    });

    let log_bext = logger.clone();
    let hashes_bext = hash_db;
    ui.on_batch_extract_xml(move |folder| {
        let log = log_bext.clone();
        let p = PathBuf::from(folder.as_str());
        let hashes = hashes_bext.lock().unwrap().clone();
        thread::spawn(move || {
            let files: Vec<_> = walkdir::WalkDir::new(&p)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|entry| {
                    entry.path().is_file()
                        && entry.path().extension().and_then(|s| s.to_str()) == Some("xml")
                        && !entry.path().to_string_lossy().ends_with(".txt.xml")
                })
                .collect();

            log.log(&format!("[*] Found {} XML files to extract.", files.len()));
            let mut success = 0;
            for (i, entry) in files.iter().enumerate() {
                let out_txt_xml = entry.path().with_extension("txt.xml");
                if xml_converter::extract_binary_xml_to_real_xml(
                    entry.path(),
                    &out_txt_xml,
                    &hashes,
                )
                .is_ok()
                {
                    success += 1;
                }
                if (i + 1) % 100 == 0 || i + 1 == files.len() {
                    log.log(&format!("Progress: {}/{}", i + 1, files.len()));
                }
            }
            log.log(&format!(
                "[+] Batch extraction complete: {}/{} files.",
                success,
                files.len()
            ));
        });
    });

    let log_brep = logger;
    ui.on_batch_repack_xml(move |folder| {
        let log = log_brep.clone();
        let p = PathBuf::from(folder.as_str());
        thread::spawn(move || {
            let txt_files: Vec<_> = walkdir::WalkDir::new(&p)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|entry| {
                    entry.path().is_file() && entry.path().to_string_lossy().ends_with(".txt.xml")
                })
                .collect();

            if txt_files.is_empty() {
                log.log("[!] No .txt.xml files found to repack.");
                return;
            }

            let out_dir = p.join("Repacked_XML");
            let _ = std::fs::create_dir_all(&out_dir);
            let mut success = 0;

            for entry in &txt_files {
                let file_name = entry
                    .path()
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy();
                let clean_name = file_name.trim_end_matches(".txt.xml");
                let orig_name = format!("{}.xml", clean_name);
                let orig_path = entry.path().parent().unwrap_or(&p).join(&orig_name);
                let out_path = out_dir.join(&orig_name);

                if orig_path.exists()
                    && xml_converter::repack_real_xml_to_binary_xml(
                        &orig_path,
                        entry.path(),
                        &out_path,
                    )
                    .is_ok()
                {
                    success += 1;
                }
            }
            log.log(&format!(
                "[+] Batch repack finished: {}/{} repacked.",
                success,
                txt_files.len()
            ));
        });
    });
}
