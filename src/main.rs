slint::include_modules!();

mod dds_converter;
mod dv2_archive;
mod hash_scanner;
mod pak_tree;
mod xml_converter;

use slint::{ModelRc, SharedString, StandardListViewItem, VecModel};
use std::collections::{HashMap, VecDeque};
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex, mpsc};
use std::thread;

#[derive(Clone)]
pub struct UiLogger {
    sender: mpsc::Sender<String>,
}

impl UiLogger {
    pub fn log(&self, msg: &str) {
        let _ = self.sender.send(format!("{}\n", msg));
    }
}

fn main() -> Result<(), slint::PlatformError> {
    let ui = AppWindow::new()?;
    let ui_handle = ui.as_weak();

    ui.set_log_text("System Ready.\n".into());
    ui.set_status_msg("Ready.".into());

    let (log_tx, log_rx) = mpsc::channel::<String>();
    let logger_base = UiLogger { sender: log_tx };

    let ui_weak_log = ui_handle.clone();
    thread::spawn(move || {
        let mut logs = VecDeque::with_capacity(300);
        while let Ok(msg) = log_rx.recv() {
            logs.push_back(msg);
            while let Ok(m) = log_rx.try_recv() {
                logs.push_back(m);
            }
            while logs.len() > 250 {
                logs.pop_front();
            }
            let combined = logs.iter().cloned().collect::<String>();
            let _ = ui_weak_log.upgrade_in_event_loop(move |ui| {
                ui.set_log_text(combined.into());
            });
            thread::sleep(std::time::Duration::from_millis(60));
        }
    });

    let mut hash_to_string = HashMap::new();
    if Path::new("hash_dictionary.txt").exists()
        && let Ok(file) = std::fs::File::open("hash_dictionary.txt")
    {
        let reader = std::io::BufReader::new(file);
        for line in reader.lines().map_while(Result::ok) {
            let trimmed = line.trim().to_string();
            if !trimmed.is_empty() {
                let h = xml_converter::get_hash_value(&trimmed);
                hash_to_string.insert(h, trimmed);
            }
        }
    }
    let hash_db = Arc::new(Mutex::new(hash_to_string));
    let tree_items_state = Arc::new(Mutex::new(Vec::<pak_tree::TreeItem>::new()));

    let ui_weak_browse = ui_handle.clone();
    let tree_items_browse = tree_items_state.clone();
    ui.on_browse_file(move |ext| {
        let ext_str = ext.as_str();
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Game Resource", &[ext_str])
            .pick_file()
        {
            let path_str = path.to_string_lossy().into_owned();
            let ui_weak = ui_weak_browse.clone();
            let tree_state = tree_items_browse.clone();
            let ext_clone = ext_str.to_string();

            thread::spawn(move || {
                if ext_clone == "dv2"
                    && let Ok(entries) = dv2_archive::read_entries(&path)
                {
                    let file_paths: Vec<String> = entries.into_iter().map(|e| e.name).collect();
                    let items = pak_tree::generate_tree_items(&file_paths);
                    let tree_strings = pak_tree::get_visible_tree_nodes(&items);

                    *tree_state.lock().unwrap() = items;

                    let list_items: Vec<_> = tree_strings
                        .into_iter()
                        .map(|s| StandardListViewItem::from(SharedString::from(s)))
                        .collect();

                    let _ = ui_weak.upgrade_in_event_loop(move |ui| {
                        let slint_model = ModelRc::from(Rc::new(VecModel::from(list_items)));
                        ui.set_archive_files(slint_model);
                    });
                }
            });
            SharedString::from(path_str)
        } else {
            SharedString::new()
        }
    });

    let ui_weak_folder = ui_handle.clone();
    let tree_items_folder = tree_items_state.clone();
    ui.on_browse_folder(move || {
        if let Some(path) = rfd::FileDialog::new().pick_folder() {
            let path_str = path.to_string_lossy().into_owned();
            let ui_weak = ui_weak_folder.clone();
            let tree_state = tree_items_folder.clone();

            thread::spawn(move || {
                let mut file_paths = Vec::new();
                for entry in walkdir::WalkDir::new(&path)
                    .into_iter()
                    .filter_map(|e| e.ok())
                {
                    if entry.path().is_file() {
                        let rel = entry.path().strip_prefix(&path).unwrap();
                        file_paths.push(rel.to_string_lossy().replace("/", "\\"));
                    }
                }
                let items = pak_tree::generate_tree_items(&file_paths);
                let tree_strings = pak_tree::get_visible_tree_nodes(&items);

                *tree_state.lock().unwrap() = items;

                let list_items: Vec<_> = tree_strings
                    .into_iter()
                    .map(|t| StandardListViewItem::from(SharedString::from(t)))
                    .collect();

                let _ = ui_weak.upgrade_in_event_loop(move |ui| {
                    let slint_model = ModelRc::from(Rc::new(VecModel::from(list_items)));
                    ui.set_archive_files(slint_model);
                });
            });
            SharedString::from(path_str)
        } else {
            SharedString::new()
        }
    });

    ui.on_save_file(|ext| {
        let ext_str = ext.as_str();
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Output Resource", &[ext_str])
            .save_file()
        {
            SharedString::from(path.to_string_lossy().into_owned())
        } else {
            SharedString::new()
        }
    });

    let tree_items_click = tree_items_state.clone();
    let ui_weak_click = ui_handle.clone();
    ui.on_archive_item_clicked(move |visible_index| {
        if visible_index < 0 {
            return;
        }
        let mut items = tree_items_click.lock().unwrap();
        if pak_tree::toggle_tree_node(&mut items, visible_index as usize) {
            let visible_nodes = pak_tree::get_visible_tree_nodes(&items);
            let list_items: Vec<_> = visible_nodes
                .into_iter()
                .map(|t| StandardListViewItem::from(SharedString::from(t)))
                .collect();
            let _ = ui_weak_click.upgrade_in_event_loop(move |ui| {
                let slint_model = ModelRc::from(Rc::new(VecModel::from(list_items)));
                ui.set_archive_files(slint_model);
            });
        }
    });

    let logger_unpack = logger_base.clone();
    ui.on_unpack_dv2(move |input, out| {
        let logger = logger_unpack.clone();
        let in_path = PathBuf::from(input.as_str());
        let out_path = PathBuf::from(out.as_str());

        thread::spawn(move || {
            logger.log(&format!("[*] Extracting DV2: {:?}", in_path));
            if let Err(e) = dv2_archive::unpack_dv2(&in_path, &out_path, |msg| logger.log(msg)) {
                logger.log(&format!("[!] Error unpacking DV2: {}", e));
            } else {
                logger.log("[+] Unpack completed successfully.");
            }
        });
    });

    let logger_pack = logger_base.clone();
    ui.on_pack_dv2(move |src, out, compress, comp_level| {
        let logger = logger_pack.clone();
        let src_path = PathBuf::from(src.as_str());
        let out_path = PathBuf::from(out.as_str());
        let level = match comp_level.as_str() {
            "Smallest Size (Zlib Level 9)" => 9,
            "Fastest (Zlib Level 1)" => 1,
            "No Compression (Zlib Level 0)" => 0,
            _ => 6,
        };

        thread::spawn(move || {
            logger.log(&format!("[*] Packing into DV2: {:?}", src_path));
            if let Err(e) =
                dv2_archive::pack_dv2(&src_path, &out_path, compress, level, |msg| logger.log(msg))
            {
                logger.log(&format!("[!] Error packing DV2: {}", e));
            } else {
                logger.log("[+] Pack completed successfully.");
            }
        });
    });

    let logger_batch_unpack = logger_base.clone();
    ui.on_batch_unpack_dv2(move |root_folder| {
        let logger = logger_batch_unpack.clone();
        let root_path = PathBuf::from(root_folder.as_str());

        thread::spawn(move || {
            logger.log(&format!("[*] Batch unpacking archives in: {:?}", root_path));
            let mut found = 0;
            for entry in walkdir::WalkDir::new(&root_path)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                if entry.path().is_file()
                    && entry.path().extension().and_then(|s| s.to_str()) == Some("dv2")
                {
                    let file_stem = entry.path().file_stem().unwrap().to_string_lossy();
                    let out_dir = entry
                        .path()
                        .parent()
                        .unwrap()
                        .join(format!("{}_extracted", file_stem));
                    logger.log(&format!(
                        "Unpacking: {:?}",
                        entry.path().file_name().unwrap()
                    ));
                    if let Err(e) = dv2_archive::unpack_dv2(entry.path(), &out_dir, |_| {}) {
                        logger.log(&format!("[!] Error: {}", e));
                    } else {
                        found += 1;
                    }
                }
            }
            logger.log(&format!(
                "[+] Batch unpack completed. Successfully processed {} archives.",
                found
            ));
        });
    });

    let logger_dds = logger_base.clone();
    ui.on_convert_dds_to_nif(move |dds, nif| {
        let logger = logger_dds.clone();
        let d_path = PathBuf::from(dds.as_str());
        let n_path = PathBuf::from(nif.as_str());
        thread::spawn(move || {
            if let Err(e) =
                dds_converter::convert_dds_to_nif(&d_path, &n_path, |msg| logger.log(msg))
            {
                logger.log(&format!("[!] DDS Conversion Error: {}", e));
            }
        });
    });

    let logger_nif = logger_base.clone();
    ui.on_extract_nif_to_dds(move |nif, dds| {
        let logger = logger_nif.clone();
        let n_path = PathBuf::from(nif.as_str());
        let d_path = PathBuf::from(dds.as_str());
        thread::spawn(move || {
            match dds_converter::extract_nif_to_dds(&n_path, &d_path, |msg| logger.log(msg)) {
                Ok(true) => logger.log("[+] DDS texture successfully extracted."),
                Ok(false) => logger.log("[!] NIF file does not contain a valid NiPersistentSrcTextureRendererData block."),
                Err(e) => logger.log(&format!("[!] Extraction Error: {}", e)),
            }
        });
    });

    let logger_batch_dds = logger_base.clone();
    ui.on_batch_dds_to_nif(move |folder| {
        let logger = logger_batch_dds.clone();
        let path = PathBuf::from(folder.as_str());
        thread::spawn(move || {
            let files: Vec<_> = walkdir::WalkDir::new(&path)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|entry| {
                    entry.path().is_file()
                        && entry.path().extension().and_then(|s| s.to_str()) == Some("dds")
                })
                .collect();

            logger.log(&format!(
                "[*] Batch conversion: Found {} DDS files.",
                files.len()
            ));
            let mut success = 0;
            for (i, entry) in files.iter().enumerate() {
                let out_nif = entry.path().with_extension("nif");
                logger.log(&format!(
                    "[{}/{}] Converting: {:?}",
                    i + 1,
                    files.len(),
                    entry.path().file_name().unwrap()
                ));
                if dds_converter::convert_dds_to_nif(entry.path(), &out_nif, |_| {}).is_ok() {
                    success += 1;
                }
            }
            logger.log(&format!(
                "[+] Completed! Successfully converted {}/{} textures.",
                success,
                files.len()
            ));
        });
    });

    let logger_batch_nif = logger_base.clone();
    ui.on_batch_nif_to_dds(move |folder| {
        let logger = logger_batch_nif.clone();
        let path = PathBuf::from(folder.as_str());
        thread::spawn(move || {
            let files: Vec<_> = walkdir::WalkDir::new(&path)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|entry| {
                    entry.path().is_file()
                        && entry.path().extension().and_then(|s| s.to_str()) == Some("nif")
                })
                .collect();

            logger.log(&format!(
                "[*] Batch extraction: Found {} NIF files.",
                files.len()
            ));
            let mut success = 0;
            for entry in &files {
                let out_dds = entry.path().with_extension("dds");
                if let Ok(true) = dds_converter::extract_nif_to_dds(entry.path(), &out_dds, |_| {})
                {
                    success += 1;
                }
            }
            logger.log(&format!(
                "[+] Completed! Successfully extracted {} textures.",
                success
            ));
        });
    });

    let logger_xml_ext = logger_base.clone();
    let hashes_xml_ext = hash_db.clone();
    ui.on_extract_binary_xml(move |binary_xml, text_xml| {
        let logger = logger_xml_ext.clone();
        let b_path = PathBuf::from(binary_xml.as_str());
        let t_path = PathBuf::from(text_xml.as_str());
        let hashes = hashes_xml_ext.lock().unwrap().clone();

        thread::spawn(move || {
            logger.log(&format!(
                "[*] Extracting binary XML: {:?}",
                b_path.file_name().unwrap()
            ));
            if let Err(e) = xml_converter::extract_binary_xml_to_real_xml(&b_path, &t_path, &hashes)
            {
                logger.log(&format!("[!] XML Extraction Error: {}", e));
            } else {
                logger.log(&format!(
                    "[+] Saved readable XML to: {:?}",
                    t_path.file_name().unwrap()
                ));
            }
        });
    });

    let logger_xml_rep = logger_base.clone();
    ui.on_repack_readable_xml(move |original_nif, text_xml, output_nif| {
        let logger = logger_xml_rep.clone();
        let orig_path = PathBuf::from(original_nif.as_str());
        let txt_path = PathBuf::from(text_xml.as_str());
        let out_path = PathBuf::from(output_nif.as_str());

        thread::spawn(move || {
            logger.log("[*] Repacking text XML into binary NIF format...");
            if let Err(e) =
                xml_converter::repack_real_xml_to_binary_xml(&orig_path, &txt_path, &out_path)
            {
                logger.log(&format!("[!] XML Repack Error: {}", e));
            } else {
                logger.log(&format!(
                    "[+] Successfully compiled XML to: {:?}",
                    out_path.file_name().unwrap()
                ));
            }
        });
    });

    let logger_batch_xml_ext = logger_base.clone();
    let hashes_batch_xml_ext = hash_db.clone();
    ui.on_batch_extract_xml(move |folder| {
        let logger = logger_batch_xml_ext.clone();
        let path = PathBuf::from(folder.as_str());
        let hashes = hashes_batch_xml_ext.lock().unwrap().clone();

        thread::spawn(move || {
            let files: Vec<_> = walkdir::WalkDir::new(&path)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|entry| {
                    entry.path().is_file()
                        && entry.path().extension().and_then(|s| s.to_str()) == Some("xml")
                        && !entry.path().to_string_lossy().ends_with(".txt.xml")
                })
                .collect();

            logger.log(&format!(
                "[*] Batch Binary XML Extraction: Found {} files.",
                files.len()
            ));
            let mut success = 0;
            for (i, entry) in files.iter().enumerate() {
                let out_txt_xml = entry.path().with_extension("txt.xml");
                logger.log(&format!(
                    "[{}/{}] Extracting: {:?}",
                    i + 1,
                    files.len(),
                    entry.path().file_name().unwrap()
                ));
                if xml_converter::extract_binary_xml_to_real_xml(
                    entry.path(),
                    &out_txt_xml,
                    &hashes,
                )
                .is_ok()
                {
                    success += 1;
                }
            }
            logger.log(&format!(
                "[+] Batch extraction complete. Extracted {}/{} files.",
                success,
                files.len()
            ));
        });
    });

    let logger_batch_xml_rep = logger_base.clone();
    ui.on_batch_repack_xml(move |folder| {
        let logger = logger_batch_xml_rep.clone();
        let path = PathBuf::from(folder.as_str());

        thread::spawn(move || {
            let txt_files: Vec<_> = walkdir::WalkDir::new(&path)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|entry| {
                    entry.path().is_file() && entry.path().to_string_lossy().ends_with(".txt.xml")
                })
                .collect();

            if txt_files.is_empty() {
                logger.log("[!] No readable .txt.xml files found in the directory.");
                return;
            }

            let out_dir = path.join("Repacked_XML");
            let _ = std::fs::create_dir_all(&out_dir);

            logger.log("[*] Batch compiling XMLs into 'Repacked_XML'...");
            let mut success = 0;

            for (i, txt_path) in txt_files.iter().enumerate() {
                if txt_path.path().starts_with(&out_dir) {
                    continue;
                }

                let file_name = txt_path.path().file_name().unwrap().to_string_lossy();
                let clean_name = file_name.trim_end_matches(".txt.xml").to_string();
                let original_xml_name = format!("{}.xml", clean_name);

                let original_xml_path = txt_path.path().parent().unwrap().join(&original_xml_name);
                let out_xml_path = out_dir.join(&original_xml_name);

                if !original_xml_path.exists() {
                    logger.log(&format!(
                        "[!] Skipped Repack {}: Original binary XML template not found.",
                        original_xml_name
                    ));
                    continue;
                }

                logger.log(&format!(
                    "[{}/{}] Compiling: {}",
                    i + 1,
                    txt_files.len(),
                    original_xml_name
                ));
                if xml_converter::repack_real_xml_to_binary_xml(
                    &original_xml_path,
                    txt_path.path(),
                    &out_xml_path,
                )
                .is_ok()
                {
                    success += 1;
                }
            }
            logger.log(&format!(
                "[+] Batch repack completed. Repacked {}/{} XMLs.",
                success,
                txt_files.len()
            ));
        });
    });

    let logger_hashes = logger_base.clone();
    ui.on_scan_hashes(move |folder| {
        let logger = logger_hashes.clone();
        let path = PathBuf::from(folder.as_str());
        thread::spawn(move || {
            logger.log("[*] Initializing XML Hash scanner...");
            if let Err(e) = hash_scanner::scan_and_verify(&path, |msg| logger.log(msg)) {
                logger.log(&format!("[!] Scanner Error: {}", e));
            }
        });
    });

    ui.run()
}
