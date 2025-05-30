pub mod compile;
pub mod export;
pub mod font;
#[cfg(feature = "gen-manual")]
pub mod manual;
pub mod query;
pub mod query_repl;
pub mod utils;
pub mod version;

use core::fmt;
use std::{
    borrow::Cow,
    path::{Path, PathBuf},
    sync::OnceLock,
};

use clap::{builder::ValueParser, ArgAction, Args, Command, Parser, Subcommand, ValueEnum};
use reflexo_typst::{
    build_info::VERSION, vfs::WorkspaceResolver, DiagnosticHandler, ImmutPath, TypstFileId,
    MEMORY_MAIN_ENTRY,
};
use typst::syntax::VirtualPath;
use utils::current_dir;
use version::VersionFormat;

/// The character typically used to separate path components
/// in environment variables.
const ENV_PATH_SEP: char = if cfg!(windows) { ';' } else { ':' };

#[derive(Debug, Parser)]
#[clap(name = "typst-ts-cli", version = VERSION)]
pub struct Opts {
    /// Print Version
    #[arg(short = 'V', long, group = "version-dump")]
    pub version: bool,

    /// Print Version in format
    #[arg(long = "VV", alias = "version-fmt", group = "version-dump", default_value_t = VersionFormat::None)]
    pub vv: VersionFormat,

    #[clap(subcommand)]
    pub sub: Option<Subcommands>,
}

#[derive(Debug, Subcommand)]
#[clap(
    about = "The cli for typst.ts.",
    after_help = "",
    next_display_order = None
)]
#[allow(clippy::large_enum_variant)]
pub enum Subcommands {
    /// Compiles or Watches an entry file into one or multiple supported output
    /// format(s)
    #[clap(visible_alias = "c")]
    Compile(CompileArgs),

    /// Processes an input file to extract provided metadata
    Query(QueryArgs),

    /// Runs repl for query
    QueryRepl(QueryReplArgs),

    /// Generates a shell completion script for CLI.
    Completion(CompletionArgs),

    /// Generates a manual for CLI.
    Manual(ManualArgs),

    /// Dumps identified client environment of CLI.
    Env(EnvArgs),

    /// Font commands
    #[clap(subcommand)]
    Font(FontSubCommands),

    /// Package commands
    #[clap(subcommand)]
    Package(PackageSubCommands),
}

#[derive(Debug, Subcommand)]
#[clap(
    about = "Font commands about font for typst.",
    after_help = "",
    next_display_order = None
)]
#[allow(clippy::large_enum_variant)]
pub enum FontSubCommands {
    /// List all discovered fonts in system and custom font paths
    List(ListFontsArgs),
}

#[derive(Debug, Subcommand)]
#[clap(
    about = "Package commands about package for typst.",
    after_help = "",
    next_display_order = None
)]
#[allow(clippy::large_enum_variant)]
pub enum PackageSubCommands {
    /// Lists all discovered packages in data and cache paths
    List(ListPackagesArgs),
    /// Links a package to local data path
    Link(LinkPackagesArgs),
    /// Unlinks a package from local data path
    Unlink(LinkPackagesArgs),
    /// Generates documentation for a package
    Doc(GenPackagesDocArgs),
}

/// Shared arguments for font related commands
#[derive(Default, Debug, Clone, Parser)]
pub struct FontArgs {
    /// Add additional directories to search for fonts
    #[clap(
        long = "font-path",
        env = "TYPST_FONT_PATHS", 
        value_name = "DIR",
        value_delimiter = ENV_PATH_SEP,
    )]
    pub paths: Vec<PathBuf>,
}

#[derive(Default, Debug, Clone, Parser)]
#[clap(next_help_heading = "Compile options")]
pub struct CompileOnceArgs {
    /// Shared arguments for font related commands.
    #[clap(flatten)]
    pub font: FontArgs,

    /// Path to typst workspace.
    #[clap(long, short, default_value = ".")]
    pub workspace: String,

    /// Path to input Typst file, use `-` to read input from stdin
    #[clap(long, short, required = true)]
    pub entry: String,

    /// Add a string key-value pair visible through `sys.inputs`
    #[clap(
        long = "input",
        value_name = "key=value",
        action = ArgAction::Append,
        value_parser = ValueParser::new(parse_input_pair),
    )]
    pub inputs: Vec<(String, String)>,

    /// Output to directory, default in the same directory as the entry file.
    #[clap(long, short, default_value = "")]
    pub output: String,

    #[clap(skip)]
    pub extra_embedded_fonts: Vec<Cow<'static, [u8]>>,

    /// The root of workspace of the compilation.
    #[clap(skip)]
    pub parsed_entry: OnceLock<ImmutPath>,

    /// The root of workspace of the compilation.
    #[clap(skip)]
    pub main_id: OnceLock<TypstFileId>,

    /// The output directory of the compilation.
    #[clap(skip)]
    pub parsed_output: OnceLock<ImmutPath>,
}

impl CompileOnceArgs {
    pub fn is_stdin(&self) -> bool {
        self.entry == "-"
    }

    pub fn root(&self) -> &ImmutPath {
        self.parsed_entry.get_or_init(|| {
            let root = Path::new(&self.workspace);

            if root.is_absolute() {
                root.into()
            } else {
                current_dir().join(root).into()
            }
        })
    }

    pub fn main_id(&self) -> &TypstFileId {
        self.main_id.get_or_init(|| {
            if self.is_stdin() {
                *MEMORY_MAIN_ENTRY
            } else {
                let root = self.root();
                let entry = Path::new(&self.entry);

                let entry = if entry.is_absolute() {
                    entry.to_owned()
                } else {
                    current_dir().join(entry)
                };

                let path = match entry.strip_prefix(root) {
                    Ok(rel) => VirtualPath::new(rel),
                    Err(_) => clap::Error::raw(
                        clap::error::ErrorKind::InvalidValue,
                        format!(
                            "entry file path must be in workspace directory: {workspace_dir}\n",
                            workspace_dir = root.display()
                        ),
                    )
                    .exit(),
                };

                WorkspaceResolver::workspace_file(Some(root), path)
            }
        })
    }

    pub fn output_dir(&self) -> &ImmutPath {
        self.parsed_output.get_or_init(|| {
            let input = self.main_id();

            if self.output.is_empty() {
                if self.is_stdin() {
                    current_dir().into()
                } else {
                    input
                        .vpath()
                        .as_rooted_path()
                        .parent()
                        .unwrap_or_else(|| {
                            clap::Error::raw(
                                clap::error::ErrorKind::InvalidValue,
                                "entry file has no parent",
                            )
                            .exit()
                        })
                        .into()
                }
            } else {
                Path::new(&self.output).into()
            }
        })
    }
}

/// Parses key/value pairs split by the first equal sign.
///
/// This function will return an error if the argument contains no equals sign
/// or contains the key (before the equals sign) is empty.
fn parse_input_pair(raw: &str) -> Result<(String, String), String> {
    let (key, val) = raw
        .split_once('=')
        .ok_or("input must be a key and a value separated by an equal sign")?;
    let key = key.trim().to_owned();
    if key.is_empty() {
        return Err("the key was missing or empty".to_owned());
    }
    let val = val.trim().to_owned();
    Ok((key, val))
}

#[derive(Default, Debug, Clone, Parser)]
#[clap(next_help_heading = "Export options")]
pub struct ExportArgs {
    /// The document's creation date formatted as a UNIX timestamp.
    ///
    /// For more information, see <https://reproducible-builds.org/specs/source-date-epoch/>.
    #[clap(
        long = "creation-timestamp",
        env = "SOURCE_DATE_EPOCH",
        value_name = "UNIX_TIMESTAMP"
    )]
    pub creation_timestamp: Option<i64>,
}

#[derive(Default, Debug, Clone, Parser)]
#[clap(next_help_heading = "Compile options")]
pub struct CompileArgs {
    /// compile arguments before query.
    #[clap(flatten)]
    pub compile: CompileOnceArgs,

    #[clap(flatten)]
    pub export: ExportArgs,

    /// Runs compilation in watch mode.
    #[clap(long)]
    pub watch: bool,

    /// Generates dynamic layout representation.
    /// Note: this is an experimental feature and will be merged as
    ///   format `dyn-svg` in the future.
    #[clap(long)]
    pub dynamic_layout: bool,

    /// Outputs format(s), possible values: `ast`, `pdf`, `svg`, and,
    /// `svg_html`.
    #[clap(long)]
    pub format: Vec<String>,

    /// The format to emit diagnostics in
    #[clap(
        long,
        default_value_t = DiagnosticFormat::Human,
        value_parser = clap::value_parser!(DiagnosticFormat)
    )]
    pub diagnostic_format: DiagnosticFormat,
}

impl CompileArgs {
    pub fn diagnostics_handler(&self) -> DiagnosticHandler {
        DiagnosticHandler {
            diagnostic_format: self.diagnostic_format.into(),
            print_compile_status: self.watch,
        }
    }
}

/// Processes an input file to extract provided metadata
///
/// Examples:
/// ```shell
/// # query elements with selector "heading"
/// query --selector "heading"
/// # query elements with selector "heading" which is of level 1
/// query --selector "heading.where(level: 1)"
/// # query first element with selector "heading" which is of level 1
/// query --selector "heading.where(level: 1)" --one
/// ```
#[derive(Debug, Clone, Parser)]
pub struct QueryArgs {
    /// compile arguments before query.
    #[clap(flatten)]
    pub compile: CompileArgs,

    /// Define what elements to retrieve
    #[clap(long = "selector")]
    pub selector: String,

    /// Extract just one field from all retrieved elements
    #[clap(long = "field")]
    pub field: Option<String>,

    /// Expect and retrieve exactly one element
    #[clap(long = "one", default_value = "false")]
    pub one: bool,
}

/// TODO: Repl Doc
#[derive(Debug, Clone, Parser)]
pub struct QueryReplArgs {
    /// compile arguments before query.
    #[clap(flatten)]
    pub compile: CompileOnceArgs,
}

/// List all discovered fonts in system and custom font paths
#[derive(Debug, Clone, Parser)]
pub struct ListFontsArgs {
    /// Shared arguments for font related commands.
    #[clap(flatten)]
    pub font: FontArgs,

    /// Also list style variants of each font family
    #[arg(long)]
    pub variants: bool,
}

/// Measure fonts and generate a profile file for compiler
#[derive(Debug, Clone, Parser)]
pub struct MeasureFontsArgs {
    /// Shared arguments for font related commands.
    #[clap(flatten)]
    pub font: FontArgs,

    /// Path to output profile file
    #[arg(long, required = true)]
    pub output: PathBuf,

    /// Exclude system font paths
    #[arg(long)]
    pub no_system_fonts: bool,
}

#[derive(ValueEnum, Debug, Clone)]
pub enum EnvKey {
    Features,
}

/// Generate shell completion script.
#[derive(Debug, Clone, Parser)]
pub struct CompletionArgs {
    /// Completion script kind.
    #[clap(value_enum)]
    pub shell: clap_complete::Shell,
}

/// Generate shell completion script.
#[derive(Debug, Clone, Parser)]
pub struct ManualArgs {
    /// Path to output directory
    pub dest: PathBuf,
}

/// Dump Client Environment.
#[derive(Debug, Clone, Parser)]
pub struct EnvArgs {
    /// The key of environment kind.
    #[clap(value_name = "KEY")]
    pub key: EnvKey,
}

#[derive(Debug, Clone, Parser)]
pub struct ListPackagesArgs {
    /// Also list other information of each package
    #[arg(short)]
    pub long: bool,
}

#[derive(Debug, Clone, Parser)]
pub struct LinkPackagesArgs {
    /// Path to package manifest file
    #[arg(long)]
    pub manifest: String,
}

#[derive(Debug, Clone, Parser)]
pub struct GenPackagesDocArgs {
    /// Path to package manifest file
    #[arg(long)]
    pub manifest: String,

    /// Path to output directory
    #[arg(long, short, default_value = "")]
    pub output: String,

    /// Generate dynamic layout representation.
    /// Note: this is an experimental feature and will be merged as
    ///   format `dyn-svg` in the future.
    #[clap(long)]
    pub dynamic_layout: bool,
}

/// Which format to use for diagnostics.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, ValueEnum)]
pub enum DiagnosticFormat {
    Human,
    Short,
}

impl From<DiagnosticFormat> for reflexo_typst::DiagnosticFormat {
    fn from(fmt: DiagnosticFormat) -> Self {
        match fmt {
            DiagnosticFormat::Human => Self::Human,
            DiagnosticFormat::Short => Self::Short,
        }
    }
}

impl Default for DiagnosticFormat {
    fn default() -> Self {
        Self::Human
    }
}

impl fmt::Display for DiagnosticFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.to_possible_value()
            .expect("no values are skipped")
            .get_name()
            .fmt(f)
    }
}

pub fn get_cli(sub_command_required: bool) -> Command {
    let cli = Command::new("$").disable_version_flag(true);
    Opts::augment_args(cli).subcommand_required(sub_command_required)
}
