#![deny(
    clippy::all,
    clippy::pedantic,
    clippy::perf,
    clippy::style,
    clippy::missing_safety_doc,
    clippy::missing_const_for_fn
)]
#![allow(clippy::as_conversions, clippy::mod_module_files)]

use std::{error, path::PathBuf, process};

mod app;
mod file;

use clap::{AppSettings, Parser, Subcommand};
use log::{info, trace};
use seb::{
    ast::Biblio,
    format::{BibTex, Reader, Writer},
};

fn main() {
    if let Err(err) = try_main() {
        eprintln!("{}", err);
        process::exit(2);
    }
}

fn try_main() -> Result<(), Box<dyn error::Error>> {
    let Cli {
        command,
        file,
        verbosity,
        quiet,
    } = Cli::parse();

    setup_errlog(verbosity as usize, quiet)?;

    let mut file = file::open_or_create_format_file::<BibTex>(file)?;
    let mut biblio = file.read_ast()?;

    let message = command.execute(&mut biblio)?;

    if biblio.dirty() {
        trace!("Updating the bibliography file..");
        file.write_ast(biblio)?;
        trace!("Done!");
    }
    println!("{message}");
    Ok(())
}

fn setup_errlog(verbosity: usize, quiet: bool) -> Result<(), Box<dyn error::Error>> {
    // if quiet then ignore verbosity but still show errors
    let verbosity = if quiet {
        dbg!("quiet flag used but dbg! and error will still be shown");
        1
    } else {
        verbosity + 2
    };

    stderrlog::new().verbosity(verbosity).init()?;
    Ok(())
}

#[derive(Parser)]
#[clap(name = "seb")]
#[clap(about = "Search and edit bibliographic entries to a supported format file in the terminal")]
#[clap(version, author)]
struct Cli {
    #[clap(subcommand)]
    command: Commands,

    /// The name of the file
    #[clap(short, long, parse(from_os_str))]
    file: Option<PathBuf>,

    /// How chatty the program is when performing commands
    ///
    /// The number of times this flag is used will increase how chatty
    /// the program is.
    #[clap(short, long, parse(from_occurrences))]
    verbosity: u8,

    /// Prevents the program from writing to stdout, errors will still be printed to stderr.
    #[clap(short, long)]
    quiet: bool,
}

#[derive(Subcommand)]
#[non_exhaustive]
enum Commands {
    /// Add an entry to the current bibliography file
    #[clap(setting(AppSettings::ArgRequiredElseHelp))]
    Add {
        #[clap(subcommand)]
        command: AddCommands,
    },
    /// Remove an entry from the bibliography file using the cite key
    #[clap(setting(AppSettings::ArgRequiredElseHelp))]
    Rm {
        /// The cite key of the entry to remove
        cite: String,
    },
}

#[derive(Subcommand)]
enum AddCommands {
    /// Search for reference by doi
    #[clap(setting(AppSettings::ArgRequiredElseHelp))]
    Doi {
        /// The doi to search for
        doi: String,

        /// The cite key of the new entry
        ///
        /// This will override any citation key either present or generated by seb.
        #[clap(long)]
        cite: Option<String>,

        /// Auto selects the first bibliographic entry found on search.
        ///
        /// This will select the very first option in this list of found entries on a search,
        /// for searches by doi, isbn and other unique identifiers this should lead to predicatable
        /// results (depends on the API).
        #[clap(long)]
        confirm: bool,
    },
    /// Search for entry by IETF RFC number
    #[clap(setting(AppSettings::ArgRequiredElseHelp))]
    Ietf {
        /// The RFC number to search for
        rfc_number: usize,

        /// The cite key of the new entry
        ///
        /// This will override any citation key either present or generated by seb.
        #[clap(long)]
        cite: Option<String>,

        /// Auto selects the first bibliographic entry found on search.
        ///
        /// This will select the very first option in this list of found entries on a search,
        /// for searches by doi, isbn and other unique identifiers this should lead to predicatable
        /// results (depends on the API).
        #[clap(long)]
        confirm: bool,
    },
    /// Search for reference by ISBN
    #[clap(setting(AppSettings::ArgRequiredElseHelp))]
    Isbn {
        /// The ISBN to search for
        isbn: String,

        /// The cite key of the new entry
        ///
        /// This will override any citation key either present or generated by seb.
        #[clap(long)]
        cite: Option<String>,

        /// Auto selects the first bibliographic entry found on search.
        ///
        /// This will select the very first option in this list of found entries on a search,
        /// for searches by doi, isbn and other unique identifiers this should lead to predicatable
        /// results (depends on the API).
        #[clap(long)]
        confirm: bool,
    },
}

impl AddCommands {
    fn execute(self, biblio: &mut Biblio) -> eyre::Result<String> {
        let (mut entries, cite, confirm) = match self {
            AddCommands::Doi { doi, cite, confirm } => {
                dbg!("doi subcommand called with value of '{}", &doi);
                trace!("Checking current bibliography for possible duplicate doi..");
                app::check_entry_field_duplication(biblio, "doi", &doi)?;
                trace!("No duplicate found!");
                (seb::entries_by_doi(&doi)?, cite, confirm)
            }
            AddCommands::Ietf {
                rfc_number,
                cite,
                confirm,
            } => {
                dbg!("ietf subcommand called with value of '{}", &rfc_number);
                trace!("Checking current bibliography for possible duplicate RFC number..");
                app::check_entry_field_duplication(biblio, "number", &rfc_number.to_string())?;
                trace!("No duplicate found!");
                (seb::entries_by_rfc(rfc_number)?, cite, confirm)
            }
            AddCommands::Isbn {
                isbn,
                cite,
                confirm,
            } => {
                dbg!("isbn subcommand called with value of '{}", &isbn);
                trace!("Checking current bibliography for possible duplicate ISBN.");
                app::check_entry_field_duplication(biblio, "isbn", &isbn)?;
                trace!("No duplicate found!");
                (seb::entries_by_isbn(&isbn)?, cite, confirm)
            }
        };

        if entries.is_empty() {
            return Ok("No entries found!".to_owned());
        }

        let mut entry = if confirm {
            info!("--confirm used - picking the first entry found..");
            // remove(0) won't panic because of the is_empty check above!
            entries.remove(0)
        } else {
            app::user_select(entries)?
        };

        if let Some(cite) = cite {
            info!("Overriding cite key value with '{cite}'");
            entry.set_cite(cite);
        }
        let cite_key = entry.cite().to_owned();

        biblio.insert(entry);

        Ok(format!(
            "Entry added to bibliography with cite key:\n{cite_key}"
        ))
    }
}

impl Commands {
    fn execute(self, biblio: &mut Biblio) -> eyre::Result<String> {
        match self {
            Commands::Add { command } => command.execute(biblio),
            Commands::Rm { cite } => {
                dbg!("rm subcommand called with the value of '{cite}'");
                trace!("Checking current bibliography for entry with this cite key..");
                if biblio.remove(&cite).is_some() {
                    Ok("Entry removed from bibliography".to_owned())
                } else {
                    Ok(format!("No entry found with the cite key of '{cite}'"))
                }
            }
        }
    }
}
