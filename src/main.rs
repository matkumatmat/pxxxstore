mod cli;
mod cfg;
mod domain;
mod crypto;

use clap::Parser;
use cli::{Cli,Commands};
use chrono::Utc;
use domain::{
    load_vault,save_vault,find_entry,find_entry_mut};
use domain::Data;
use domain::init_vault;

fn main()-> Result<(), Box<dyn std::error::Error>>{
    let cli = Cli::parse();
    let cfg = cfg::AppCfg::new("x.bin");
    println!("[ Msg ]: {} - pwd stored ", cfg.app_name);
    match cli.command {
        Commands::Init => {
            init_vault(&cfg)?;
            println!("[ Msg ]: Vault initialized at: {}", cfg.vault_path.display());
        }
        
        Commands::Add { src, id, pass } => {
            let (mut vault, passphrase) = load_vault(&cfg)?;            
            if find_entry(&vault, &src).is_some() {
                return Err(format!("[ Msg ]: Entry with src '{}' already exists. Use 'update' or 'delete' first.", src).into());
            }
            let entry = Data::new(src.clone(), id, pass);
            vault.entries.push(entry);
            
            save_vault(&vault, &cfg, &passphrase)?;
            println!("[ Msg ]: Added: {}", src);
        }
        
        Commands::List => {
            let (vault, _) = load_vault(&cfg)?;
            if vault.entries.is_empty() {
                println!("[ Error ]: No entries found.");
            } else {
                println!("📋 Entries:");
                for entry in &vault.entries {
                    println!("- {} (id: {})", entry.src, entry.id);
                }
            }
        }
        
        Commands::Get { src } => {
            let (vault, _) = load_vault(&cfg)?;
            match find_entry(&vault, &src) {
                Some(entry) => {
                    println!("🔑 {}:", entry.src);
                    println!("  ID: {}", entry.id);
                    println!("  Password: {}", entry.pass);
                    println!("  Created: {}", entry.created_at);
                    println!("  Updated: {}", entry.updated_at);
                }
                None => println!("[ Error ]: Entry '{}' not found.", src),
            }
        }

        Commands::Update { src, id, pass } => {
            let (mut vault, passphrase) = load_vault(&cfg)?;
            match find_entry_mut(&mut vault , &src) {
                Some(entry) => {
                    entry.id = id;
                    entry.pass = pass;
                    entry.updated_at = Utc::now();
                    save_vault(&vault, &cfg, &passphrase)?;
                    println!("[ Msg ]: Updated : {}", src);
                }
                None => println!("[ Error ]: Entry {}, Not found", src),
            }
        }
        
        Commands::Delete { src } => {
            let (mut vault, passphrase) = load_vault(&cfg)?;
            let original_len = vault.entries.len();
            vault.entries.retain(|e| e.src != src);
            
            if vault.entries.len() == original_len {
                println!("[ Error ]: Entry '{}' not found.", src);
            } else {
                save_vault(&vault, &cfg, &passphrase)?;
                println!("[ Msg ]: Deleted: {}", src);
            }
        }
    }
    Ok(())
}