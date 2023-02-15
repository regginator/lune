use std::process::ExitCode;

use anyhow::Result;
use clap::{CommandFactory, Parser};

use lune::Lune;
use tokio::fs::{read_to_string, write};

use crate::{
    gen::generate_docs_json_from_definitions,
    utils::{
        files::find_parse_file_path,
        listing::{find_lune_scripts, print_lune_scripts, sort_lune_scripts},
    },
};

pub(crate) const FILE_NAME_SELENE_TYPES: &str = "lune.yml";
pub(crate) const FILE_NAME_LUAU_TYPES: &str = "luneTypes.d.luau";
pub(crate) const FILE_NAME_DOCS: &str = "luneDocs.json";

pub(crate) const FILE_CONTENTS_SELENE_TYPES: &str = include_str!("../../../lune.yml");
pub(crate) const FILE_CONTENTS_LUAU_TYPES: &str = include_str!("../../../luneTypes.d.luau");

/// A Luau script runner
#[derive(Parser, Debug, Default, Clone)]
#[command(version, long_about = None)]
#[allow(clippy::struct_excessive_bools)]
pub struct Cli {
    /// Script name or full path to the file to run
    script_path: Option<String>,
    /// Arguments to pass to the script, stored in process.args
    script_args: Vec<String>,
    /// List scripts found inside of a nearby `lune` directory
    #[clap(long, short = 'l')]
    list: bool,
    /// Generate a Selene type definitions file in the current dir
    #[clap(long)]
    generate_selene_types: bool,
    /// Generate a Luau type definitions file in the current dir
    #[clap(long)]
    generate_luau_types: bool,
    /// Generate a Lune documentation file for Luau LSP
    #[clap(long)]
    generate_docs_file: bool,
    /// Generate the full Lune wiki directory
    #[clap(long, hide = true)]
    generate_wiki_dir: bool,
}

#[allow(dead_code)]
impl Cli {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_path<S>(mut self, path: S) -> Self
    where
        S: Into<String>,
    {
        self.script_path = Some(path.into());
        self
    }

    pub fn with_args<A>(mut self, args: A) -> Self
    where
        A: Into<Vec<String>>,
    {
        self.script_args = args.into();
        self
    }

    pub fn generate_selene_types(mut self) -> Self {
        self.generate_selene_types = true;
        self
    }

    pub fn generate_luau_types(mut self) -> Self {
        self.generate_luau_types = true;
        self
    }

    pub fn generate_docs_file(mut self) -> Self {
        self.generate_docs_file = true;
        self
    }

    pub fn list(mut self) -> Self {
        self.list = true;
        self
    }

    pub async fn run(self) -> Result<ExitCode> {
        // List files in `lune` and `.lune` directories, if wanted
        // This will also exit early and not run anything else
        if self.list {
            match find_lune_scripts().await {
                Ok(scripts) => {
                    let sorted = sort_lune_scripts(scripts);
                    if sorted.is_empty() {
                        println!("No scripts found.");
                    } else {
                        print!("Available scripts:");
                        print_lune_scripts(sorted)?;
                    }
                    return Ok(ExitCode::SUCCESS);
                }
                Err(e) => {
                    eprintln!("{e}");
                    return Ok(ExitCode::FAILURE);
                }
            }
        }
        // Generate (save) definition files, if wanted
        let generate_file_requested =
            self.generate_selene_types || self.generate_luau_types || self.generate_docs_file;
        if generate_file_requested {
            if self.generate_selene_types {
                generate_and_save_file(FILE_NAME_SELENE_TYPES, "Selene type definitions", || {
                    Ok(FILE_CONTENTS_SELENE_TYPES.to_string())
                })
                .await?;
            }
            if self.generate_luau_types {
                generate_and_save_file(FILE_NAME_LUAU_TYPES, "Luau type definitions", || {
                    Ok(FILE_CONTENTS_LUAU_TYPES.to_string())
                })
                .await?;
            }
            if self.generate_docs_file {
                generate_and_save_file(FILE_NAME_DOCS, "Luau LSP documentation", || {
                    let docs = &generate_docs_json_from_definitions(
                        FILE_CONTENTS_LUAU_TYPES,
                        "roblox/global",
                    )?;
                    Ok(serde_json::to_string_pretty(docs)?)
                })
                .await?;
            }
        }
        if self.script_path.is_none() {
            // Only generating typedefs without running a script is completely
            // fine, and we should just exit the program normally afterwards
            if generate_file_requested {
                return Ok(ExitCode::SUCCESS);
            }
            // HACK: We know that we didn't get any arguments here but since
            // script_path is optional clap will not error on its own, to fix
            // we will duplicate the cli command and make arguments required,
            // which will then fail and print out the normal help message
            let cmd = Cli::command();
            cmd.arg_required_else_help(true).get_matches();
        }
        // Parse and read the wanted file
        let file_path = find_parse_file_path(&self.script_path.unwrap())?;
        let file_contents = read_to_string(&file_path).await?;
        // Display the file path relative to cwd with no extensions in stack traces
        let file_display_name = file_path.with_extension("").display().to_string();
        // Create a new lune object with all globals & run the script
        let lune = Lune::new().with_all_globals_and_args(self.script_args);
        let result = lune.run(&file_display_name, &file_contents).await;
        Ok(match result {
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
            Ok(code) => code,
        })
    }
}

async fn generate_and_save_file(
    file_path: &str,
    display_name: &str,
    f: impl Fn() -> Result<String>,
) -> Result<()> {
    #[cfg(test)]
    use crate::tests::fmt_path_relative_to_workspace_root;
    match f() {
        Ok(file_contents) => {
            write(file_path, file_contents).await?;
            #[cfg(not(test))]
            println!("Generated {display_name} file at '{file_path}'");
            #[cfg(test)]
            println!(
                "Generated {display_name} file at '{}'",
                fmt_path_relative_to_workspace_root(file_path)
            );
        }
        Err(e) => {
            #[cfg(not(test))]
            println!("Failed to generate {display_name} file at '{file_path}'\n{e}");
            #[cfg(test)]
            println!(
                "Failed to generate {display_name} file at '{}'\n{e}",
                fmt_path_relative_to_workspace_root(file_path)
            );
        }
    }
    Ok(())
}