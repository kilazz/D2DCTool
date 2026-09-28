use crate::AppWindow;
use crate::dv2_archive;
use crate::logger::UiLogger;
use crate::pak_tree::{self, TreeItem};
use slint::{ComponentHandle, ModelRc, SharedString, StandardListViewItem, VecModel};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::thread;

pub fn register_archive_callbacks(ui: &AppWindow, logger: UiLogger) {
    let tree_items_state = Arc::new(Mutex::new(Vec::<TreeItem>::new()));

    // Virtualized tree expand/collapse click
    let tree_items_click = tree_items_state.clone();
    let ui_weak_click = ui.as_weak();

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
                ui.set_archive_files(ModelRc::from(Rc::new(VecModel::from(list_items))));
            });
        }
    });

    // Inspect single DV2 archive
    let tree_items_browse = tree_items_state;
    let ui_weak_browse = ui.as_weak();
    let log_inspect = logger.clone();

    ui.on_load_dv2_tree(move |path_str| {
        let p = PathBuf::from(path_str.as_str());
        let tree_state = tree_items_browse.clone();
        let ui_weak = ui_weak_browse.clone();
        let log = log_inspect.clone();

        thread::spawn(move || {
            log.log(&format!("[*] Inspecting DV2 archive: {:?}", p));
            match dv2_archive::read_entries(&p) {
                Ok(entries) => {
                    let total = entries.len();
                    let file_paths: Vec<String> = entries.into_iter().map(|e| e.name).collect();
                    let items = pak_tree::generate_tree_items(&file_paths);
                    let visible_nodes = pak_tree::get_visible_tree_nodes(&items);
                    *tree_state.lock().unwrap() = items;

                    let list_items: Vec<_> = visible_nodes
                        .into_iter()
                        .map(|s| StandardListViewItem::from(SharedString::from(s)))
                        .collect();

                    log.log(&format!("[+] Loaded {} files into VFS tree.", total));

                    let _ = ui_weak.upgrade_in_event_loop(move |ui| {
                        ui.set_archive_files(ModelRc::from(Rc::new(VecModel::from(list_items))));
                        ui.set_status_msg(format!("Archive loaded: {} files.", total).into());
                    });
                }
                Err(e) => {
                    log.log(&format!("[!] Error reading DV2 archive: {}", e));
                }
            }
        });
    });

    // Single unpack
    let log_unpack = logger.clone();
    ui.on_unpack_dv2(move |input, out| {
        let log = log_unpack.clone();
        let in_p = PathBuf::from(input.as_str());
        let out_p = PathBuf::from(out.as_str());

        thread::spawn(move || {
            log.log(&format!("[*] Extracting DV2 archive: {:?}", in_p));
            if let Err(e) = dv2_archive::unpack_dv2(&in_p, &out_p, |msg| log.log(msg)) {
                log.log(&format!("[!] Extraction Error: {}", e));
            } else {
                log.log("[+] Extraction completed successfully.");
            }
        });
    });

    // Parallel batch unpack across all CPU cores
    let log_bunpack = logger.clone();
    ui.on_batch_unpack_dv2(move |root_folder| {
        let log = log_bunpack.clone();
        let src_p = PathBuf::from(root_folder.as_str());

        thread::spawn(move || {
            if let Some(dest_p) = rfd::FileDialog::new()
                .set_title("Select Destination Folder for Extracted Archives")
                .pick_folder()
            {
                if let Err(e) = dv2_archive::batch_unpack_dv2(&src_p, &dest_p, |msg| log.log(msg)) {
                    log.log(&format!("[!] Batch Unpack Error: {}", e));
                }
            } else {
                log.log("[*] Batch unpack cancelled: no destination folder selected.");
            }
        });
    });

    // Single pack with chunked parallel compression
    let log_pack = logger.clone();
    ui.on_pack_dv2(move |src, out, comp, algo, lvl| {
        let log = log_pack.clone();
        let s_path = PathBuf::from(src.as_str());
        let o_path = PathBuf::from(out.as_str());
        let algo_str = algo.to_string();
        let level = lvl.clamp(0, 9) as u32;

        thread::spawn(move || {
            log.log(&format!(
                "[*] Packing DV2 archive (Algorithm: {}, Level: {}, Dedup: XXH3): {:?}",
                algo_str, level, s_path
            ));
            if let Err(e) =
                dv2_archive::pack_dv2(&s_path, &o_path, comp, &algo_str, level, |msg| log.log(msg))
            {
                log.log(&format!("[!] Packing Error: {}", e));
            } else {
                log.log("[+] Archive packed successfully.");
            }
        });
    });

    // Parallel batch pack across all CPU cores
    let log_bpack = logger;
    ui.on_batch_pack_dv2(move |root_folder, comp, algo, lvl| {
        let log = log_bpack.clone();
        let src_p = PathBuf::from(root_folder.as_str());
        let algo_str = algo.to_string();
        let level = lvl.clamp(0, 9) as u32;

        thread::spawn(move || {
            if let Some(dest_p) = rfd::FileDialog::new()
                .set_title("Select Destination Output Folder for Compiled .DV2 Archives")
                .pick_folder()
            {
                if let Err(e) = dv2_archive::batch_pack_folders(
                    &src_p,
                    &dest_p,
                    comp,
                    &algo_str,
                    level,
                    |msg| log.log(msg),
                ) {
                    log.log(&format!("[!] Batch Pack Error: {}", e));
                }
            } else {
                log.log("[*] Batch pack cancelled: no output folder selected.");
            }
        });
    });
}
