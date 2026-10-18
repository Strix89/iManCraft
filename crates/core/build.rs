// Copia lib/sqlite3.dll accanto agli eseguibili compilati (app e test), che senza non partono.
fn main() {
    let dll = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../lib/sqlite3.dll");
    println!("cargo:rerun-if-changed={}", dll.display());
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let profile_dir = out.ancestors().nth(3).unwrap();
    for dir in [profile_dir.to_path_buf(), profile_dir.join("deps")] {
        let _ = std::fs::create_dir_all(&dir);
        std::fs::copy(&dll, dir.join("sqlite3.dll")).unwrap();
    }
}
