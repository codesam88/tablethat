use clap::Parser;
use std::path::PathBuf;
use tablethat_lib as lib;

#[derive(Parser)]
#[command(
    name = "plan",
    version,
    max_term_width = 80,
    about = "Task management with kanban TUI\nCommands can be abbreviated to their first letter (e.g., `plan b` for browse)"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Path to config file (default: auto-detect)
    #[arg(long, global = true, value_name = "PATH")]
    config: Option<PathBuf>,

    /// Repo root containing .plan (default: auto-detect)
    #[arg(short, long, global = true, value_name = "PATH")]
    root: Option<PathBuf>,

    /// Show done items (hidden by default)
    #[arg(long, global = true)]
    show_done: bool,
}

#[derive(Parser)]
enum Commands {
    /// Kanban board view (default)
    #[command(alias = "k")]
    Kanban {
        /// Filter by status
        #[arg(short, long, value_name = "STATUS")]
        status: Option<String>,

        /// Filter by type
        #[arg(short = 't', long = "type", value_name = "TYPE")]
        type_: Option<String>,

        /// Filter by priority
        #[arg(short, long, value_name = "PRIORITY")]
        priority: Option<String>,

        /// Filter by area label
        #[arg(short, long, value_name = "AREA")]
        area: Option<String>,

        /// Search slug and body text
        #[arg(short = 'q', long, value_name = "QUERY")]
        search: Option<String>,

        /// Sort key(s), repeatable (status, type, priority, area, slug)
        #[arg(short = 'S', long = "sort", value_name = "FIELD")]
        sort: Vec<String>,
    },

    /// Interactive kanban browser
    #[command(alias = "b")]
    Browse {
        /// Filter by status
        #[arg(short, long, value_name = "STATUS")]
        status: Option<String>,

        /// Filter by type
        #[arg(short = 't', long = "type", value_name = "TYPE")]
        type_: Option<String>,

        /// Filter by priority
        #[arg(short, long, value_name = "PRIORITY")]
        priority: Option<String>,

        /// Filter by area label
        #[arg(short, long, value_name = "AREA")]
        area: Option<String>,

        /// Search slug and body text
        #[arg(short = 'q', long, value_name = "QUERY")]
        search: Option<String>,
    },

    /// List tasks in a table
    #[command(alias = "l")]
    List {
        /// Filter by status
        #[arg(short, long, value_name = "STATUS")]
        status: Option<String>,

        /// Filter by type
        #[arg(short = 't', long = "type", value_name = "TYPE")]
        type_: Option<String>,

        /// Filter by priority
        #[arg(short, long, value_name = "PRIORITY")]
        priority: Option<String>,

        /// Filter by area label
        #[arg(short, long, value_name = "AREA")]
        area: Option<String>,

        /// Search slug and body text
        #[arg(short = 'q', long, value_name = "QUERY")]
        search: Option<String>,

        /// Sort key(s), repeatable (status, type, priority, area, slug)
        #[arg(short = 'S', long = "sort", value_name = "FIELD")]
        sort: Vec<String>,
    },

    /// Initialize a .plan/ directory with schema and template
    Init,

    /// Create a new task
    #[command(alias = "a")]
    Add {
        /// Task slug (filename without .md)
        slug: String,

        /// Initial status
        #[arg(short, long, default_value = "open", value_name = "STATUS")]
        status: String,

        /// Task type
        #[arg(
            short = 't',
            long = "type",
            default_value = "feature",
            value_name = "TYPE"
        )]
        type_: String,

        /// Priority level
        #[arg(short, long, default_value = "medium", value_name = "PRIORITY")]
        priority: String,

        /// Area label
        #[arg(long, default_value = "", value_name = "AREA")]
        area: String,
    },

    /// Open a task in your editor
    #[command(alias = "o")]
    Open {
        /// Task slug (prefix/substring match)
        slug: String,
    },

    /// Delete a task
    #[command(alias = "d")]
    Delete {
        /// Task slug (prefix/substring match)
        slug: String,
    },

    /// Validate task frontmatter
    Lint,

    /// Format task markdown (frontmatter + body)
    Format {
        /// File or directory to format (default: .plan/*.md)
        path: Option<PathBuf>,
    },
}

fn run(cli: Cli) -> Result<(), String> {
    let mut cfg = lib::Config::try_load("plan", "PLAN_", cli.config.as_deref())?;
    if cli.root.is_some() {
        cfg.root = cli.root;
    }
    if cli.show_done {
        cfg.hide_done = false;
    }
    let root = cfg.root.clone().unwrap_or_else(lib::workspace_root);

    match cli.command {
        Some(Commands::List {
            status,
            type_,
            priority,
            area,
            search,
            sort,
        }) => {
            tablethat_lib::plan::display::list_tasks(
                &root,
                &cfg,
                status.as_deref(),
                type_.as_deref(),
                priority.as_deref(),
                area.as_deref(),
                search.as_deref(),
                false,
                &sort,
            )?;
            Ok(())
        }
        Some(Commands::Kanban {
            status,
            type_,
            priority,
            area,
            search,
            sort,
        }) => {
            tablethat_lib::plan::display::list_tasks(
                &root,
                &cfg,
                status.as_deref(),
                type_.as_deref(),
                priority.as_deref(),
                area.as_deref(),
                search.as_deref(),
                true,
                &sort,
            )?;
            Ok(())
        }
        Some(Commands::Add {
            slug,
            status,
            type_,
            priority,
            area,
        }) => match tablethat_lib::plan::store::create_task(
            &root, &slug, &status, &type_, &priority, &area,
        ) {
            Ok(path) => {
                eprintln!("created {}", path.display());
                Ok(())
            }
            Err(e) => Err(e),
        },
        Some(Commands::Open { slug }) => {
            let path = match tablethat_lib::plan::store::resolve_single_slug(&root, &slug) {
                Some(p) => p,
                None => return Err(format!("no task matching '{slug}'")),
            };
            tablethat_lib::plan::store::open_task(&path, cfg.pager_width)?;
            Ok(())
        }
        Some(Commands::Delete { slug }) => {
            let path = match tablethat_lib::plan::store::resolve_single_slug(&root, &slug) {
                Some(p) => p,
                None => return Err(format!("no task matching '{slug}'")),
            };
            match tablethat_lib::plan::store::delete_task(&path) {
                Ok(deleted) => {
                    eprintln!("deleted {}", deleted.display());
                    Ok(())
                }
                Err(e) => Err(e),
            }
        }
        Some(Commands::Browse {
            status,
            type_,
            priority,
            area,
            search,
        }) => {
            tablethat_lib::plan::tui::run_tui(
                &root,
                &cfg,
                status.as_deref(),
                type_.as_deref(),
                priority.as_deref(),
                area.as_deref(),
                search.as_deref(),
            );
            Ok(())
        }
        Some(Commands::Init) => {
            tablethat_lib::plan::store::init_plan(&root);
            Ok(())
        }
        Some(Commands::Lint) => {
            let mut warnings = Vec::new();
            match tablethat_lib::plan::lint::validate_all(&root, &mut warnings) {
                Ok(()) => {}
                Err(errors) => {
                    let mut all = errors;
                    all.extend(warnings);
                    return Err(all.join("\n"));
                }
            }
            for w in &warnings {
                eprintln!("{w}");
            }
            Ok(())
        }
        Some(Commands::Format { path }) => match path {
            Some(p) => {
                if p.is_dir() {
                    let entries: Vec<PathBuf> = std::fs::read_dir(&p)
                        .into_iter()
                        .flatten()
                        .filter_map(|e| e.ok())
                        .map(|e| e.path())
                        .filter(|p| p.extension().is_some_and(|ext| ext == "md"))
                        .collect();
                    let mut errors: Vec<String> = Vec::new();
                    for entry in &entries {
                        match tablethat_lib::plan::format::format_file(entry, 120) {
                            Ok(true) => eprintln!("formatted {}", entry.display()),
                            Ok(false) => {}
                            Err(e) => errors.push(e),
                        }
                    }
                    if !errors.is_empty() {
                        return Err(errors.join("\n"));
                    }
                    Ok(())
                } else if p.is_file() {
                    match tablethat_lib::plan::format::format_file(&p, 120) {
                        Ok(true) => {
                            eprintln!("formatted {}", p.display());
                            Ok(())
                        }
                        Ok(false) => Ok(()),
                        Err(e) => Err(e),
                    }
                } else {
                    Err(format!("{}: not a file or directory", p.display()))
                }
            }
            None => match tablethat_lib::plan::format::normalize_all(&root) {
                Ok(0) => {
                    eprintln!("all files already canonical");
                    Ok(())
                }
                Ok(n) => {
                    eprintln!("fixed {n} file(s)");
                    Ok(())
                }
                Err(errors) => Err(errors.join("\n")),
            },
        },
        None => {
            // Default view from config
            match cfg.default_view.as_str() {
                "kanban" => {
                    tablethat_lib::plan::display::list_tasks(
                        &root,
                        &cfg,
                        None,
                        None,
                        None,
                        None,
                        None,
                        true,
                        &[],
                    )?;
                }
                _ => {
                    tablethat_lib::plan::display::list_tasks(
                        &root,
                        &cfg,
                        None,
                        None,
                        None,
                        None,
                        None,
                        false,
                        &[],
                    )?;
                }
            }
            Ok(())
        }
    }
}

fn main() {
    let cli = Cli::parse();
    if let Err(e) = run(cli) {
        eprintln!("plan: {e}");
        std::process::exit(1);
    }
}
