use std::path::PathBuf;
use dirs;

pub struct AppCfg{
    pub app_name : String,
    pub cfg_dir : PathBuf,
    pub vault_path : PathBuf
}

impl AppCfg{
    pub fn new(
        vault_fn: &str
    ) -> Self {
        let app_name = String::from("pxxxstore");
        let cfg_dir :PathBuf = dirs::config_dir()
            .expect("[ Error ] : config not founds")
            .join(&app_name);
        let vault_path = cfg_dir.join(vault_fn);
        AppCfg{
            app_name,
            cfg_dir,
            vault_path
        }
    }
}