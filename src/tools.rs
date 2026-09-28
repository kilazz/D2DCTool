use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;

pub fn atomic_write(path: &Path, data: &[u8]) -> io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;

    let temp_name = format!(
        ".tmp_{}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
        path.file_name().and_then(|n| n.to_str()).unwrap_or("file")
    );
    let temp_path = parent.join(temp_name);

    {
        let mut file = File::create(&temp_path)?;
        file.write_all(data)?;
        file.sync_all()?;
    }

    #[cfg(windows)]
    {
        if path.exists() {
            let _ = fs::remove_file(path);
        }
    }

    if let Err(e) = fs::rename(&temp_path, path) {
        let _ = fs::copy(&temp_path, path);
        let _ = fs::remove_file(&temp_path);
        return Err(e);
    }

    Ok(())
}
