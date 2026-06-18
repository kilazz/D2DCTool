## D2DCTool
Utility for **Divinity 2 Developer's Cut** modding, written in Rust and powered by the Slint GUI framework.

## Features

### 📦 Archive Management (.dv2)
*   **Interactive TreeView:** Navigate the archive structure with an expandable and collapsible directory explorer.
*   **Unpack:** Extract `.dv2` archives (supports Dv2 v4 and v5 formats).
*   **Pack:** Compile directories into valid Divinity 2 `.dv2` archives with customizable zlib compression levels.
*   **Batch Operations:** Automatically scan and unpack all `.dv2` archives found within a target folder.

### 🖼️ Texture Tools
*   **Convert DDS to NIF:** Convert single DDS files (DXT1, DXT3, DXT5) to Gamebryo persistent texture NIF model files.
*   **Extract NIF to DDS:** Extract original raw DDS textures directly out of Gamebryo persistent texture NIF files.
*   **Batch Convert:** Convert folders containing DDS to NIF, or extract folders containing NIF to DDS.

### 📄 XML Tools & Hash Database Scanner
*   **Extract Binary XML:** Decode compiled binary Gamebryo XML files (`xml::dom::CStreamableNode`) to readable text XML, utilizing a custom hash dictionary.
*   **Repack Edited XML:** Compile readable text XML back into binary Gamebryo XML, preserving structural alignment.
*   **Batch XML Processing:** Perform folder-wide batch extractions and batch compilations.
*   **Hash Database Scanner:** Scan XML folders to verify and extract unknown hashes, cleanly updating your local `hash_dictionary.txt` database.

## How to Build & Run

### Prerequisites
*   Install [Rust & Cargo](https://www.rust-lang.org/tools/install).

### Compilation
*   Open your terminal in the project's root directory and run:
```
cargo run
cargo build --release
```
