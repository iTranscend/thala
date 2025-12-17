use anyhow::Context;
use std::{
    fs,
    path::{Path, PathBuf},
};

use litep2p::crypto::ed25519::{Keypair, SecretKey};

pub struct IdentityManager {
    data_dir: PathBuf,
}

impl IdentityManager {
    pub fn new(data_dir: PathBuf) -> anyhow::Result<Self> {
        // create data-dir if it doesn't exist
        if !data_dir.exists() {
            fs::create_dir_all(&data_dir)?;
        }

        Ok(Self { data_dir })
    }

    pub fn load_or_generate_keypair(&self) -> anyhow::Result<Keypair> {
        let key_path = self.data_dir.join("node-key");

        if key_path.exists() {
            self.load_keypair(&key_path)
        } else {
            let keypair = Keypair::generate();
            self.save_keypair(&keypair, &key_path)?;
            Ok(keypair)
        }
    }

    fn save_keypair(&self, keypair: &Keypair, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).context("Failed to create directory")?;
        }

        let secret_bytes = keypair.secret().to_bytes();
        std::fs::write(path, secret_bytes)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let mut perms = fs::metadata(path)?.permissions();
            perms.set_mode(0o600); // Read and write for owner only
            fs::set_permissions(path, perms)?;
        }

        Ok(())
    }

    pub fn load_keypair(&self, path: &Path) -> anyhow::Result<Keypair> {
        let sk_bytes = fs::read(path)?;
        let secret_key = SecretKey::try_from_bytes(sk_bytes)?;
        Ok(Keypair::from(secret_key))
    }
}
