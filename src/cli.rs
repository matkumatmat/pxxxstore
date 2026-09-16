use clap::{Parser,Subcommand};

#[derive(Parser)]
#[command(version,about,long_about=None)]
pub struct Cli {
    #[command(subcommand)]
    pub command : Commands
}

#[derive(Subcommand)]
pub enum Commands{
    /// Application config initialization
    Init,
    /// Add new Entry
    Add {
        #[arg(short,long)]
        src : String,
        #[arg(short,long)]
        id : String,
        #[arg(short,long)]
        pass : String
    },
    /// Update existed Entry
    Update {
        #[arg(short,long)]
        src : String,
        #[arg(short,long)]
        id : String,
        #[arg(short,long)]
        pass : String
    },
    /// Display existed Entry List (without password)
    List,
    /// Display existed Entry List (with password)
    Get {
        src : String,
    },
    /// Delete existed Entry List
    Delete {
        src : String,
    },
    /// Launch interactive TUI (ratatui)
    Tui,

}