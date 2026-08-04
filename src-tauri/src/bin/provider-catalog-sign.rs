use base64::{engine::general_purpose::STANDARD, Engine as _};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use std::{env, fs, path::PathBuf};

const DEVELOPMENT_SEED: [u8; 32] = [0x42; 32];

fn main() {
    if let Err(error) = run() {
        eprintln!("provider-catalog-sign: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    match args.first().map(String::as_str) {
        Some("public-key") => {
            let key = signing_key(&args)?;
            println!("{}", STANDARD.encode(key.verifying_key().as_bytes()));
            Ok(())
        }
        Some("sign") => {
            let catalog = required_path(&args, "--catalog")?;
            let signature_path = required_path(&args, "--signature")?;
            let signature = signing_key(&args)?.sign(
                &fs::read(&catalog)
                    .map_err(|error| format!("cannot read {}: {error}", catalog.display()))?,
            );
            fs::write(
                &signature_path,
                format!("{}\n", STANDARD.encode(signature.to_bytes())),
            )
            .map_err(|error| format!("cannot write {}: {error}", signature_path.display()))
        }
        Some("verify") => {
            let catalog = required_path(&args, "--catalog")?;
            let signature_path = required_path(&args, "--signature")?;
            let public_key = required_value(&args, "--public-key")?;
            verify(
                &fs::read(&catalog)
                    .map_err(|error| format!("cannot read {}: {error}", catalog.display()))?,
                fs::read_to_string(&signature_path)
                    .map_err(|error| format!("cannot read {}: {error}", signature_path.display()))?
                    .trim(),
                public_key,
            )
        }
        _ => Err("usage: provider-catalog-sign <public-key|sign|verify> [options]".to_string()),
    }
}

fn signing_key(args: &[String]) -> Result<SigningKey, String> {
    let encoded = if args.iter().any(|value| value == "--development") {
        return Ok(SigningKey::from_bytes(&DEVELOPMENT_SEED));
    } else if let Ok(value) = env::var("VISUAL_MAP_PROVIDER_SIGNING_KEY") {
        value
    } else if let Ok(path) = env::var("VISUAL_MAP_PROVIDER_SIGNING_KEY_FILE") {
        fs::read_to_string(&path)
            .map_err(|error| format!("cannot read signing key file {path}: {error}"))?
    } else {
        return Err("set VISUAL_MAP_PROVIDER_SIGNING_KEY(_FILE) or pass --development".to_string());
    };
    let bytes = STANDARD
        .decode(encoded.trim())
        .map_err(|error| format!("invalid signing key base64: {error}"))?;
    let seed: [u8; 32] = bytes
        .try_into()
        .map_err(|_| "provider signing key must be a 32-byte Ed25519 seed".to_string())?;
    Ok(SigningKey::from_bytes(&seed))
}

fn verify(catalog: &[u8], encoded_signature: &str, encoded_public_key: &str) -> Result<(), String> {
    let public_key: [u8; 32] = STANDARD
        .decode(encoded_public_key.trim())
        .map_err(|error| format!("invalid public key base64: {error}"))?
        .try_into()
        .map_err(|_| "provider public key must be 32 bytes".to_string())?;
    let signature = Signature::from_slice(
        &STANDARD
            .decode(encoded_signature)
            .map_err(|error| format!("invalid signature base64: {error}"))?,
    )
    .map_err(|error| format!("invalid Ed25519 signature: {error}"))?;
    VerifyingKey::from_bytes(&public_key)
        .map_err(|error| format!("invalid Ed25519 public key: {error}"))?
        .verify(catalog, &signature)
        .map_err(|_| "provider catalog signature mismatch".to_string())
}

fn required_path(args: &[String], name: &str) -> Result<PathBuf, String> {
    required_value(args, name).map(PathBuf::from)
}

fn required_value<'a>(args: &'a [String], name: &str) -> Result<&'a str, String> {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].as_str())
        .ok_or_else(|| format!("missing {name}"))
}
