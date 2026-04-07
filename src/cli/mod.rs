use std::collections::{HashMap, HashSet};
use std::io::IsTerminal;
use std::path::PathBuf;

use anyhow::Context;
use clap::builder::ArgGroup;
use clap::{Args, Parser, Subcommand, ValueEnum};
use clap_complete::Shell;
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, Color, ContentArrangement, Row as ComfyRow, Table};
use serde::Serialize;

use crate::client::{
    BulkEditTransactionsResult, Category, ClientMode, CopilotClient, PageInfo, Transaction,
    TransactionIdRef,
};
use crate::config::{load_token, session_path, token_path};
use crate::types::{
    AccountId, CategoryId, RecurringFrequency, RecurringId, TagId, TransactionId, TransactionType,
};

mod accounts;
mod auth;
mod budgets;
mod categories;
mod completions;
mod networth;
mod recurrings;
mod render;
mod spending;
mod tags;
use render::{
    KeyValueRow, TableRow, header_cell, render_output, shorten_id_for_table, terminal_width,
};

#[derive(Debug, Clone, Copy, ValueEnum, Serialize, PartialEq, Eq)]
pub enum OutputFormat {
    Json,
    Table,
    Csv,
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum ColorMode {
    Auto,
    Always,
    Never,
}

#[derive(Debug, Clone, Parser)]
#[command(name = "copilot")]
#[command(about = "CLI for Copilot Money (unofficial)", version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    #[arg(long, value_enum, default_value_t = OutputFormat::Table, global = true)]
    pub output: OutputFormat,

    #[arg(long, value_enum, default_value_t = ColorMode::Auto, global = true)]
    pub color: ColorMode,

    #[arg(long, global = true)]
    pub dry_run: bool,

    /// Skip confirmation prompts for write actions (required in non-interactive runs).
    #[arg(long, global = true, default_value_t = false)]
    pub yes: bool,

    #[arg(
        long,
        global = true,
        env = "COPILOT_BASE_URL",
        default_value = "https://app.copilot.money"
    )]
    pub base_url: String,

    #[arg(long, global = true, env = "COPILOT_TOKEN")]
    pub token: Option<String>,

    #[arg(long, global = true, env = "COPILOT_TOKEN_FILE")]
    pub token_file: Option<PathBuf>,

    #[arg(long, global = true, env = "COPILOT_SESSION_DIR")]
    pub session_dir: Option<PathBuf>,

    #[arg(long, global = true, env = "COPILOT_FIXTURES_DIR", hide = true)]
    pub fixtures_dir: Option<PathBuf>,

    /// Disable automatic browser re-authentication when the token expires.
    /// By default, the CLI will open a browser to log in again if the token
    /// is expired and silent refresh fails (only in interactive terminals).
    #[arg(long, global = true, default_value_t = false)]
    pub no_auto_login: bool,
}

#[derive(Debug, Clone, Subcommand)]
pub enum Command {
    Auth {
        #[command(subcommand)]
        cmd: AuthCmd,
    },
    Transactions {
        #[command(subcommand)]
        cmd: TransactionsCmd,
    },
    Categories {
        #[command(subcommand)]
        cmd: CategoriesCmd,
    },
    Recurrings {
        #[command(subcommand)]
        cmd: RecurringsCmd,
    },
    Tags {
        #[command(subcommand)]
        cmd: TagsCmd,
    },
    Budgets {
        #[command(subcommand)]
        cmd: BudgetsCmd,
    },
    /// List and inspect connected financial accounts.
    Accounts {
        #[command(subcommand)]
        cmd: AccountsCmd,
    },
    /// Show net worth (assets, debt, total).
    Networth {
        #[command(subcommand)]
        cmd: NetworthCmd,
    },
    /// Spending summaries and history.
    Spending {
        #[command(subcommand)]
        cmd: SpendingCmd,
    },
    /// Generate shell completions for bash, zsh, fish, etc.
    Completions {
        /// Shell to generate completions for.
        #[arg(value_enum)]
        shell: Shell,
    },
    /// Start the MCP (Model Context Protocol) server over stdio.
    Mcp,
    Version,
}

#[derive(Debug, Clone, Subcommand)]
pub enum AuthCmd {
    Status,
    Login(AuthLoginArgs),
    Refresh(AuthRefreshArgs),
    SetToken(AuthSetTokenArgs),
    Logout,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum AuthLoginMode {
    /// Opens a browser and waits for you to log in.
    Interactive,
    /// Sends a magic link email; paste the link back (SSH-friendly).
    EmailLink,
    /// Uses `--secrets-file` with email+password (not recommended for open-source).
    Credentials,
}

#[derive(Debug, Clone, Args)]
pub struct AuthLoginArgs {
    #[arg(long)]
    pub secrets_file: Option<PathBuf>,

    #[arg(long, value_enum, default_value_t = AuthLoginMode::Interactive)]
    pub mode: AuthLoginMode,

    /// Required for `--mode email-link` unless it can be inferred from `--secrets-file`.
    #[arg(long)]
    pub email: Option<String>,

    #[arg(long, default_value_t = 180)]
    pub timeout_seconds: u64,

    /// Store a persistent browser session so tokens can be refreshed automatically.
    ///
    /// Disable with `--no-persist-session` (not recommended).
    #[arg(long, default_value_t = false)]
    pub persist_session: bool,

    /// Do not store a persistent browser session (tokens may expire and require re-auth).
    #[arg(long, default_value_t = false)]
    pub no_persist_session: bool,
}

#[derive(Debug, Clone, Args)]
pub struct AuthRefreshArgs {
    #[arg(long, default_value_t = 180)]
    pub timeout_seconds: u64,
}

#[derive(Debug, Clone, Args)]
pub struct AuthSetTokenArgs {
    /// Where to store the token (defaults to `~/.config/copilot-money-cli/token`)
    #[arg(long)]
    pub token_file: Option<PathBuf>,
}

#[derive(Debug, Clone, Subcommand)]
pub enum TransactionsCmd {
    List(TransactionsListArgs),
    Search(TransactionsSearchArgs),
    Show(TransactionsShowArgs),
    Review(TransactionsReviewArgs),
    Unreview(TransactionsReviewArgs),
    SetCategory(TransactionsSetCategoryArgs),
    AssignRecurring(TransactionsAssignRecurringArgs),
    SetNotes(TransactionsSetNotesArgs),
    SetTags(TransactionsSetTagsArgs),
    Edit(TransactionsEditArgs),
    /// Find potential duplicate transactions (same amount, similar date/name).
    Duplicates(TransactionsDuplicatesArgs),
}

#[derive(Debug, Clone, Subcommand)]
pub enum AccountsCmd {
    /// List connected accounts.
    List(AccountsListArgs),
    /// Show details for a specific account.
    Show {
        id: AccountId,
    },
}

#[derive(Debug, Clone, Args)]
pub struct AccountsListArgs {
    /// Filter by name substring (case-insensitive).
    #[arg(long)]
    pub name_contains: Option<String>,

    /// Filter by account type (e.g. depository, credit, investment).
    #[arg(long)]
    pub account_type: Option<String>,

    /// Include hidden accounts.
    #[arg(long, default_value_t = false)]
    pub show_hidden: bool,

    /// Include closed accounts.
    #[arg(long, default_value_t = false)]
    pub show_closed: bool,
}

#[derive(Debug, Clone, Subcommand)]
pub enum NetworthCmd {
    /// Show current net worth (live balance).
    Current,
    /// Show net worth history over time.
    History(NetworthHistoryArgs),
}

#[derive(Debug, Clone, Args)]
pub struct NetworthHistoryArgs {
    /// Time frame (e.g. "ONE_MONTH", "THREE_MONTHS", "ONE_YEAR", "ALL").
    #[arg(long)]
    pub time_frame: Option<String>,
}

#[derive(Debug, Clone, Subcommand)]
pub enum SpendingCmd {
    /// Monthly spending totals.
    Monthly,
    /// Transaction summary (total income, spent, net).
    Summary,
    /// Spending history by month with comparisons.
    History,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum TransactionsSort {
    DateDesc,
    DateAsc,
    AmountDesc,
    AmountAsc,
}

fn sort_to_graphql(sort: Option<TransactionsSort>) -> Option<serde_json::Value> {
    let s = sort?;
    let (field, direction) = match s {
        TransactionsSort::DateDesc => ("DATE", "DESC"),
        TransactionsSort::DateAsc => ("DATE", "ASC"),
        TransactionsSort::AmountDesc => ("AMOUNT", "DESC"),
        TransactionsSort::AmountAsc => ("AMOUNT", "ASC"),
    };
    Some(serde_json::json!([{ "field": field, "direction": direction }]))
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum TransactionField {
    Date,
    Name,
    Amount,
    Reviewed,
    Category,
    Tags,
    Type,
    Account,
    Notes,
    Id,
}

#[derive(Debug, Clone, Args)]
pub struct TransactionsDuplicatesArgs {
    /// Number of transactions to scan (default 200).
    #[arg(long, default_value_t = 200)]
    pub limit: usize,

    /// Date proximity in days to consider duplicates (default 3).
    #[arg(long, default_value_t = 3)]
    pub days: u32,

    /// Columns to show.
    #[arg(
        long,
        value_enum,
        value_delimiter = ',',
        default_value = "date,name,amount,account,id"
    )]
    pub fields: Vec<TransactionField>,
}

#[derive(Debug, Clone, Args)]
pub struct TransactionsListArgs {
    #[arg(long, default_value_t = 25)]
    pub limit: usize,

    /// Cursor to continue pagination from a previous call (`pageInfo.endCursor`).
    #[arg(long)]
    pub after: Option<String>,

    /// Number of pages to fetch (each page is `--limit`).
    #[arg(long, default_value_t = 1)]
    pub pages: usize,

    /// Fetch all pages until exhausted (can be slow).
    #[arg(long, default_value_t = false, conflicts_with = "pages")]
    pub all: bool,

    /// Filter to reviewed transactions only.
    #[arg(long, default_value_t = false, conflicts_with = "unreviewed")]
    pub reviewed: bool,

    /// Filter to unreviewed transactions only.
    #[arg(long, default_value_t = false, conflicts_with = "reviewed")]
    pub unreviewed: bool,

    /// Filter to a specific category id.
    #[arg(long)]
    pub category_id: Option<CategoryId>,

    /// Filter to a specific category by name (case-insensitive exact match).
    #[arg(long, conflicts_with = "category_id")]
    pub category: Option<String>,

    /// Filter to transactions that include any of these tags (repeatable).
    #[arg(long, value_name = "TAG")]
    pub tag: Vec<String>,

    /// Filter to a specific date (supports YYYY-MM-DD and MM-DD-YYYY).
    #[arg(long)]
    pub date: Option<String>,

    /// Filter transactions on or after this date. Supports YYYY-MM-DD, MM-DD-YYYY,
    /// or relative: today, yesterday, Nd (e.g. 7d, 30d), this-week, last-week,
    /// this-month, last-month.
    #[arg(long)]
    pub from: Option<String>,

    /// Filter transactions on or before this date. Same formats as --from.
    #[arg(long)]
    pub to: Option<String>,

    /// Minimum transaction amount (absolute value).
    #[arg(long)]
    pub min_amount: Option<f64>,

    /// Maximum transaction amount (absolute value).
    #[arg(long)]
    pub max_amount: Option<f64>,

    /// Filter by account name (case-insensitive substring match).
    #[arg(long, conflicts_with = "account_id")]
    pub account: Option<String>,

    /// Filter by account ID.
    #[arg(long)]
    pub account_id: Option<AccountId>,

    /// Filter by merchant/name substring (case-insensitive).
    #[arg(long)]
    pub name_contains: Option<String>,

    /// Sort transactions server-side (best-effort).
    #[arg(long, value_enum)]
    pub sort: Option<TransactionsSort>,

    /// Columns to show in table output (comma-separated).
    #[arg(
        long,
        value_enum,
        value_delimiter = ',',
        default_value = "date,name,amount,reviewed,category,tags,type"
    )]
    pub fields: Vec<TransactionField>,

    /// Include pagination info (`pageInfo`) in the output.
    #[arg(long, default_value_t = false)]
    pub page_info: bool,

    /// Show a totals summary row at the bottom.
    #[arg(long, default_value_t = false)]
    pub totals: bool,

    /// Load a saved filter preset by name (from ~/.config/copilot-money-cli/presets.json).
    #[arg(long)]
    pub preset: Option<String>,
}

#[derive(Debug, Clone, Args)]
pub struct TransactionsSearchArgs {
    /// Search query — matches against name, notes, category, and tags.
    pub query: String,

    #[arg(long, default_value_t = 200)]
    pub limit: usize,

    /// Cursor to continue pagination from a previous call (`pageInfo.endCursor`).
    #[arg(long)]
    pub after: Option<String>,

    /// Number of pages to fetch (each page is `--limit`).
    #[arg(long, default_value_t = 1)]
    pub pages: usize,

    /// Fetch all pages until exhausted (can be slow).
    #[arg(long, default_value_t = false, conflicts_with = "pages")]
    pub all: bool,

    /// Filter to reviewed transactions only.
    #[arg(long, default_value_t = false, conflicts_with = "unreviewed")]
    pub reviewed: bool,

    /// Filter to unreviewed transactions only.
    #[arg(long, default_value_t = false, conflicts_with = "reviewed")]
    pub unreviewed: bool,

    /// Filter to a specific category id.
    #[arg(long)]
    pub category_id: Option<CategoryId>,

    /// Filter to a specific category by name (case-insensitive exact match).
    #[arg(long, conflicts_with = "category_id")]
    pub category: Option<String>,

    /// Filter to transactions that include any of these tags (repeatable).
    #[arg(long, value_name = "TAG")]
    pub tag: Vec<String>,

    /// Filter to a specific date (supports YYYY-MM-DD and MM-DD-YYYY).
    #[arg(long)]
    pub date: Option<String>,

    /// Filter transactions on or after this date. Supports relative dates.
    #[arg(long)]
    pub from: Option<String>,

    /// Filter transactions on or before this date. Supports relative dates.
    #[arg(long)]
    pub to: Option<String>,

    /// Minimum transaction amount (absolute value).
    #[arg(long)]
    pub min_amount: Option<f64>,

    /// Maximum transaction amount (absolute value).
    #[arg(long)]
    pub max_amount: Option<f64>,

    /// Filter by account name (case-insensitive substring match).
    #[arg(long, conflicts_with = "account_id")]
    pub account: Option<String>,

    /// Filter by account ID.
    #[arg(long)]
    pub account_id: Option<AccountId>,

    /// Sort transactions server-side (best-effort).
    #[arg(long, value_enum)]
    pub sort: Option<TransactionsSort>,

    /// Columns to show in table output (comma-separated).
    #[arg(
        long,
        value_enum,
        value_delimiter = ',',
        default_value = "date,name,amount,reviewed,category,tags,type"
    )]
    pub fields: Vec<TransactionField>,

    /// Include pagination info (`pageInfo`) in the output.
    #[arg(long, default_value_t = false)]
    pub page_info: bool,

    /// Show a totals summary row at the bottom.
    #[arg(long, default_value_t = false)]
    pub totals: bool,
}

#[derive(Debug, Clone, Args)]
pub struct TransactionsShowArgs {
    pub id: TransactionId,

    #[arg(long, default_value_t = 200)]
    pub limit: usize,
}

#[derive(Debug, Clone, Args)]
pub struct TransactionsReviewArgs {
    pub ids: Vec<TransactionId>,
}

#[derive(Debug, Clone, Args)]
#[command(group(
    ArgGroup::new("category_target")
        .args(["category_id", "category"])
))]
pub struct TransactionsSetCategoryArgs {
    pub ids: Vec<TransactionId>,

    #[arg(long)]
    pub category_id: Option<CategoryId>,

    #[arg(long)]
    pub category: Option<String>,
}

#[derive(Debug, Clone, Args)]
pub struct TransactionsAssignRecurringArgs {
    pub ids: Vec<TransactionId>,

    #[arg(long)]
    pub recurring_id: RecurringId,
}

#[derive(Debug, Clone, Args)]
pub struct TransactionsSetNotesArgs {
    pub ids: Vec<TransactionId>,

    #[arg(long, conflicts_with = "clear")]
    pub notes: Option<String>,

    #[arg(long, default_value_t = false)]
    pub clear: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum TagUpdateMode {
    Set,
    Add,
    Remove,
}

#[derive(Debug, Clone, Args)]
pub struct TransactionsSetTagsArgs {
    pub ids: Vec<TransactionId>,

    #[arg(long, value_enum, default_value_t = TagUpdateMode::Set)]
    pub mode: TagUpdateMode,

    /// One or more tag IDs (repeatable).
    #[arg(long = "tag-id", value_name = "TAG_ID")]
    pub tag_ids: Vec<crate::types::TagId>,
}

#[derive(Debug, Clone, Args)]
#[command(group(
    ArgGroup::new("edit_input")
        .required(true)
        .args(["type_", "input_json"])
))]
pub struct TransactionsEditArgs {
    pub ids: Vec<TransactionId>,

    /// Set transaction type (best-effort; server enum values vary).
    #[arg(long = "type")]
    pub type_: Option<TransactionType>,

    /// Raw JSON to pass as EditTransactionInput (advanced).
    #[arg(long)]
    pub input_json: Option<String>,
}

#[derive(Debug, Clone, Subcommand)]
pub enum CategoriesCmd {
    List(CategoriesListArgs),
    Show { id: CategoryId },
    Create(CategoriesCreateArgs),
    Edit(CategoriesEditArgs),
}

#[derive(Debug, Clone, Args)]
pub struct CategoriesListArgs {
    /// Include spend data (current + history).
    #[arg(long, default_value_t = false)]
    pub spend: bool,

    /// Include budget data (current + history).
    #[arg(long, default_value_t = false)]
    pub budget: bool,

    /// When used with `--budget`, request rollover-enabled budgets.
    #[arg(long, default_value_t = false)]
    pub rollovers: bool,

    /// Include child categories (nested categories).
    #[arg(long, default_value_t = false)]
    pub children: bool,

    /// Filter by name substring (case-insensitive).
    #[arg(long)]
    pub name_contains: Option<String>,
}

#[derive(Debug, Clone, Args)]
pub struct CategoriesCreateArgs {
    pub name: String,

    #[arg(long)]
    pub emoji: Option<String>,

    #[arg(long)]
    pub color_name: Option<String>,

    #[arg(long, default_value_t = false)]
    pub excluded: bool,

    #[arg(long)]
    pub template_id: Option<String>,

    /// When set, include an initial budget in the category input.
    #[arg(long)]
    pub budget_unassigned_amount: Option<i64>,
}

#[derive(Debug, Clone, Args)]
pub struct CategoriesEditArgs {
    pub id: String,

    #[arg(long)]
    pub name: Option<String>,

    #[arg(long)]
    pub emoji: Option<String>,

    #[arg(long)]
    pub color_name: Option<String>,

    #[arg(long)]
    pub excluded: Option<bool>,
}

#[derive(Debug, Clone, Subcommand)]
pub enum RecurringsCmd {
    List(RecurringsListArgs),
    Show { id: RecurringId },
    Create(RecurringsCreateArgs),
    Edit(RecurringsEditArgs),
    /// Show upcoming unpaid recurring payments.
    Upcoming,
}

#[derive(Debug, Clone, Subcommand)]
pub enum TagsCmd {
    List,
    Create(TagsCreateArgs),
    Delete(TagsDeleteArgs),
}

#[derive(Debug, Clone, Args)]
pub struct TagsCreateArgs {
    pub name: String,

    #[arg(long)]
    pub color_name: Option<String>,
}

#[derive(Debug, Clone, Args)]
pub struct TagsDeleteArgs {
    pub id: crate::types::TagId,
}

#[derive(Debug, Clone, Args)]
pub struct RecurringsListArgs {
    /// Filter to a specific category id.
    #[arg(long)]
    pub category_id: Option<CategoryId>,

    /// Filter by name substring (case-insensitive).
    #[arg(long)]
    pub name_contains: Option<String>,
}

#[derive(Debug, Clone, Args)]
pub struct RecurringsCreateArgs {
    /// A transaction ID to derive the recurring rule from.
    pub transaction_id: TransactionId,

    /// Recurring frequency (best-effort; Copilot expects values like ANNUALLY, MONTHLY, etc).
    #[arg(long)]
    pub frequency: RecurringFrequency,
}

#[derive(Debug, Clone, Args)]
pub struct RecurringsEditArgs {
    pub id: RecurringId,

    #[arg(long)]
    pub name_contains: Option<String>,

    #[arg(long)]
    pub min_amount: Option<i64>,

    #[arg(long)]
    pub max_amount: Option<i64>,

    #[arg(long, default_value_t = false)]
    pub recalculate_only_for_future: bool,
}

#[derive(Debug, Clone, Subcommand)]
pub enum BudgetsCmd {
    Month,
    Set,
}

pub fn run(cli: Cli) -> anyhow::Result<()> {
    if let Command::Version = &cli.command {
        println!("copilot-money-cli {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    if let Command::Mcp = &cli.command {
        return crate::mcp::run_mcp(
            cli.token.clone(),
            cli.token_file.clone(),
            cli.session_dir.clone(),
            cli.base_url.clone(),
            cli.fixtures_dir.clone(),
        );
    }

    if let Command::Completions { shell } = &cli.command {
        return completions::run_completions(*shell);
    }

    let token_file_path = cli.token_file.clone().unwrap_or_else(token_path);
    let token = cli
        .token
        .clone()
        .or_else(|| load_token(&token_file_path).ok());

    let mode = match &cli.fixtures_dir {
        Some(dir) => ClientMode::Fixtures(dir.clone()),
        None => ClientMode::Http {
            base_url: cli.base_url.clone(),
            token,
            token_file: token_file_path.clone(),
            session_dir: cli
                .session_dir
                .clone()
                .or_else(|| session_path().exists().then_some(session_path())),
            auto_login: !cli.no_auto_login,
        },
    };
    let client = CopilotClient::new(mode);

    match &cli.command {
        Command::Auth { cmd } => auth::run_auth(&cli, &client, cmd.clone()),
        Command::Transactions { cmd } => run_transactions(&cli, &client, cmd.clone()),
        Command::Categories { cmd } => categories::run_categories(&cli, &client, cmd.clone()),
        Command::Recurrings { cmd } => recurrings::run_recurrings(&cli, &client, cmd.clone()),
        Command::Tags { cmd } => tags::run_tags(&cli, &client, cmd.clone()),
        Command::Budgets { cmd } => budgets::run_budgets(&cli, &client, cmd.clone()),
        Command::Accounts { cmd } => accounts::run_accounts(&cli, &client, cmd.clone()),
        Command::Networth { cmd } => networth::run_networth(&cli, &client, cmd.clone()),
        Command::Spending { cmd } => spending::run_spending(&cli, &client, cmd.clone()),
        Command::Mcp | Command::Version | Command::Completions { .. } => unreachable!(),
    }
}

impl TableRow for KeyValueRow {
    const HEADERS: &'static [&'static str] = &["key", "value"];

    fn cells(&self) -> Vec<Cell> {
        vec![Cell::new(&self.key), Cell::new(&self.value)]
    }
}

fn value_to_string(v: Option<serde_json::Value>) -> String {
    match v {
        None => String::new(),
        Some(serde_json::Value::String(s)) => s,
        Some(serde_json::Value::Number(n)) => n.to_string(),
        Some(serde_json::Value::Bool(b)) => b.to_string(),
        Some(serde_json::Value::Null) => String::new(),
        Some(other) => other.to_string(),
    }
}

fn value_to_money_string(v: Option<serde_json::Value>) -> String {
    let s = value_to_string(v);
    if s.trim().is_empty() {
        return String::new();
    }

    // Common cases from Copilot: "-57.48" or 185.4 (already stringified).
    let trimmed = s.trim();
    let negative = trimmed.starts_with('-');
    let numeric = trimmed.trim_start_matches('-');

    if let Ok(n) = numeric.parse::<f64>() {
        let formatted = format!("{:.2}", n.abs());
        if negative {
            format!("-${formatted}")
        } else {
            format!("${formatted}")
        }
    } else {
        // Fallback: keep original, but prefix `$` if it looks like a number.
        if negative {
            format!("-${numeric}")
        } else {
            format!("${trimmed}")
        }
    }
}

fn normalize_date(s: &str) -> Option<String> {
    let s = s.trim();
    if s.len() != 10 {
        return None;
    }

    let parts = s.split('-').collect::<Vec<_>>();
    if parts.len() != 3 {
        return None;
    }

    let (year, month, day) = if parts[0].len() == 4 {
        (parts[0], parts[1], parts[2])
    } else if parts[2].len() == 4 {
        (parts[2], parts[0], parts[1])
    } else {
        return None;
    };

    let y = year.parse::<u32>().ok()?;
    let m = month.parse::<u32>().ok()?;
    let d = day.parse::<u32>().ok()?;
    if !(1900..=2100).contains(&y) || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some(format!("{y:04}-{m:02}-{d:02}"))
}

// ---- Relative date support ----

/// Convert a unix day count to (year, month, day) using the Hinnant algorithm.
fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m, d)
}

fn today_days() -> i64 {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    (secs / 86400) as i64
}

fn format_civil(y: i32, m: u32, d: u32) -> String {
    format!("{y:04}-{m:02}-{d:02}")
}

fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y as i64 - 1 } else { y as i64 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u32;
    let m = m as u32;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe as i64 - 719468
}

/// Resolve a date string which can be:
/// - An absolute date (YYYY-MM-DD or MM-DD-YYYY)
/// - A relative keyword: today, yesterday, this-week, last-week, this-month, last-month
/// - A relative offset: Nd (e.g. 7d, 30d, 90d)
fn resolve_date(s: &str) -> Option<String> {
    let s = s.trim().to_lowercase();

    // Try absolute date first
    if let Some(d) = normalize_date(&s) {
        return Some(d);
    }

    let today = today_days();
    let (ty, tm, td) = civil_from_days(today);

    match s.as_str() {
        "today" => Some(format_civil(ty, tm, td)),
        "yesterday" => {
            let (y, m, d) = civil_from_days(today - 1);
            Some(format_civil(y, m, d))
        }
        "this-week" => {
            // Monday of current week (ISO: Monday=1..Sunday=7)
            let dow = ((today % 7) + 4) % 7; // 0=Monday .. 6=Sunday
            let monday = today - dow;
            let (y, m, d) = civil_from_days(monday);
            Some(format_civil(y, m, d))
        }
        "last-week" => {
            let dow = ((today % 7) + 4) % 7;
            let last_monday = today - dow - 7;
            let (y, m, d) = civil_from_days(last_monday);
            Some(format_civil(y, m, d))
        }
        "this-month" => Some(format_civil(ty, tm, 1)),
        "last-month" => {
            let (y, m) = if tm == 1 { (ty - 1, 12) } else { (ty, tm - 1) };
            Some(format_civil(y, m, 1))
        }
        other => {
            // Try Nd pattern (e.g. "7d", "30d")
            if let Some(n_str) = other.strip_suffix('d') {
                if let Ok(n) = n_str.parse::<i64>() {
                    let (y, m, d) = civil_from_days(today - n);
                    return Some(format_civil(y, m, d));
                }
            }
            None
        }
    }
}

fn last_day_of_month(y: i32, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0) {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}

/// For --to with relative keywords, resolve to end-of-period.
fn resolve_date_end(s: &str) -> Option<String> {
    let s = s.trim().to_lowercase();

    // Try absolute date first
    if let Some(d) = normalize_date(&s) {
        return Some(d);
    }

    let today = today_days();
    #[allow(unused_variables)]
    let (ty, tm, td) = civil_from_days(today);

    match s.as_str() {
        "today" | "yesterday" => resolve_date(&s),
        "this-week" => {
            let dow = ((today % 7) + 4) % 7;
            let sunday = today + (6 - dow);
            let (y, m, d) = civil_from_days(sunday);
            Some(format_civil(y, m, d))
        }
        "last-week" => {
            let dow = ((today % 7) + 4) % 7;
            let last_sunday = today - dow - 1;
            let (y, m, d) = civil_from_days(last_sunday);
            Some(format_civil(y, m, d))
        }
        "this-month" => Some(format_civil(ty, tm, last_day_of_month(ty, tm))),
        "last-month" => {
            let (y, m) = if tm == 1 { (ty - 1, 12) } else { (ty, tm - 1) };
            Some(format_civil(y, m, last_day_of_month(y, m)))
        }
        _ => resolve_date(&s),
    }
}

// ---- Preset loading ----

fn presets_path() -> PathBuf {
    let home = std::env::var_os("HOME").unwrap_or_default();
    let mut p = PathBuf::from(home);
    p.push(".config");
    p.push("copilot-money-cli");
    p.push("presets.json");
    p
}

fn load_preset(name: &str) -> anyhow::Result<serde_json::Value> {
    let path = presets_path();
    if !path.exists() {
        anyhow::bail!(
            "presets file not found at {}. Create it with named filter configurations.",
            path.display()
        );
    }
    let s = std::fs::read_to_string(&path)?;
    let presets: serde_json::Value = serde_json::from_str(&s)?;
    presets
        .get(name)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("preset {:?} not found in {}", name, path.display()))
}

fn apply_preset(args: &mut TransactionsListArgs) -> anyhow::Result<()> {
    let Some(ref name) = args.preset else {
        return Ok(());
    };
    let preset = load_preset(name)?;
    if let Some(v) = preset.get("category").and_then(|v| v.as_str()) {
        if args.category.is_none() {
            args.category = Some(v.to_string());
        }
    }
    if let Some(v) = preset.get("from").and_then(|v| v.as_str()) {
        if args.from.is_none() {
            args.from = Some(v.to_string());
        }
    }
    if let Some(v) = preset.get("to").and_then(|v| v.as_str()) {
        if args.to.is_none() {
            args.to = Some(v.to_string());
        }
    }
    if let Some(v) = preset.get("min_amount").and_then(|v| v.as_f64()) {
        if args.min_amount.is_none() {
            args.min_amount = Some(v);
        }
    }
    if let Some(v) = preset.get("max_amount").and_then(|v| v.as_f64()) {
        if args.max_amount.is_none() {
            args.max_amount = Some(v);
        }
    }
    if let Some(v) = preset.get("reviewed").and_then(|v| v.as_bool()) {
        if !args.reviewed && !args.unreviewed {
            if v {
                args.reviewed = true;
            } else {
                args.unreviewed = true;
            }
        }
    }
    if let Some(v) = preset.get("name_contains").and_then(|v| v.as_str()) {
        if args.name_contains.is_none() {
            args.name_contains = Some(v.to_string());
        }
    }
    if let Some(v) = preset.get("account").and_then(|v| v.as_str()) {
        if args.account.is_none() {
            args.account = Some(v.to_string());
        }
    }
    if let Some(v) = preset.get("limit").and_then(|v| v.as_u64()) {
        args.limit = v as usize;
    }
    if let Some(true) = preset.get("totals").and_then(|v| v.as_bool()) {
        args.totals = true;
    }
    Ok(())
}

// ---- Interactive selection helpers ----

pub(super) fn interactive_select_category(client: &CopilotClient) -> anyhow::Result<CategoryId> {
    let categories = client.list_categories(false, false, false)?;
    let flat = flatten_categories_for_lookup(&categories);
    if flat.is_empty() {
        anyhow::bail!("no categories found");
    }
    let names: Vec<String> = flat.iter().map(|(_, name)| name.clone()).collect();
    let selection = dialoguer::FuzzySelect::new()
        .with_prompt("Select category")
        .items(&names)
        .default(0)
        .interact()?;
    Ok(flat[selection].0.clone())
}

#[allow(dead_code)]
pub(super) fn interactive_select_tag(client: &CopilotClient) -> anyhow::Result<crate::types::TagId> {
    let tags = client.list_tags()?;
    if tags.is_empty() {
        anyhow::bail!("no tags found");
    }
    let names: Vec<String> = tags
        .iter()
        .map(|t| t.name.clone().unwrap_or_default())
        .collect();
    let selection = dialoguer::FuzzySelect::new()
        .with_prompt("Select tag")
        .items(&names)
        .default(0)
        .interact()?;
    Ok(tags[selection].id.clone())
}

#[allow(dead_code)]
pub(super) fn interactive_select_account(client: &CopilotClient) -> anyhow::Result<AccountId> {
    let accts = client.list_accounts()?;
    if accts.is_empty() {
        anyhow::bail!("no accounts found");
    }
    let names: Vec<String> = accts
        .iter()
        .map(|a| {
            format!(
                "{} ({})",
                a.name.as_deref().unwrap_or("?"),
                a.account_type.as_deref().unwrap_or("?")
            )
        })
        .collect();
    let selection = dialoguer::FuzzySelect::new()
        .with_prompt("Select account")
        .items(&names)
        .default(0)
        .interact()?;
    Ok(accts[selection].id.clone())
}

// ---- Amount parsing helper ----

fn parse_amount_f64(v: &Option<serde_json::Value>) -> Option<f64> {
    let s = value_to_string(v.clone());
    if s.trim().is_empty() {
        return None;
    }
    s.trim().parse::<f64>().ok()
}

fn build_transactions_filter(reviewed: bool, unreviewed: bool) -> Option<serde_json::Value> {
    if reviewed {
        Some(serde_json::json!({ "isReviewed": true }))
    } else if unreviewed {
        Some(serde_json::json!({ "isReviewed": false }))
    } else {
        None
    }
}

fn flatten_categories_for_lookup(categories: &[Category]) -> Vec<(CategoryId, String)> {
    fn walk(out: &mut Vec<(CategoryId, String)>, cats: &[Category]) {
        for c in cats {
            out.push((c.id.clone(), c.name.clone().unwrap_or_default()));
            if let Some(children) = c.child_categories.as_ref() {
                walk(out, children);
            }
        }
    }

    let mut out = Vec::new();
    walk(&mut out, categories);
    out
}

fn category_name_map(client: &CopilotClient) -> anyhow::Result<HashMap<CategoryId, String>> {
    let categories = client.list_categories(false, false, false)?;
    let mut out = HashMap::new();
    for (id, name) in flatten_categories_for_lookup(&categories) {
        out.insert(id, name);
    }
    Ok(out)
}

fn resolve_category_id(
    client: &CopilotClient,
    category_id: Option<&CategoryId>,
    category_name: Option<&str>,
) -> anyhow::Result<Option<CategoryId>> {
    if let Some(id) = category_id {
        return Ok(Some(id.clone()));
    }
    let Some(name) = category_name else {
        return Ok(None);
    };

    let want = name.trim().to_lowercase();
    if want.is_empty() {
        anyhow::bail!("empty --category");
    }

    let categories = client.list_categories(false, false, false)?;
    let matches = flatten_categories_for_lookup(&categories)
        .into_iter()
        .filter(|(_, n)| n.to_lowercase() == want)
        .collect::<Vec<_>>();

    match matches.as_slice() {
        [] => anyhow::bail!("no category named {:?}", name),
        [(id, _)] => Ok(Some(id.clone())),
        many => anyhow::bail!(
            "category name {:?} is ambiguous ({} matches); use --category-id instead",
            name,
            many.len()
        ),
    }
}

fn should_color(cli: &Cli) -> bool {
    match cli.color {
        ColorMode::Always => true,
        ColorMode::Never => false,
        ColorMode::Auto => std::io::stdout().is_terminal(),
    }
}

fn confirm_write(cli: &Cli, action: &str) -> anyhow::Result<()> {
    if cli.dry_run {
        return Ok(());
    }
    if cli.yes {
        return Ok(());
    }
    if !std::io::stdin().is_terminal() {
        anyhow::bail!("refusing to write in non-interactive mode without --yes");
    }

    eprintln!("{action}");
    let input = rpassword::prompt_password("Proceed? Type 'yes' to confirm: ")?;
    if input.trim() != "yes" {
        anyhow::bail!("aborted");
    }
    Ok(())
}

fn run_transactions(cli: &Cli, client: &CopilotClient, cmd: TransactionsCmd) -> anyhow::Result<()> {
    match cmd {
        TransactionsCmd::List(mut args) => {
            apply_preset(&mut args)?;
            let category_id =
                resolve_category_id(client, args.category_id.as_ref(), args.category.as_deref())?;
            let account_id =
                accounts::resolve_account_id(client, args.account_id.as_ref(), args.account.as_deref())?;
            let filter = build_transactions_filter(args.reviewed, args.unreviewed);
            let sort = sort_to_graphql(args.sort);
            let (items, page_info) = fetch_transactions_with_filter_sort(
                client,
                args.limit,
                args.after.clone(),
                args.pages,
                args.all,
                filter,
                sort,
            )?;
            let cat_names = category_name_map(client).unwrap_or_default();
            let filters = TransactionFilters {
                reviewed: args.reviewed,
                unreviewed: args.unreviewed,
                category_id: category_id.as_ref(),
                tags: &args.tag,
                query: args.name_contains.as_deref(),
                date: args.date.as_deref(),
                from: args.from.as_deref(),
                to: args.to.as_deref(),
                min_amount: args.min_amount,
                max_amount: args.max_amount,
                account_id: account_id.as_ref(),
                category_names: Some(&cat_names),
            };
            let filtered = filter_transactions(items, &filters);
            render_transactions_output(
                cli,
                client,
                filtered,
                page_info,
                args.page_info,
                &args.fields,
                args.totals,
            )
        }
        TransactionsCmd::Search(args) => {
            let category_id =
                resolve_category_id(client, args.category_id.as_ref(), args.category.as_deref())?;
            let account_id =
                accounts::resolve_account_id(client, args.account_id.as_ref(), args.account.as_deref())?;
            let filter = build_transactions_filter(args.reviewed, args.unreviewed);
            let sort = sort_to_graphql(args.sort);
            let (items, page_info) = fetch_transactions_with_filter_sort(
                client,
                args.limit,
                args.after.clone(),
                args.pages,
                args.all,
                filter,
                sort,
            )?;
            let cat_names = category_name_map(client).unwrap_or_default();
            let filters = TransactionFilters {
                reviewed: args.reviewed,
                unreviewed: args.unreviewed,
                category_id: category_id.as_ref(),
                tags: &args.tag,
                query: Some(&args.query),
                date: args.date.as_deref(),
                from: args.from.as_deref(),
                to: args.to.as_deref(),
                min_amount: args.min_amount,
                max_amount: args.max_amount,
                account_id: account_id.as_ref(),
                category_names: Some(&cat_names),
            };
            let filtered = filter_transactions(items, &filters);
            render_transactions_output(
                cli,
                client,
                filtered,
                page_info,
                args.page_info,
                &args.fields,
                args.totals,
            )
        }
        TransactionsCmd::Show(args) => {
            let items = client.list_transactions(args.limit)?;
            let found = items.into_iter().find(|t| t.id == args.id);
            match found {
                Some(t) => render_output(
                    cli,
                    vec![
                        KeyValueRow {
                            key: "id".to_string(),
                            value: t.id.to_string(),
                        },
                        KeyValueRow {
                            key: "date".to_string(),
                            value: t.date.unwrap_or_default(),
                        },
                        KeyValueRow {
                            key: "name".to_string(),
                            value: t.name.unwrap_or_default(),
                        },
                        KeyValueRow {
                            key: "amount".to_string(),
                            value: value_to_money_string(t.amount),
                        },
                        KeyValueRow {
                            key: "category_id".to_string(),
                            value: t
                                .category_id
                                .as_ref()
                                .map(|c| c.to_string())
                                .unwrap_or_default(),
                        },
                        KeyValueRow {
                            key: "reviewed".to_string(),
                            value: t.is_reviewed.unwrap_or(false).to_string(),
                        },
                    ],
                ),
                None => anyhow::bail!("transaction not found"),
            }
        }
        TransactionsCmd::Review(args) => {
            if cli.dry_run {
                println!("dry-run: would mark reviewed: {:?}", args.ids);
                return Ok(());
            }
            confirm_write(cli, &format!("Mark reviewed: {:?}", args.ids))?;
            let txns = resolve_transactions_by_ids(client, &args.ids)?;
            let refs = build_transaction_id_refs(&txns)?;
            let result = client.bulk_edit_transactions_reviewed(refs, true)?;
            render_bulk_edit_result(cli, result)
        }
        TransactionsCmd::Unreview(args) => {
            if cli.dry_run {
                println!("dry-run: would mark unreviewed: {:?}", args.ids);
                return Ok(());
            }
            confirm_write(cli, &format!("Mark unreviewed: {:?}", args.ids))?;
            let txns = resolve_transactions_by_ids(client, &args.ids)?;
            let refs = build_transaction_id_refs(&txns)?;
            let result = client.bulk_edit_transactions_reviewed(refs, false)?;
            render_bulk_edit_result(cli, result)
        }
        TransactionsCmd::SetCategory(args) => {
            if cli.dry_run {
                println!(
                    "dry-run: would set category {:?}/{:?} for {:?}",
                    args.category_id, args.category, args.ids
                );
                return Ok(());
            }
            let category_id = if args.category_id.is_none() && args.category.is_none() && std::io::stdin().is_terminal() {
                interactive_select_category(client)?
            } else {
                resolve_category_id(client, args.category_id.as_ref(), args.category.as_deref())?
                    .ok_or_else(|| anyhow::anyhow!("missing category target"))?
            };
            confirm_write(
                cli,
                &format!(
                    "Set category {:?}/{:?} for {:?}",
                    category_id, args.category, args.ids
                ),
            )?;
            let txns = resolve_transactions_by_ids(client, &args.ids)?;
            let mut updated = Vec::new();
            for txn in txns {
                let (item_id, account_id) = require_item_and_account(&txn)?;
                let t = client.edit_transaction(
                    &item_id,
                    &account_id,
                    &txn.id,
                    serde_json::json!({ "categoryId": category_id.clone() }),
                )?;
                updated.push(t);
            }
            render_transactions_updated(cli, updated)
        }
        TransactionsCmd::AssignRecurring(args) => {
            if cli.dry_run {
                println!(
                    "dry-run: would assign recurring {} for {:?}",
                    args.recurring_id, args.ids
                );
                return Ok(());
            }
            confirm_write(
                cli,
                &format!("Assign recurring {} for {:?}", args.recurring_id, args.ids),
            )?;
            let txns = resolve_transactions_by_ids(client, &args.ids)?;
            let mut updated = Vec::new();
            for txn in txns {
                let (item_id, account_id) = require_item_and_account(&txn)?;
                let t = client.add_transaction_to_recurring(
                    &item_id,
                    &account_id,
                    &txn.id,
                    &args.recurring_id,
                )?;
                updated.push(t);
            }
            render_transactions_updated(cli, updated)
        }
        TransactionsCmd::SetNotes(args) => {
            if cli.dry_run {
                println!(
                    "dry-run: would set notes for {:?} (clear={})",
                    args.ids, args.clear
                );
                return Ok(());
            }
            confirm_write(
                cli,
                &format!("Set notes for {:?} (clear={})", args.ids, args.clear),
            )?;
            if !args.clear && args.notes.is_none() {
                anyhow::bail!("use --notes <TEXT> or --clear");
            }
            let txns = resolve_transactions_by_ids(client, &args.ids)?;
            let mut updated = Vec::new();
            for txn in txns {
                let (item_id, account_id) = require_item_and_account(&txn)?;
                let input = if args.clear {
                    serde_json::json!({ "userNotes": "" })
                } else {
                    serde_json::json!({ "userNotes": args.notes.clone().unwrap_or_default() })
                };
                let t = client.edit_transaction(&item_id, &account_id, &txn.id, input)?;
                updated.push(t);
            }
            render_transactions_updated(cli, updated)
        }
        TransactionsCmd::SetTags(args) => {
            if cli.dry_run {
                println!(
                    "dry-run: would update tags mode={:?} tag_ids={:?} for {:?}",
                    args.mode, args.tag_ids, args.ids
                );
                return Ok(());
            }
            confirm_write(
                cli,
                &format!(
                    "Update tags mode={:?} tag_ids={:?} for {:?}",
                    args.mode, args.tag_ids, args.ids
                ),
            )?;
            if (args.mode == TagUpdateMode::Add || args.mode == TagUpdateMode::Remove)
                && args.tag_ids.is_empty()
            {
                anyhow::bail!("--tag-id is required for --mode add/remove");
            }

            let txns = resolve_transactions_by_ids(client, &args.ids)?;
            let mut updated = Vec::new();

            for txn in txns {
                let (item_id, account_id) = require_item_and_account(&txn)?;
                let existing = txn
                    .tags
                    .as_ref()
                    .map(|ts| ts.iter().map(|t| t.id.clone()).collect::<HashSet<_>>())
                    .unwrap_or_default();

                let next_ids: Vec<TagId> = match args.mode {
                    TagUpdateMode::Set => args.tag_ids.clone(),
                    TagUpdateMode::Add => {
                        let mut out = existing;
                        for id in &args.tag_ids {
                            out.insert(id.clone());
                        }
                        out.into_iter().collect()
                    }
                    TagUpdateMode::Remove => {
                        let mut out = existing;
                        for id in &args.tag_ids {
                            out.remove(id);
                        }
                        out.into_iter().collect()
                    }
                };

                let t = client.edit_transaction(
                    &item_id,
                    &account_id,
                    &txn.id,
                    serde_json::json!({
                        "tagIds": next_ids
                            .into_iter()
                            .map(|id| id.to_string())
                            .collect::<Vec<_>>()
                    }),
                )?;
                updated.push(t);
            }

            render_transactions_updated(cli, updated)
        }
        TransactionsCmd::Edit(args) => {
            if cli.dry_run {
                println!(
                    "dry-run: would edit transactions {:?} (type={:?}, input_json={})",
                    args.ids,
                    args.type_,
                    args.input_json.is_some()
                );
                return Ok(());
            }
            confirm_write(cli, &format!("Edit transactions {:?}", args.ids))?;

            let mut input = match args.input_json.as_ref() {
                None => serde_json::Value::Object(serde_json::Map::new()),
                Some(s) => serde_json::from_str::<serde_json::Value>(s)
                    .context("failed to parse --input-json")?,
            };

            if !input.is_object() {
                anyhow::bail!("--input-json must be a JSON object");
            }

            if let Some(t) = args.type_.as_ref() {
                input
                    .as_object_mut()
                    .expect("checked is_object above")
                    .insert("type".to_string(), serde_json::Value::String(t.to_string()));
            }

            let txns = resolve_transactions_by_ids(client, &args.ids)?;
            let mut updated = Vec::new();
            for txn in txns {
                let (item_id, account_id) = require_item_and_account(&txn)?;
                let t = client.edit_transaction(&item_id, &account_id, &txn.id, input.clone())?;
                updated.push(t);
            }
            render_transactions_updated(cli, updated)
        }
        TransactionsCmd::Duplicates(args) => {
            let txns = client.list_transactions(args.limit)?;
            let groups = find_duplicate_groups(&txns, args.days);
            if groups.is_empty() {
                println!("No potential duplicates found.");
                return Ok(());
            }
            let acct_names = accounts::account_name_map(client).unwrap_or_default();
            let cats = category_name_map(client).unwrap_or_default();
            for (i, group) in groups.iter().enumerate() {
                if i > 0 {
                    println!();
                }
                println!(
                    "--- Potential duplicate group ({} transactions, amount: {}) ---",
                    group.len(),
                    value_to_money_string(group[0].amount.clone())
                );
                let owned: Vec<Transaction> = group.iter().map(|t| (*t).clone()).collect();
                render_transactions_table(
                    cli,
                    &owned,
                    &args.fields,
                    Some(&cats),
                    Some(&acct_names),
                    false,
                )?;
            }
            Ok(())
        }
    }
}

fn find_duplicate_groups(txns: &[Transaction], day_threshold: u32) -> Vec<Vec<&Transaction>> {
    let mut groups: Vec<Vec<&Transaction>> = Vec::new();
    let mut used: HashSet<usize> = HashSet::new();

    for i in 0..txns.len() {
        if used.contains(&i) {
            continue;
        }
        let a = &txns[i];
        let a_amount = parse_amount_f64(&a.amount);
        let a_date = a.date.as_deref().unwrap_or("");

        let mut group = vec![a];
        for j in (i + 1)..txns.len() {
            if used.contains(&j) {
                continue;
            }
            let b = &txns[j];
            let b_amount = parse_amount_f64(&b.amount);
            let b_date = b.date.as_deref().unwrap_or("");

            // Same amount?
            let amount_match = match (a_amount, b_amount) {
                (Some(a), Some(b)) => (a - b).abs() < 0.01,
                _ => false,
            };
            if !amount_match {
                continue;
            }

            // Close dates?
            let date_close = dates_within_days(a_date, b_date, day_threshold);
            if !date_close {
                continue;
            }

            // Similar name? (Levenshtein-like: at least 50% overlap)
            let a_name = a.name.as_deref().unwrap_or("").to_lowercase();
            let b_name = b.name.as_deref().unwrap_or("").to_lowercase();
            let name_similar = if a_name.is_empty() && b_name.is_empty() {
                true
            } else if a_name.is_empty() || b_name.is_empty() {
                false
            } else {
                a_name.contains(&b_name) || b_name.contains(&a_name) || a_name == b_name
            };

            if name_similar {
                used.insert(j);
                group.push(b);
            }
        }

        if group.len() > 1 {
            used.insert(i);
            groups.push(group);
        }
    }
    groups
}

fn dates_within_days(a: &str, b: &str, threshold: u32) -> bool {
    let Some(ad) = normalize_date(a) else {
        return false;
    };
    let Some(bd) = normalize_date(b) else {
        return false;
    };
    let parse = |s: &str| -> Option<i64> {
        let parts: Vec<&str> = s.split('-').collect();
        if parts.len() != 3 {
            return None;
        }
        let y: i32 = parts[0].parse().ok()?;
        let m: u32 = parts[1].parse().ok()?;
        let d: u32 = parts[2].parse().ok()?;
        Some(days_from_civil(y, m, d))
    };
    match (parse(&ad), parse(&bd)) {
        (Some(a), Some(b)) => (a - b).unsigned_abs() as u32 <= threshold,
        _ => false,
    }
}

fn require_item_and_account(
    txn: &Transaction,
) -> anyhow::Result<(crate::types::ItemId, crate::types::AccountId)> {
    let item_id = txn
        .item_id
        .clone()
        .ok_or_else(|| anyhow::anyhow!("transaction {} missing itemId", txn.id))?;
    let account_id = txn
        .account_id
        .clone()
        .ok_or_else(|| anyhow::anyhow!("transaction {} missing accountId", txn.id))?;
    Ok((item_id, account_id))
}

fn build_transaction_id_refs(txns: &[Transaction]) -> anyhow::Result<Vec<TransactionIdRef>> {
    let mut out = Vec::new();
    for txn in txns {
        let (item_id, account_id) = require_item_and_account(txn)?;
        out.push(TransactionIdRef {
            account_id,
            id: txn.id.clone(),
            item_id,
        });
    }
    Ok(out)
}

fn resolve_transactions_by_ids(
    client: &CopilotClient,
    ids: &[TransactionId],
) -> anyhow::Result<Vec<Transaction>> {
    let want: HashSet<TransactionId> = ids.iter().cloned().collect();
    let mut found: HashMap<TransactionId, Transaction> = HashMap::new();

    let mut cursor: Option<String> = None;
    let mut scanned = 0usize;
    let max_pages = 200usize; // safety guard; use `transactions list --all` if you need more context.

    for _ in 0..max_pages {
        let page = client.list_transactions_page(200, cursor.clone(), None, None)?;
        let has_next = page.page_info.has_next_page.unwrap_or(false);
        cursor = page.page_info.end_cursor.clone();
        scanned += page.transactions.len();

        for t in page.transactions {
            if want.contains(&t.id) {
                found.insert(t.id.clone(), t);
            }
        }

        if found.len() == want.len() {
            break;
        }

        if has_next {
            continue;
        }
        break;
    }

    let mut missing = Vec::new();
    let mut ordered = Vec::new();
    for id in ids {
        match found.remove(id) {
            Some(t) => ordered.push(t),
            None => missing.push(id.to_string()),
        }
    }

    if !missing.is_empty() {
        anyhow::bail!(
            "could not resolve {} transaction ids after scanning {scanned} transactions: {:?}",
            missing.len(),
            missing
        );
    }

    Ok(ordered)
}

#[derive(Debug, Serialize)]
struct BulkEditJsonOutput {
    updated: Vec<Transaction>,
    failed: Vec<crate::client::BulkEditFailed>,
}

fn render_bulk_edit_result(cli: &Cli, result: BulkEditTransactionsResult) -> anyhow::Result<()> {
    if !result.failed.is_empty() {
        if cli.output == OutputFormat::Json {
            let out = BulkEditJsonOutput {
                updated: result.updated,
                failed: result.failed,
            };
            let s = serde_json::to_string_pretty(&out)?;
            println!("{s}");
            return Ok(());
        }
        anyhow::bail!(
            "bulk edit failed for {} transaction(s)",
            result.failed.len()
        );
    }
    render_transactions_updated(cli, result.updated)
}

fn render_transactions_updated(cli: &Cli, items: Vec<Transaction>) -> anyhow::Result<()> {
    const DEFAULT_FIELDS: &[TransactionField] = &[
        TransactionField::Date,
        TransactionField::Name,
        TransactionField::Amount,
        TransactionField::Reviewed,
        TransactionField::Category,
        TransactionField::Tags,
        TransactionField::Type,
    ];

    match cli.output {
        OutputFormat::Json => {
            let out = TransactionsJsonOutput {
                transactions: items,
                page_info: None,
            };
            let s = serde_json::to_string_pretty(&out)?;
            println!("{s}");
            Ok(())
        }
        OutputFormat::Csv | OutputFormat::Table => {
            render_transactions_table(cli, &items, DEFAULT_FIELDS, None, None, false)
        }
    }
}

#[derive(Debug, Serialize)]
struct TransactionsJsonOutput {
    transactions: Vec<Transaction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    page_info: Option<PageInfo>,
}

fn fetch_transactions_with_filter_sort(
    client: &CopilotClient,
    page_size: usize,
    after: Option<String>,
    pages: usize,
    all: bool,
    filter: Option<serde_json::Value>,
    sort: Option<serde_json::Value>,
) -> anyhow::Result<(Vec<Transaction>, PageInfo)> {
    let mut out = Vec::new();
    let mut cursor = after;
    let max_pages = if all { usize::MAX } else { pages.max(1) };

    let mut last_page_info: Option<PageInfo> = None;

    for _ in 0..max_pages {
        let page = client.list_transactions_page(
            page_size,
            cursor.clone(),
            filter.clone(),
            sort.clone(),
        )?;
        cursor = page.page_info.end_cursor.clone();
        last_page_info = Some(page.page_info);
        out.extend(page.transactions);

        let has_next = last_page_info
            .as_ref()
            .and_then(|p| p.has_next_page)
            .unwrap_or(false);
        if !has_next || cursor.is_none() {
            break;
        }
    }

    Ok((
        out,
        last_page_info.unwrap_or(PageInfo {
            end_cursor: None,
            has_next_page: None,
            has_previous_page: None,
            start_cursor: None,
        }),
    ))
}

struct TransactionFilters<'a> {
    reviewed: bool,
    unreviewed: bool,
    category_id: Option<&'a CategoryId>,
    tags: &'a [String],
    query: Option<&'a str>,
    date: Option<&'a str>,
    from: Option<&'a str>,
    to: Option<&'a str>,
    min_amount: Option<f64>,
    max_amount: Option<f64>,
    account_id: Option<&'a AccountId>,
    category_names: Option<&'a HashMap<CategoryId, String>>,
}

fn filter_transactions(items: Vec<Transaction>, f: &TransactionFilters<'_>) -> Vec<Transaction> {
    let q = f.query.map(|s| s.to_lowercase());
    let want_tags = f.tags.iter().map(|t| t.to_lowercase()).collect::<Vec<_>>();
    let from_date = f.from.and_then(resolve_date);
    let to_date = f.to.and_then(resolve_date_end);

    items
        .into_iter()
        .filter(|t| {
            if f.reviewed && !t.is_reviewed.unwrap_or(false) {
                return false;
            }
            if f.unreviewed && t.is_reviewed.unwrap_or(false) {
                return false;
            }
            if let Some(cat) = f.category_id
                && t.category_id.as_ref() != Some(cat)
            {
                return false;
            }
            if let Some(acct) = f.account_id
                && t.account_id.as_ref() != Some(acct)
            {
                return false;
            }
            // Enhanced search: match against name, notes, category name, and tags
            if let Some(q) = &q {
                let name = t.name.as_deref().unwrap_or("").to_lowercase();
                let notes = t.user_notes.as_deref().unwrap_or("").to_lowercase();
                let cat_name = t
                    .category_id
                    .as_ref()
                    .and_then(|cid| f.category_names.and_then(|m| m.get(cid)))
                    .map(|n| n.to_lowercase())
                    .unwrap_or_default();
                let tag_str = t
                    .tags
                    .as_ref()
                    .map(|ts| {
                        ts.iter()
                            .filter_map(|tag| tag.name.as_ref())
                            .map(|s| s.to_lowercase())
                            .collect::<Vec<_>>()
                            .join(" ")
                    })
                    .unwrap_or_default();
                if !name.contains(q)
                    && !notes.contains(q)
                    && !cat_name.contains(q)
                    && !tag_str.contains(q)
                {
                    return false;
                }
            }
            // Exact date match
            if let Some(d) = f.date {
                let want = normalize_date(d).unwrap_or_else(|| d.to_string());
                if t.date.as_deref().unwrap_or("") != want {
                    return false;
                }
            }
            // Date range
            if let Some(ref from) = from_date {
                let txn_date = t.date.as_deref().unwrap_or("");
                if txn_date < from.as_str() {
                    return false;
                }
            }
            if let Some(ref to) = to_date {
                let txn_date = t.date.as_deref().unwrap_or("");
                if txn_date > to.as_str() {
                    return false;
                }
            }
            // Amount range
            if f.min_amount.is_some() || f.max_amount.is_some() {
                if let Some(amt) = parse_amount_f64(&t.amount) {
                    let abs_amt = amt.abs();
                    if let Some(min) = f.min_amount {
                        if abs_amt < min {
                            return false;
                        }
                    }
                    if let Some(max) = f.max_amount {
                        if abs_amt > max {
                            return false;
                        }
                    }
                }
            }
            // Tag filter
            if want_tags.is_empty() {
                return true;
            }
            let txn_tags = t
                .tags
                .as_ref()
                .map(|ts| {
                    ts.iter()
                        .filter_map(|tag| tag.name.as_ref())
                        .map(|s| s.to_lowercase())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            txn_tags.iter().any(|t| want_tags.iter().any(|w| w == t))
        })
        .collect()
}

fn render_transactions_table(
    cli: &Cli,
    items: &[Transaction],
    fields: &[TransactionField],
    categories: Option<&HashMap<CategoryId, String>>,
    acct_names: Option<&HashMap<AccountId, String>>,
    show_totals: bool,
) -> anyhow::Result<()> {
    use comfy_table::CellAlignment;

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::DynamicFullWidth);

    if let Some(w) = terminal_width() {
        table.set_width(w);
    }

    let header = fields
        .iter()
        .map(|f| match f {
            TransactionField::Date => header_cell(cli, "date"),
            TransactionField::Name => header_cell(cli, "name"),
            TransactionField::Amount => header_cell(cli, "amount"),
            TransactionField::Reviewed => header_cell(cli, "reviewed"),
            TransactionField::Category => header_cell(cli, "category"),
            TransactionField::Tags => header_cell(cli, "tags"),
            TransactionField::Type => header_cell(cli, "type"),
            TransactionField::Account => header_cell(cli, "account"),
            TransactionField::Notes => header_cell(cli, "notes"),
            TransactionField::Id => header_cell(cli, "id"),
        })
        .collect::<Vec<_>>();
    table.set_header(ComfyRow::from(header));

    let use_color = should_color(cli);

    let mut total_amount: f64 = 0.0;
    let mut count = 0usize;

    for t in items {
        let mut cells = Vec::new();
        for f in fields {
            match f {
                TransactionField::Date => cells.push(Cell::new(t.date.as_deref().unwrap_or(""))),
                TransactionField::Name => cells.push(Cell::new(t.name.as_deref().unwrap_or(""))),
                TransactionField::Amount => {
                    let s = value_to_money_string(t.amount.clone());
                    if let Some(amt) = parse_amount_f64(&t.amount) {
                        total_amount += amt;
                    }
                    let mut cell = Cell::new(&s).set_alignment(CellAlignment::Right);
                    if use_color && !s.is_empty() {
                        if s.starts_with("-$") {
                            cell = cell.fg(Color::Red);
                        } else {
                            cell = cell.fg(Color::Green);
                        }
                    }
                    cells.push(cell);
                }
                TransactionField::Reviewed => {
                    let reviewed = t.is_reviewed.unwrap_or(false);
                    let mut cell = Cell::new(if reviewed { "✓" } else { "" });
                    if use_color && reviewed {
                        cell = cell.fg(Color::Green);
                    }
                    cells.push(cell);
                }
                TransactionField::Category => {
                    let name = t
                        .category_id
                        .as_ref()
                        .and_then(|id| categories.and_then(|m| m.get(id)))
                        .map(|s| s.as_str())
                        .or_else(|| t.category_id.as_ref().map(|id| id.as_str()))
                        .unwrap_or("");
                    cells.push(Cell::new(name));
                }
                TransactionField::Tags => {
                    let tags = t
                        .tags
                        .as_ref()
                        .map(|ts| {
                            ts.iter()
                                .filter_map(|tag| tag.name.as_deref())
                                .collect::<Vec<_>>()
                                .join(",")
                        })
                        .unwrap_or_default();
                    cells.push(Cell::new(tags));
                }
                TransactionField::Type => cells.push(Cell::new(
                    t.txn_type
                        .as_ref()
                        .map(|t| t.to_string())
                        .unwrap_or_default(),
                )),
                TransactionField::Account => {
                    let name = t
                        .account_id
                        .as_ref()
                        .and_then(|id| acct_names.and_then(|m| m.get(id)))
                        .map(|s| s.as_str())
                        .or_else(|| t.account_id.as_ref().map(|id| id.as_str()))
                        .unwrap_or("");
                    cells.push(Cell::new(name));
                }
                TransactionField::Notes => {
                    cells.push(Cell::new(t.user_notes.as_deref().unwrap_or("")));
                }
                TransactionField::Id => cells.push(Cell::new(shorten_id_for_table(t.id.as_str()))),
            }
        }
        count += 1;
        table.add_row(ComfyRow::from(cells));
    }

    println!("{table}");

    if show_totals && count > 0 {
        let total_str = if total_amount < 0.0 {
            format!("-${:.2}", total_amount.abs())
        } else {
            format!("${:.2}", total_amount)
        };
        println!();
        println!("{count} transactions, net {total_str}");
    }

    Ok(())
}

fn render_transactions_output(
    cli: &Cli,
    client: &CopilotClient,
    items: Vec<Transaction>,
    page_info: PageInfo,
    include_page_info: bool,
    fields: &[TransactionField],
    show_totals: bool,
) -> anyhow::Result<()> {
    match cli.output {
        OutputFormat::Json => {
            let out = TransactionsJsonOutput {
                transactions: items,
                page_info: include_page_info.then_some(page_info),
            };
            let s = serde_json::to_string_pretty(&out)?;
            println!("{s}");
            Ok(())
        }
        OutputFormat::Csv => {
            // CSV header
            let headers: Vec<&str> = fields
                .iter()
                .map(|f| match f {
                    TransactionField::Date => "date",
                    TransactionField::Name => "name",
                    TransactionField::Amount => "amount",
                    TransactionField::Reviewed => "reviewed",
                    TransactionField::Category => "category",
                    TransactionField::Tags => "tags",
                    TransactionField::Type => "type",
                    TransactionField::Account => "account",
                    TransactionField::Notes => "notes",
                    TransactionField::Id => "id",
                })
                .collect();
            println!("{}", headers.join(","));

            let cats = category_name_map(client).unwrap_or_default();
            let accts = accounts::account_name_map(client).unwrap_or_default();

            for t in &items {
                let vals: Vec<String> = fields
                    .iter()
                    .map(|f| match f {
                        TransactionField::Date => t.date.clone().unwrap_or_default(),
                        TransactionField::Name => csv_escape_field(t.name.as_deref().unwrap_or("")),
                        TransactionField::Amount => value_to_money_string(t.amount.clone()),
                        TransactionField::Reviewed => {
                            t.is_reviewed.unwrap_or(false).to_string()
                        }
                        TransactionField::Category => {
                            let name = t
                                .category_id
                                .as_ref()
                                .and_then(|id| cats.get(id))
                                .map(|s| s.as_str())
                                .unwrap_or("");
                            csv_escape_field(name)
                        }
                        TransactionField::Tags => {
                            let s = t
                                .tags
                                .as_ref()
                                .map(|ts| {
                                    ts.iter()
                                        .filter_map(|tag| tag.name.as_deref())
                                        .collect::<Vec<_>>()
                                        .join(";")
                                })
                                .unwrap_or_default();
                            csv_escape_field(&s)
                        }
                        TransactionField::Type => {
                            t.txn_type.map(|t| t.to_string()).unwrap_or_default()
                        }
                        TransactionField::Account => {
                            let name = t
                                .account_id
                                .as_ref()
                                .and_then(|id| accts.get(id))
                                .map(|s| s.as_str())
                                .unwrap_or("");
                            csv_escape_field(name)
                        }
                        TransactionField::Notes => {
                            csv_escape_field(t.user_notes.as_deref().unwrap_or(""))
                        }
                        TransactionField::Id => t.id.to_string(),
                    })
                    .collect();
                println!("{}", vals.join(","));
            }
            Ok(())
        }
        OutputFormat::Table => {
            let cats = if fields.contains(&TransactionField::Category) {
                Some(category_name_map(client)?)
            } else {
                None
            };
            let accts = if fields.contains(&TransactionField::Account) {
                Some(accounts::account_name_map(client)?)
            } else {
                None
            };
            render_transactions_table(cli, &items, fields, cats.as_ref(), accts.as_ref(), show_totals)?;
            if include_page_info {
                render_output(
                    cli,
                    vec![
                        KeyValueRow {
                            key: "endCursor".to_string(),
                            value: page_info.end_cursor.unwrap_or_default(),
                        },
                        KeyValueRow {
                            key: "hasNextPage".to_string(),
                            value: page_info
                                .has_next_page
                                .map(|b| b.to_string())
                                .unwrap_or_default(),
                        },
                    ],
                )?;
            }
            Ok(())
        }
    }
}

fn csv_escape_field(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod helper_tests {
    use super::*;

    #[test]
    fn normalize_date_accepts_yyyy_mm_dd_and_mm_dd_yyyy() {
        assert_eq!(normalize_date("2025-12-03"), Some("2025-12-03".to_string()));
        assert_eq!(normalize_date("12-03-2025"), Some("2025-12-03".to_string()));
        assert_eq!(normalize_date("03-12-2025"), Some("2025-03-12".to_string()));
    }

    #[test]
    fn normalize_date_rejects_invalid() {
        assert_eq!(normalize_date(""), None);
        assert_eq!(normalize_date("2025-13-01"), None);
        assert_eq!(normalize_date("2025-00-01"), None);
        assert_eq!(normalize_date("2025-12-32"), None);
        assert_eq!(normalize_date("2025/12/01"), None);
    }

    #[test]
    fn money_string_formats_numbers() {
        assert_eq!(
            value_to_money_string(Some(serde_json::json!("-57.48"))),
            "-$57.48"
        );
        assert_eq!(
            value_to_money_string(Some(serde_json::json!(185.4))),
            "$185.40"
        );
        assert_eq!(value_to_money_string(Some(serde_json::json!("0"))), "$0.00");
        assert_eq!(value_to_money_string(None), "");
    }

    #[test]
    fn sort_to_graphql_maps_values() {
        assert_eq!(
            sort_to_graphql(Some(TransactionsSort::DateDesc)).unwrap(),
            serde_json::json!([{ "field": "DATE", "direction": "DESC" }])
        );
        assert_eq!(
            sort_to_graphql(Some(TransactionsSort::AmountAsc)).unwrap(),
            serde_json::json!([{ "field": "AMOUNT", "direction": "ASC" }])
        );
        assert!(sort_to_graphql(None).is_none());
    }

    #[test]
    fn build_transactions_filter_works() {
        assert_eq!(
            build_transactions_filter(true, false),
            Some(serde_json::json!({"isReviewed": true}))
        );
        assert_eq!(
            build_transactions_filter(false, true),
            Some(serde_json::json!({"isReviewed": false}))
        );
        assert_eq!(build_transactions_filter(false, false), None);
    }

    // ---- Relative date tests ----

    #[test]
    fn resolve_date_absolute_dates() {
        assert_eq!(resolve_date("2025-12-03"), Some("2025-12-03".to_string()));
        assert_eq!(resolve_date("12-03-2025"), Some("2025-12-03".to_string()));
    }

    #[test]
    fn resolve_date_today_returns_current_date() {
        let result = resolve_date("today");
        assert!(result.is_some());
        let d = result.unwrap();
        assert_eq!(d.len(), 10);
        assert!(d.starts_with("20")); // should be a 20xx year
    }

    #[test]
    fn resolve_date_yesterday_returns_a_date() {
        let result = resolve_date("yesterday");
        assert!(result.is_some());
        let d = result.unwrap();
        assert_eq!(d.len(), 10);
    }

    #[test]
    fn resolve_date_relative_days() {
        let result = resolve_date("7d");
        assert!(result.is_some());
        let d = result.unwrap();
        assert_eq!(d.len(), 10);

        let result30 = resolve_date("30d");
        assert!(result30.is_some());
    }

    #[test]
    fn resolve_date_this_month() {
        let result = resolve_date("this-month");
        assert!(result.is_some());
        let d = result.unwrap();
        // Should end with -01
        assert!(d.ends_with("-01"));
    }

    #[test]
    fn resolve_date_last_month() {
        let result = resolve_date("last-month");
        assert!(result.is_some());
        let d = result.unwrap();
        assert!(d.ends_with("-01"));
    }

    #[test]
    fn resolve_date_this_week() {
        let result = resolve_date("this-week");
        assert!(result.is_some());
    }

    #[test]
    fn resolve_date_last_week() {
        let result = resolve_date("last-week");
        assert!(result.is_some());
    }

    #[test]
    fn resolve_date_unknown_returns_none() {
        assert_eq!(resolve_date("foobar"), None);
        assert_eq!(resolve_date(""), None);
    }

    #[test]
    fn resolve_date_end_this_month_gives_end_of_month() {
        let result = resolve_date_end("this-month");
        assert!(result.is_some());
        let d = result.unwrap();
        // Should not end with -01 (should be last day)
        let day: u32 = d[8..10].parse().unwrap();
        assert!(day >= 28);
    }

    #[test]
    fn resolve_date_end_absolute_passthrough() {
        assert_eq!(
            resolve_date_end("2025-06-15"),
            Some("2025-06-15".to_string())
        );
    }

    // ---- Civil date math tests ----

    #[test]
    fn civil_date_roundtrip() {
        // Roundtrip: convert to days and back
        let days = days_from_civil(2025, 1, 1);
        let (y, m, d) = civil_from_days(days);
        assert_eq!((y, m, d), (2025, 1, 1));

        let days2 = days_from_civil(2024, 2, 29);
        let (y2, m2, d2) = civil_from_days(days2);
        assert_eq!((y2, m2, d2), (2024, 2, 29));
    }

    #[test]
    fn civil_date_epoch() {
        // Unix epoch = 1970-01-01 = day 0
        let (y, m, d) = civil_from_days(0);
        assert_eq!((y, m, d), (1970, 1, 1));
    }

    #[test]
    fn last_day_of_month_values() {
        assert_eq!(last_day_of_month(2025, 1), 31);
        assert_eq!(last_day_of_month(2025, 2), 28);
        assert_eq!(last_day_of_month(2024, 2), 29); // leap year
        assert_eq!(last_day_of_month(2025, 4), 30);
        assert_eq!(last_day_of_month(2025, 12), 31);
    }

    // ---- Amount parsing ----

    #[test]
    fn parse_amount_f64_works() {
        assert_eq!(
            parse_amount_f64(&Some(serde_json::json!("-57.48"))),
            Some(-57.48)
        );
        assert_eq!(
            parse_amount_f64(&Some(serde_json::json!(185.4))),
            Some(185.4)
        );
        assert_eq!(parse_amount_f64(&None), None);
        assert_eq!(
            parse_amount_f64(&Some(serde_json::json!(""))),
            None
        );
    }

    // ---- Dates within days ----

    #[test]
    fn dates_within_days_same_date() {
        assert!(dates_within_days("2025-12-15", "2025-12-15", 3));
    }

    #[test]
    fn dates_within_days_close() {
        assert!(dates_within_days("2025-12-15", "2025-12-17", 3));
        assert!(dates_within_days("2025-12-15", "2025-12-12", 3));
    }

    #[test]
    fn dates_within_days_far() {
        assert!(!dates_within_days("2025-12-15", "2025-12-25", 3));
    }

    #[test]
    fn dates_within_days_invalid() {
        assert!(!dates_within_days("", "2025-12-15", 3));
        assert!(!dates_within_days("invalid", "2025-12-15", 3));
    }

    // ---- Duplicate detection ----

    #[test]
    fn find_duplicates_empty() {
        let groups = find_duplicate_groups(&[], 3);
        assert!(groups.is_empty());
    }

    #[test]
    fn find_duplicates_no_match() {
        let txns = vec![
            Transaction {
                id: "t1".into(),
                date: Some("2025-12-15".into()),
                name: Some("Store A".into()),
                amount: Some(serde_json::json!("-50.00")),
                item_id: None,
                txn_type: None,
                is_reviewed: None,
                category_id: None,
                account_id: None,
                recurring_id: None,
                user_notes: None,
                tags: None,
            },
            Transaction {
                id: "t2".into(),
                date: Some("2025-12-15".into()),
                name: Some("Store B".into()),
                amount: Some(serde_json::json!("-100.00")),
                item_id: None,
                txn_type: None,
                is_reviewed: None,
                category_id: None,
                account_id: None,
                recurring_id: None,
                user_notes: None,
                tags: None,
            },
        ];
        let groups = find_duplicate_groups(&txns, 3);
        assert!(groups.is_empty());
    }

    #[test]
    fn find_duplicates_match() {
        let txns = vec![
            Transaction {
                id: "t1".into(),
                date: Some("2025-12-15".into()),
                name: Some("Amazon".into()),
                amount: Some(serde_json::json!("-50.00")),
                item_id: None,
                txn_type: None,
                is_reviewed: None,
                category_id: None,
                account_id: None,
                recurring_id: None,
                user_notes: None,
                tags: None,
            },
            Transaction {
                id: "t2".into(),
                date: Some("2025-12-16".into()),
                name: Some("Amazon".into()),
                amount: Some(serde_json::json!("-50.00")),
                item_id: None,
                txn_type: None,
                is_reviewed: None,
                category_id: None,
                account_id: None,
                recurring_id: None,
                user_notes: None,
                tags: None,
            },
        ];
        let groups = find_duplicate_groups(&txns, 3);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].len(), 2);
    }

    // ---- CSV escape ----

    #[test]
    fn csv_escape_field_simple() {
        assert_eq!(csv_escape_field("hello"), "hello");
        assert_eq!(csv_escape_field("hello,world"), "\"hello,world\"");
        assert_eq!(csv_escape_field("say \"hi\""), "\"say \"\"hi\"\"\"");
    }
}
