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

fn main() {
    let cli = Cli::parse();

    let mut cfg = match lib::Config::try_load("plan", "PLAN_", cli.config.as_deref()) {
        Ok(cfg) => cfg,
        Err(err) => {
            eprintln!("plan: {err}");
            std::process::exit(1);
        }
    };
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
            tablethat_lib::plan::tasks::list_tasks(
                &root,
                &cfg,
                status.as_deref(),
                type_.as_deref(),
                priority.as_deref(),
                area.as_deref(),
                search.as_deref(),
                false,
                &sort,
            );
        }
        Some(Commands::Kanban {
            status,
            type_,
            priority,
            area,
            search,
            sort,
        }) => {
            tablethat_lib::plan::tasks::list_tasks(
                &root,
                &cfg,
                status.as_deref(),
                type_.as_deref(),
                priority.as_deref(),
                area.as_deref(),
                search.as_deref(),
                true,
                &sort,
            );
        }
        Some(Commands::Add {
            slug,
            status,
            type_,
            priority,
            area,
        }) => match tablethat_lib::plan::tasks::create_task(
            &root, &slug, &status, &type_, &priority, &area,
        ) {
            Ok(path) => {
                eprintln!("created {}", path.display());
            }
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        },
        Some(Commands::Open { slug }) => {
            let path = match tablethat_lib::plan::tasks::resolve_single_slug(&root, &slug) {
                Some(p) => p,
                None => {
                    eprintln!("no task matching '{slug}'");
                    std::process::exit(1);
                }
            };
            match tablethat_lib::plan::tasks::open_task(&path, cfg.pager_width) {
                Ok(()) => {}
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            }
        }
        Some(Commands::Delete { slug }) => {
            let path = match tablethat_lib::plan::tasks::resolve_single_slug(&root, &slug) {
                Some(p) => p,
                None => {
                    eprintln!("no task matching '{slug}'");
                    std::process::exit(1);
                }
            };
            match tablethat_lib::plan::tasks::delete_task(&path) {
                Ok(deleted) => {
                    eprintln!("deleted {}", deleted.display());
                }
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
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
        }
        Some(Commands::Init) => {
            tablethat_lib::plan::tasks::init_plan(&root);
        }
        Some(Commands::Lint) => {
            let mut warnings = Vec::new();
            match tablethat_lib::plan::lint::validate_all(&root, &mut warnings) {
                Ok(()) => {}
                Err(errors) => {
                    for e in &errors {
                        eprintln!("{e}");
                    }
                    for w in &warnings {
                        eprintln!("{w}");
                    }
                    std::process::exit(1);
                }
            }
            for w in &warnings {
                eprintln!("{w}");
            }
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
                    let mut failed = false;
                    for entry in &entries {
                        match tablethat_lib::plan::tasks::format_file(entry, 120) {
                            Ok(true) => eprintln!("formatted {}", entry.display()),
                            Ok(false) => {}
                            Err(e) => {
                                eprintln!("{e}");
                                failed = true;
                            }
                        }
                    }
                    std::process::exit(if failed { 1 } else { 0 });
                } else if p.is_file() {
                    match tablethat_lib::plan::tasks::format_file(&p, 120) {
                        Ok(true) => eprintln!("formatted {}", p.display()),
                        Ok(false) => {}
                        Err(e) => {
                            eprintln!("{e}");
                            std::process::exit(1);
                        }
                    }
                } else {
                    eprintln!("{}: not a file or directory", p.display());
                    std::process::exit(1);
                }
            }
            None => match tablethat_lib::plan::tasks::normalize_all(&root) {
                Ok(0) => eprintln!("all files already canonical"),
                Ok(n) => eprintln!("fixed {n} file(s)"),
                Err(errors) => {
                    for e in &errors {
                        eprintln!("{e}");
                    }
                    std::process::exit(1);
                }
            },
        },
        None => {
            // Default view from config
            match cfg.default_view.as_str() {
                "kanban" => {
                    tablethat_lib::plan::tasks::list_tasks(
                        &root,
                        &cfg,
                        None,
                        None,
                        None,
                        None,
                        None,
                        true,
                        &[],
                    );
                }
                _ => {
                    tablethat_lib::plan::tasks::list_tasks(
                        &root,
                        &cfg,
                        None,
                        None,
                        None,
                        None,
                        None,
                        false,
                        &[],
                    );
                }
            }
        }
    }
}
