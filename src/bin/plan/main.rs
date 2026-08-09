use clap::Parser;
use std::path::PathBuf;
use tablethat_lib as lib;

#[path = "../../plan/tasks.rs"]
mod tasks;
#[path = "../../plan/tui_kanban.rs"]
mod tui_kanban;

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

    let mut cfg = lib::Config::load("plan", "PLAN_", cli.config.as_deref());
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
            tasks::list_tasks(
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
            tasks::list_tasks(
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
        }) => {
            let ok = tasks::create_task(&root, &slug, &status, &type_, &priority, &area);
            std::process::exit(if ok { 0 } else { 1 });
        }
        Some(Commands::Open { slug }) => {
            let path = match tasks::resolve_single_slug(&root, &slug) {
                Some(p) => p,
                None => {
                    eprintln!("no task matching '{slug}'");
                    std::process::exit(1);
                }
            };
            let ok = tasks::open_task(&path, cfg.pager_width);
            std::process::exit(if ok { 0 } else { 1 });
        }
        Some(Commands::Delete { slug }) => {
            let path = match tasks::resolve_single_slug(&root, &slug) {
                Some(p) => p,
                None => {
                    eprintln!("no task matching '{slug}'");
                    std::process::exit(1);
                }
            };
            let ok = tasks::delete_task(&path);
            std::process::exit(if ok { 0 } else { 1 });
        }
        Some(Commands::Browse {
            status,
            type_,
            priority,
            area,
            search,
        }) => {
            tui_kanban::run_tui(
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
            tasks::init_plan(&root);
        }
        Some(Commands::Lint) => {
            let ok = tasks::validate_all(&root);
            std::process::exit(if ok { 0 } else { 1 });
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
                    let mut ok = true;
                    for entry in &entries {
                        if !tasks::format_file(entry, 120) {
                            ok = false;
                        }
                    }
                    std::process::exit(if ok { 0 } else { 1 });
                } else if p.is_file() {
                    let ok = tasks::format_file(&p, 120);
                    std::process::exit(if ok { 0 } else { 1 });
                } else {
                    eprintln!("{}: not a file or directory", p.display());
                    std::process::exit(1);
                }
            }
            None => {
                let ok = tasks::normalize_all(&root);
                std::process::exit(if ok { 0 } else { 1 });
            }
        },
        None => {
            // Default view from config
            match cfg.default_view.as_str() {
                "kanban" => {
                    tasks::list_tasks(&root, &cfg, None, None, None, None, None, true, &[]);
                }
                _ => {
                    tasks::list_tasks(&root, &cfg, None, None, None, None, None, false, &[]);
                }
            }
        }
    }
}
