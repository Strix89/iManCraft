// Copia lib/sqlite3.dll accanto agli eseguibili compilati su Windows (app e test), che senza non partono.
fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "windows" {
        let dll = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../lib/sqlite3.dll");
        if dll.exists() {
            println!("cargo:rerun-if-changed={}", dll.display());
            if let Ok(out_str) = std::env::var("OUT_DIR") {
                let out = std::path::PathBuf::from(out_str);
                if let Some(profile_dir) = out.ancestors().nth(3) {
                    for dir in [profile_dir.to_path_buf(), profile_dir.join("deps")] {
                        let _ = std::fs::create_dir_all(&dir);
                        let _ = std::fs::copy(&dll, dir.join("sqlite3.dll"));
                    }
                }
            }
        }
    }
}
