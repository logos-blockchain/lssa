fn main() {
    if cfg!(feature = "no_docker") {
        println!("cargo:warning=NO_DOCKER feature enabled – deterministic build skipped");
        return;
    }

    build_deterministic().expect("Deterministic build failed");
}

fn build_deterministic() -> Result<(), Box<dyn std::error::Error>> {
    use std::{env, fs, path::PathBuf, process::Command};

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let out_dir = PathBuf::from(env::var("OUT_DIR")?);
    let mod_dir = out_dir.join("program_methods");
    let mod_file = mod_dir.join("mod.rs");

    println!("cargo:rerun-if-changed=program_methods/guest/src");
    println!("cargo:rerun-if-changed=program_methods/guest/Cargo.toml");

    let guest_manifest = manifest_dir.join("program_methods/guest/Cargo.toml");

    let status = Command::new("cargo")
        .args(["risczero", "build", "--manifest-path"])
        .arg(&guest_manifest)
        .status()?;
    if !status.success() {
        return Err("Risc0 deterministic build failed".into());
    }

    let target_dir =
        manifest_dir.join("program_methods/guest/target/riscv32im-risc0-zkvm-elf/docker/");

    let bins = fs::read_dir(&target_dir)?
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "bin"))
        .collect::<Vec<_>>();

    if bins.is_empty() {
        return Err(format!("No .bin files found in {:?}", target_dir).into());
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
