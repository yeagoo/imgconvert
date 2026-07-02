fn main() {
    println!("cargo:rerun-if-env-changed=IMGCONVERT_DISABLE_EXTERNAL_CODECS");
    tauri_build::build()
}
