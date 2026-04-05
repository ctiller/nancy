use did_key::{Ed25519KeyPair, Fingerprint, KeyMaterial, generate};
use git2::Repository;
use serde_json::json;
use std::fs::{self, OpenOptions};
use std::io::Write;

pub fn init() {
    let repo = match Repository::discover(".") {
        Ok(r) => r,
        Err(e) => {
            eprintln!(
                "Failed to validate git tree. Ensure you are inside a git repository: {}",
                e
            );
            std::process::exit(1);
        }
    };

    let workdir = match repo.workdir() {
        Some(p) => p,
        None => {
            eprintln!("Repository appears to be bare. Need a working directory.");
            std::process::exit(1);
        }
    };

    let nancy_dir = workdir.join(".nancy");
    let identity_file = nancy_dir.join("identity.json");

    if nancy_dir.exists() && identity_file.exists() {
        eprintln!("nancy is already initialized (identity.json exists). Aborting without changes.");
        std::process::exit(1);
    }

    // Ensure `.nancy` is in `.gitignore`
    let gitignore_path = workdir.join(".gitignore");
    let gitignore_contents = fs::read_to_string(&gitignore_path).unwrap_or_default();
    let mut has_nancy = false;
    for line in gitignore_contents.lines() {
        if line.trim() == ".nancy" || line.trim() == "/.nancy" || line.trim() == ".nancy/" {
            has_nancy = true;
            break;
        }
    }
    if !has_nancy {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&gitignore_path)
            .expect("Failed to open .gitignore for appending");
        if !gitignore_contents.ends_with('\n') && !gitignore_contents.is_empty() {
            writeln!(file).expect("Failed to write to .gitignore");
        }
        writeln!(file, ".nancy").expect("Failed to write to .gitignore");
        println!("Added .nancy to .gitignore");
    }

    if let Err(e) = fs::create_dir_all(&nancy_dir) {
        eprintln!("Failed to create .nancy directory: {}", e);
        std::process::exit(1);
    }
    // Generate a new Ed25519 key pair
    println!("Generating a new Ed25519 DID...");
    let key = generate::<Ed25519KeyPair>(None);

    // Print the did:key URI (the fingerprint)
    let did = key.fingerprint();

    let id_obj = json!({
        "did": did,
        "public_key_hex": hex::encode(key.public_key_bytes()),
        "private_key_hex": hex::encode(key.private_key_bytes())
    });

    if let Err(e) = fs::write(
        &identity_file,
        serde_json::to_string_pretty(&id_obj).unwrap(),
    ) {
        eprintln!("Failed to write identity.json: {}", e);
        std::process::exit(1);
    }

    println!("Successfully provisioned new DID and initialized .nancy!");
    println!("DID: {}", did);
}
