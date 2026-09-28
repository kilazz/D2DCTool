use crate::dds;
use crate::diff;
use crate::dv2_archive;
use crate::hash_scanner;
use crate::logger::make_cli_logger;
use crate::xml_converter;
use std::collections::HashMap;
use std::io::BufRead;
use std::path::Path;

pub fn print_help() {
    println!(
        "\
D2DCTool v2.0 - Divinity 2 Developer's Cut Modding & Asset Studio (CLI Mode)
Usage: D2DCTool <command> [arguments...]

Archive Commands:
  unpack <archive.dv2> <out_dir>
      Extract all files from a single .dv2 archive.

  pack <src_dir> <out_dv2> [algo: zlib|zopfli] [level: 0-9]
      Pack a single folder into a .dv2 archive.

  batch_unpack_dv2 <source_game_dir> <dest_extracted_dir>
      Recursively find and extract all .dv2 archives, preserving the full directory tree.

  batch_pack_dv2 <source_extracted_dir> <dest_packed_dir> [algo: zlib|zopfli] [level: 0-9]
      Recursively find all `_extracted` folders and compile each into `.dv2` archives.

Texture & Mesh Commands:
  repair_dds <path_or_dir>
      Scan and repair corrupt DDS texture mipmap headers (DDSD_MIPMAPCOUNT / DDSCAPS_MIPMAP).

  dds_to_nif <input.dds> <output.nif>
      Convert a DDS texture file into a Gamebryo NIF texture.

  nif_to_dds <input.nif> <output.dds>
      Extract a DDS texture from a Gamebryo NIF texture file.

XML & Hash Commands:
  extract_xml <input_binary.xml> <output_text.xml>
      Decode Gamebryo binary XML (CStreamableNode) into human-readable text XML.

  repack_xml <original.xml> <edited_text.xml> <output.xml>
      Compile human-readable text XML back into binary XML format.

  scan_hashes <xml_dir>
      Scan an XML directory for unknown hashes and verify against hash dictionary.

Mod Patching Commands:
  create_diff <base_dir> <mod_dir> <out_patch.json>
      Generate a non-destructive mod diff patch between base and modified assets.

  apply_diff <target_dir> <patch.json>
      Apply and merge a mod diff patch into a target directory without file conflicts.

General:
  help, --help, -h
      Show this help message."
    );
}

pub fn handle_cli(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let cmd = args[1].to_lowercase();
    match cmd.as_str() {
        "--help" | "-h" | "help" => print_help(),

        "unpack" => {
            if args.len() < 4 {
                eprintln!("Usage: D2DCTool unpack <archive.dv2> <out_dir>");
                return Ok(());
            }
            let (logger, handle) = make_cli_logger();
            dv2_archive::unpack_dv2(Path::new(&args[2]), Path::new(&args[3]), |msg| {
                logger.log(msg)
            })?;
            drop(logger);
            let _ = handle.join();
        }

        "batch_unpack_dv2" => {
            if args.len() < 4 {
                eprintln!(
                    "Usage: D2DCTool batch_unpack_dv2 <source_game_dir> <dest_extracted_dir>"
                );
                return Ok(());
            }
            let (logger, handle) = make_cli_logger();
            dv2_archive::batch_unpack_dv2(Path::new(&args[2]), Path::new(&args[3]), |msg| {
                logger.log(msg)
            })
            .map_err(std::io::Error::other)?;
            drop(logger);
            let _ = handle.join();
        }

        "pack" => {
            if args.len() < 4 {
                eprintln!(
                    "Usage: D2DCTool pack <src_dir> <out_dv2> [algo: zlib|zopfli] [level: 0-9]"
                );
                return Ok(());
            }
            let algo = args.get(4).map(|s| s.as_str()).unwrap_or("zlib");
            let level = args.get(5).and_then(|s| s.parse::<u32>().ok()).unwrap_or(6);
            let (logger, handle) = make_cli_logger();
            dv2_archive::pack_dv2(
                Path::new(&args[2]),
                Path::new(&args[3]),
                level > 0,
                algo,
                level,
                |msg| logger.log(msg),
            )?;
            drop(logger);
            let _ = handle.join();
        }

        "batch_pack_dv2" => {
            if args.len() < 4 {
                eprintln!(
                    "Usage: D2DCTool batch_pack_dv2 <source_extracted_dir> <dest_packed_dir> [algo: zlib|zopfli] [level: 0-9]"
                );
                return Ok(());
            }
            let algo = args.get(4).map(|s| s.as_str()).unwrap_or("zlib");
            let level = args.get(5).and_then(|s| s.parse::<u32>().ok()).unwrap_or(6);
            let (logger, handle) = make_cli_logger();
            dv2_archive::batch_pack_folders(
                Path::new(&args[2]),
                Path::new(&args[3]),
                level > 0,
                algo,
                level,
                |msg| logger.log(msg),
            )
            .map_err(std::io::Error::other)?;
            drop(logger);
            let _ = handle.join();
        }

        "repair_dds" => {
            if args.len() < 3 {
                eprintln!("Usage: D2DCTool repair_dds <path_or_dir>");
                return Ok(());
            }
            let target_path = Path::new(&args[2]);
            if target_path.is_file() {
                if dds::repair_single_dds(target_path)? {
                    println!("[+] Successfully repaired DDS header: {:?}", target_path);
                } else {
                    println!(
                        "[*] Texture header is valid or unmodified: {:?}",
                        target_path
                    );
                }
            } else if target_path.is_dir() {
                let fixed = dds::batch_repair_dds(target_path, |msg| println!("{}", msg))?;
                println!(
                    "[+] Scan completed. Repaired {} corrupted DDS headers.",
                    fixed
                );
            }
        }

        "dds_to_nif" => {
            if args.len() < 4 {
                eprintln!("Usage: D2DCTool dds_to_nif <input.dds> <output.nif>");
                return Ok(());
            }
            let (logger, handle) = make_cli_logger();
            dds::convert_dds_to_nif(Path::new(&args[2]), Path::new(&args[3]), |msg| {
                logger.log(msg)
            })
            .map_err(std::io::Error::other)?;
            drop(logger);
            let _ = handle.join();
        }

        "nif_to_dds" => {
            if args.len() < 4 {
                eprintln!("Usage: D2DCTool nif_to_dds <input.nif> <output.dds>");
                return Ok(());
            }
            let (logger, handle) = make_cli_logger();
            dds::extract_nif_to_dds(Path::new(&args[2]), Path::new(&args[3]), |msg| {
                logger.log(msg)
            })
            .map_err(std::io::Error::other)?;
            drop(logger);
            let _ = handle.join();
        }

        "extract_xml" => {
            if args.len() < 4 {
                eprintln!("Usage: D2DCTool extract_xml <input_binary.xml> <output_text.xml>");
                return Ok(());
            }
            let mut hashes = HashMap::new();
            let dict_path = Path::new("xml_hashes/hash_dictionary.txt");
            if dict_path.exists()
                && let Ok(file) = std::fs::File::open(dict_path)
            {
                let reader = std::io::BufReader::new(file);
                for line in reader.lines().map_while(Result::ok) {
                    let trimmed = line.trim().to_string();
                    if !trimmed.is_empty() {
                        hashes.insert(xml_converter::get_hash_value(&trimmed), trimmed);
                    }
                }
            }
            xml_converter::extract_binary_xml_to_real_xml(
                Path::new(&args[2]),
                Path::new(&args[3]),
                &hashes,
            )
            .map_err(std::io::Error::other)?;
            println!("[+] Text XML successfully exported.");
        }

        "repack_xml" => {
            if args.len() < 5 {
                eprintln!(
                    "Usage: D2DCTool repack_xml <original.xml> <edited_text.xml> <output.xml>"
                );
                return Ok(());
            }
            xml_converter::repack_real_xml_to_binary_xml(
                Path::new(&args[2]),
                Path::new(&args[3]),
                Path::new(&args[4]),
            )
            .map_err(std::io::Error::other)?;
            println!("[+] Binary XML successfully recompiled.");
        }

        "scan_hashes" => {
            if args.len() < 3 {
                eprintln!("Usage: D2DCTool scan_hashes <xml_dir>");
                return Ok(());
            }
            let (logger, handle) = make_cli_logger();
            hash_scanner::scan_and_verify(Path::new(&args[2]), |msg| logger.log(msg))
                .map_err(std::io::Error::other)?;
            drop(logger);
            let _ = handle.join();
        }

        "create_diff" => {
            if args.len() < 5 {
                eprintln!("Usage: D2DCTool create_diff <base_dir> <mod_dir> <out_patch.json>");
                return Ok(());
            }
            let (logger, handle) = make_cli_logger();
            diff::create_diff(
                Path::new(&args[2]),
                Path::new(&args[3]),
                Path::new(&args[4]),
                &logger,
            )?;
            drop(logger);
            let _ = handle.join();
        }

        "apply_diff" => {
            if args.len() < 4 {
                eprintln!("Usage: D2DCTool apply_diff <target_dir> <patch.json>");
                return Ok(());
            }
            let (logger, handle) = make_cli_logger();
            diff::apply_patch(Path::new(&args[2]), Path::new(&args[3]), &logger)?;
            drop(logger);
            let _ = handle.join();
        }

        unknown => {
            eprintln!("[!] Unknown CLI command: '{}'", unknown);
            print_help();
        }
    }
    Ok(())
}
