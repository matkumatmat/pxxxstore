use chrono::{DateTime, Utc};
use serde::{Deserialize,Serialize};

use crate::cfg::AppCfg;
use crate::crypto;
use serde_json;
use rpassword;
use std::fs;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Data{
    pub src : String,
    pub id : String,
    pub pass : String,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub created_at : DateTime<Utc>,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub updated_at : DateTime<Utc>,
}
impl Data {
    pub fn new(
        src: String,
        id: String,
        pass: String
    ) -> Self {
        let now = Utc::now();
        Data{
            src,
            id,
            pass,
            created_at : now,
            updated_at : now
        }
    }
}

#[derive(Serialize,Deserialize)]
pub struct Vault {
    pub version : u8,
    pub entries : Vec<Data>
}

impl Vault {
    pub fn new () -> Self {
        Vault {
            version : 1,
            entries : Vec::new()
        }
    }
}

pub fn load_vault(
    cfg : &AppCfg
) -> Result <(Vault, String), Box<dyn std::error::Error>>{
    if !cfg.vault_path.exists(){
        return Err("[ Error ] : Vault not found, Try - run 'init' first"
                .into())
    };
    let enc = fs::read(&cfg.vault_path)?;
    let passphrase = rpassword::prompt_password("enter the passphrase : ")?;
    let plaintxt = crypto::decrypt(&enc, &passphrase)?;
    let vault : Vault = serde_json::from_slice(&plaintxt)?;
    Ok((vault,passphrase))
}

pub fn save_vault(
    vault: &Vault,
    cfg : &AppCfg,
    passphrase : &str
) -> Result<(), Box<dyn std::error::Error>> {
    let json = serde_json::to_vec(vault)?;
    let enc = crypto::encrypt(&json, passphrase)?;
    let tmp_path = cfg.vault_path
                            .with_extension("tmp");
    fs::write(&tmp_path, enc)?;
    fs::rename(tmp_path, &cfg.vault_path)?;
    Ok(())
}

pub fn find_entry<'a>(
    vault : &'a Vault,
    src : &str
) -> Option<&'a Data>{
    vault.entries
        .iter()
        .find(|e|e.src == src)
}

pub fn find_entry_mut<'a>(
    vault : &'a mut Vault,
    src : &str
) -> Option<&'a mut Data>{
    vault.entries
        .iter_mut()
        .find(|e|e.src == src)
}

pub fn init_vault(config: &AppCfg) -> Result<(), Box<dyn std::error::Error>> {
    if !config.cfg_dir.exists() {
        fs::create_dir_all(&config.cfg_dir)?;
        println!("[ Msg ] : Config directory created: {}", config.cfg_dir.display());
    }

    if !config.vault_path.exists() {
        println!("🔐Set a passphrase:");
        let passphrase = rpassword::prompt_password("New passphrase: ")?;
        let confirm = rpassword::prompt_password("Confirm passphrase: ")?;
        if passphrase != confirm {
            return Err("Passphrases do not match".into());
        }

        let vault = Vault::new();
        let json = serde_json::to_vec(&vault)?;
        let encrypted = crypto::encrypt(&json, &passphrase)?;
        fs::write(&config.vault_path, encrypted)?;
        println!("[ Msg ] : Vault initialized at: {}", config.vault_path.display());
    } else {
        println!("[ Error ] : Vault already exists.");
    }
    Ok(())
}

