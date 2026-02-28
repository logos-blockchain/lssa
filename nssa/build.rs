use std::{env, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let out_dir = PathBuf::from(env::var("OUT_DIR")?);
    let mod_dir = out_dir.join("program_methods");
    let mod_file = mod_dir.join("mod.rs");
    // Allow overriding the artifacts directory via environment variable.
    // This is needed for reproducible/sandboxed builds (e.g., Nix) where the
    // default relative path may resolve to a read-only location.
    let program_methods_dir = env::var("NSSA_ARTIFACTS_DIR")
        .map(|d| PathBuf::from(d).join("program_methods"))
        .unwrap_or_else(|_| manifest_dir.join("../artifacts/program_methods/"));

    println!("cargo:rerun-if-changed={}", program_methods_dir.display());

    let bins = fs::read_dir(&program_methods_dir)?
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "bin"))
        .collect::<Vec<_>>();

    if bins.is_empty() {
        return Err(format!("No .bin files found in {:?}", program_methods_dir).into());
    }

    fs::create_dir_all(&mod_dir)?;
    let mut src = String::new();
    for entry in bins {
        let path = entry.path();
        let name = path.file_stem().unwrap().to_string_lossy();
        let bytecode = fs::read(&path)?;
        let image_id: [u32; 8] = risc0_binfmt::compute_image_id(&bytecode)?.into();
        src.push_str(&format!(
            "pub const {}_ELF: &[u8] = include_bytes!(r#\"{}\"#);\n\
             pub const {}_ID: [u32; 8] = {:?};\n",
            name.to_uppercase(),
            path.display(),
            name.to_uppercase(),
            image_id
        ));
    }
    fs::write(&mod_file, src)?;
    println!("cargo:warning=Generated module at {}", mod_file.display());

    Ok(())
}
