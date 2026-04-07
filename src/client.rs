use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::config::{load_token, save_token};
use crate::ops;
use crate::types::{
    AccountId, CategoryId, InstitutionId, ItemId, RecurringFrequency, RecurringId, TagId,
    TransactionId, TransactionType,
};

#[derive(Debug, Clone)]
pub enum ClientMode {
    Http {
        base_url: String,
        token: Option<String>,
        token_file: PathBuf,
        session_dir: Option<PathBuf>,
        /// When true and running in a terminal, automatically open a browser to
        /// re-authenticate if the token is expired and session refresh fails.
        auto_login: bool,
    },
    Fixtures(PathBuf),
}

#[derive(Debug, Clone)]
pub struct CopilotClient {
    mode: ClientMode,
}

impl CopilotClient {
    pub fn new(mode: ClientMode) -> Self {
        Self { mode }
    }

    pub fn try_user_query(&self) -> anyhow::Result<()> {
        let _ = self.graphql("User", ops::USER, json!({}))?;
        Ok(())
    }

    pub fn list_transactions(&self, limit: usize) -> anyhow::Result<Vec<Transaction>> {
        Ok(self
            .list_transactions_page(limit, None, None, None)?
            .transactions)
    }

    pub fn list_transactions_page(
        &self,
        first: usize,
        after: Option<String>,
        filter: Option<Value>,
        sort: Option<Value>,
    ) -> anyhow::Result<TransactionsPage> {
        let data = self.graphql(
            "Transactions",
            ops::TRANSACTIONS,
            json!({
                "first": first,
                "after": after,
                "filter": filter,
                "sort": sort,
            }),
        )?;

        let edges = data
            .pointer("/data/transactions/edges")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("unexpected Transactions response shape"))?;

        let mut transactions = Vec::new();
        for edge in edges {
            if let Some(node) = edge.pointer("/node") {
                let t: Transaction = serde_json::from_value(node.clone())?;
                transactions.push(t);
            }
        }

        let page_info = data
            .pointer("/data/transactions/pageInfo")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let page_info: PageInfo = serde_json::from_value(page_info)?;

        Ok(TransactionsPage {
            transactions,
            page_info,
        })
    }

    pub fn list_categories(
        &self,
        spend: bool,
        budget: bool,
        rollovers: bool,
    ) -> anyhow::Result<Vec<Category>> {
        let data = self.graphql(
            "Categories",
            ops::CATEGORIES,
            json!({
                "spend": spend,
                "budget": budget,
                "rollovers": rollovers
            }),
        )?;

        let items = data
            .pointer("/data/categories")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("unexpected Categories response shape"))?;

        let mut out = Vec::new();
        for item in items {
            let c: Category = serde_json::from_value(item.clone())?;
            out.push(c);
        }
        Ok(out)
    }

    pub fn list_recurrings(&self) -> anyhow::Result<Vec<Recurring>> {
        let data = self.graphql("Recurrings", ops::RECURRINGS, json!({ "filter": null }))?;
        let items = data
            .pointer("/data/recurrings")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("unexpected Recurrings response shape"))?;

        let mut out = Vec::new();
        for item in items {
            let r: Recurring = serde_json::from_value(item.clone())?;
            out.push(r);
        }
        Ok(out)
    }

    pub fn list_tags(&self) -> anyhow::Result<Vec<Tag>> {
        let data = self.graphql("Tags", ops::TAGS, json!({}))?;
        let items = data
            .pointer("/data/tags")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("unexpected Tags response shape"))?;

        let mut out = Vec::new();
        for item in items {
            let t: Tag = serde_json::from_value(item.clone())?;
            out.push(t);
        }
        Ok(out)
    }

    pub fn list_budget_months(&self) -> anyhow::Result<Vec<BudgetMonth>> {
        let data = self.graphql("Budgets", ops::BUDGETS, json!({}))?;
        let histories = data
            .pointer("/data/categoriesTotal/budget/histories")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("unexpected Budgets response shape"))?;

        let mut out = Vec::new();
        for item in histories {
            let month = item
                .get("month")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let amount = item
                .get("amount")
                .map(|v| v.to_string())
                .unwrap_or_else(|| "null".into());
            out.push(BudgetMonth { month, amount });
        }
        Ok(out)
    }

    pub fn bulk_edit_transactions_reviewed(
        &self,
        ids: Vec<TransactionIdRef>,
        is_reviewed: bool,
    ) -> anyhow::Result<BulkEditTransactionsResult> {
        let data = self.graphql(
            "BulkEditTransactions",
            ops::BULK_EDIT_TRANSACTIONS,
            json!({
                "filter": { "ids": ids },
                "input": { "isReviewed": is_reviewed }
            }),
        )?;

        let updated = data
            .pointer("/data/bulkEditTransactions/updated")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("unexpected BulkEditTransactions response shape"))?;
        let mut updated_out = Vec::new();
        for item in updated {
            let t: Transaction = serde_json::from_value(item.clone())?;
            updated_out.push(t);
        }

        let failed_out = match data
            .pointer("/data/bulkEditTransactions/failed")
            .and_then(|v| v.as_array())
        {
            None => Vec::new(),
            Some(items) => {
                let mut out = Vec::new();
                for item in items {
                    let f: BulkEditFailed = serde_json::from_value(item.clone())?;
                    out.push(f);
                }
                out
            }
        };

        Ok(BulkEditTransactionsResult {
            updated: updated_out,
            failed: failed_out,
        })
    }

    pub fn edit_transaction(
        &self,
        item_id: &ItemId,
        account_id: &AccountId,
        id: &TransactionId,
        input: Value,
    ) -> anyhow::Result<Transaction> {
        let data = self.graphql(
            "EditTransaction",
            ops::EDIT_TRANSACTION,
            json!({
                "itemId": item_id.as_str(),
                "accountId": account_id.as_str(),
                "id": id.as_str(),
                "input": input
            }),
        )?;

        let txn = data
            .pointer("/data/editTransaction/transaction")
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unexpected EditTransaction response shape"))?;
        Ok(serde_json::from_value(txn)?)
    }

    pub fn add_transaction_to_recurring(
        &self,
        item_id: &ItemId,
        account_id: &AccountId,
        id: &TransactionId,
        recurring_id: &RecurringId,
    ) -> anyhow::Result<Transaction> {
        let data = self.graphql(
            "AddTransactionToRecurring",
            ops::ADD_TRANSACTION_TO_RECURRING,
            json!({
                "itemId": item_id.as_str(),
                "accountId": account_id.as_str(),
                "id": id.as_str(),
                "input": {
                    "isExcluded": false,
                    "recurringId": recurring_id.as_str()
                }
            }),
        )?;

        let txn = data
            .pointer("/data/addTransactionToRecurring/transaction")
            .cloned()
            .ok_or_else(|| {
                anyhow::anyhow!("unexpected AddTransactionToRecurring response shape")
            })?;
        Ok(serde_json::from_value(txn)?)
    }

    pub fn delete_tag(&self, id: &TagId) -> anyhow::Result<bool> {
        let data = self.graphql(
            "DeleteTag",
            ops::DELETE_TAG,
            json!({
                "id": id.as_str(),
            }),
        )?;

        let v = data
            .pointer("/data/deleteTag")
            .and_then(|v| v.as_bool())
            .ok_or_else(|| anyhow::anyhow!("unexpected DeleteTag response shape"))?;
        Ok(v)
    }

    pub fn create_tag(&self, name: &str, color_name: Option<&str>) -> anyhow::Result<Tag> {
        let data = self.graphql(
            "CreateTag",
            ops::CREATE_TAG,
            json!({
                "input": {
                    "name": name,
                    "colorName": color_name
                }
            }),
        )?;

        let tag = data
            .pointer("/data/createTag")
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unexpected CreateTag response shape"))?;
        Ok(serde_json::from_value(tag)?)
    }

    pub fn create_category(
        &self,
        input: Value,
        spend: bool,
        budget: bool,
    ) -> anyhow::Result<Category> {
        let data = self.graphql(
            "CreateCategory",
            ops::CREATE_CATEGORY,
            json!({
                "input": input,
                "spend": spend,
                "budget": budget
            }),
        )?;

        let cat = data
            .pointer("/data/createCategory")
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unexpected CreateCategory response shape"))?;
        Ok(serde_json::from_value(cat)?)
    }

    pub fn create_recurring_from_transaction(
        &self,
        item_id: &ItemId,
        account_id: &AccountId,
        transaction_id: &TransactionId,
        frequency: RecurringFrequency,
    ) -> anyhow::Result<Recurring> {
        let data = self.graphql(
            "CreateRecurring",
            ops::CREATE_RECURRING,
            json!({
                "input": {
                    "frequency": frequency,
                    "transaction": {
                        "accountId": account_id.as_str(),
                        "itemId": item_id.as_str(),
                        "transactionId": transaction_id.as_str()
                    }
                }
            }),
        )?;

        let recurring = data
            .pointer("/data/createRecurring")
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unexpected CreateRecurring response shape"))?;
        Ok(serde_json::from_value(recurring)?)
    }

    pub fn edit_recurring(&self, id: &RecurringId, input: Value) -> anyhow::Result<Recurring> {
        let data = self.graphql(
            "EditRecurring",
            ops::EDIT_RECURRING,
            json!({
                "id": id.as_str(),
                "input": input
            }),
        )?;

        let recurring = data
            .pointer("/data/editRecurring/recurring")
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unexpected EditRecurring response shape"))?;
        Ok(serde_json::from_value(recurring)?)
    }

    pub fn list_accounts(&self) -> anyhow::Result<Vec<Account>> {
        let data = self.graphql(
            "Accounts",
            ops::ACCOUNTS,
            json!({ "filter": null, "accountLink": false }),
        )?;
        let items = data
            .pointer("/data/accounts")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("unexpected Accounts response shape"))?;
        let mut out = Vec::new();
        for item in items {
            let a: Account = serde_json::from_value(item.clone())?;
            out.push(a);
        }
        Ok(out)
    }

    pub fn get_networth(&self, time_frame: Option<&str>) -> anyhow::Result<Vec<NetworthEntry>> {
        let data = self.graphql(
            "Networth",
            ops::NETWORTH,
            json!({ "timeFrame": time_frame }),
        )?;
        let items = data
            .pointer("/data/networthHistory")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("unexpected Networth response shape"))?;
        let mut out = Vec::new();
        for item in items {
            let e: NetworthEntry = serde_json::from_value(item.clone())?;
            out.push(e);
        }
        Ok(out)
    }

    pub fn get_networth_live_balance(&self) -> anyhow::Result<NetworthEntry> {
        let data = self.graphql(
            "NetworthLiveBalance",
            ops::NETWORTH_LIVE_BALANCE,
            json!({}),
        )?;
        let entry = data
            .pointer("/data/networthLiveBalance")
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unexpected NetworthLiveBalance response shape"))?;
        Ok(serde_json::from_value(entry)?)
    }

    pub fn list_monthly_spend(&self) -> anyhow::Result<Vec<MonthlySpendEntry>> {
        let data = self.graphql("MonthlySpend", ops::MONTHLY_SPEND, json!({}))?;
        let items = data
            .pointer("/data/monthlySpending")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("unexpected MonthlySpend response shape"))?;
        let mut out = Vec::new();
        for item in items {
            let e: MonthlySpendEntry = serde_json::from_value(item.clone())?;
            out.push(e);
        }
        Ok(out)
    }

    pub fn list_spends(&self, history: bool) -> anyhow::Result<SpendData> {
        let data = self.graphql("Spends", ops::SPENDS, json!({ "history": history }))?;
        let spend = data
            .pointer("/data/categoriesTotal/spend")
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unexpected Spends response shape"))?;
        Ok(serde_json::from_value(spend)?)
    }

    pub fn list_upcoming_recurrings(&self) -> anyhow::Result<Vec<UpcomingRecurring>> {
        let data = self.graphql(
            "UpcomingRecurrings",
            ops::UPCOMING_RECURRINGS,
            json!({}),
        )?;
        let items = data
            .pointer("/data/unpaidUpcomingRecurrings")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("unexpected UpcomingRecurrings response shape"))?;
        let mut out = Vec::new();
        for item in items {
            let r: UpcomingRecurring = serde_json::from_value(item.clone())?;
            out.push(r);
        }
        Ok(out)
    }

    pub fn get_transaction_summary(
        &self,
        filter: Option<Value>,
    ) -> anyhow::Result<TransactionSummary> {
        let data = self.graphql(
            "TransactionSummary",
            ops::TRANSACTION_SUMMARY,
            json!({ "filter": filter }),
        )?;
        let summary = data
            .pointer("/data/transactionsSummary")
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unexpected TransactionSummary response shape"))?;
        Ok(serde_json::from_value(summary)?)
    }

    pub fn edit_category(&self, id: &CategoryId, input: Value) -> anyhow::Result<Category> {
        let data = self.graphql(
            "EditCategory",
            ops::EDIT_CATEGORY,
            json!({
                "id": id.as_str(),
                "input": input
            }),
        )?;
        let cat = data
            .pointer("/data/editCategory")
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unexpected EditCategory response shape"))?;
        Ok(serde_json::from_value(cat)?)
    }

    fn graphql(
        &self,
        operation_name: &str,
        query: &str,
        variables: Value,
    ) -> anyhow::Result<Value> {
        match &self.mode {
            ClientMode::Fixtures(dir) => {
                let path = dir.join(format!("{operation_name}.json"));
                let s = fs::read_to_string(&path)?;
                Ok(serde_json::from_str(&s)?)
            }
            ClientMode::Http {
                base_url,
                token,
                token_file,
                session_dir,
                auto_login,
            } => {
                let url = format!("{}/api/graphql", base_url.trim_end_matches('/'));
                let http = http_client_from_env()?;

                let mut current_token = token.clone().or_else(|| load_token(token_file).ok());

                // Up to 3 attempts:
                //   1 → original token
                //   2 → session-based headless refresh (if session dir exists)
                //   3 → interactive browser login  (if auto_login + terminal)
                for attempt in 1..=3 {
                    let mut req = http.post(&url).json(&json!({
                        "operationName": operation_name,
                        "query": query,
                        "variables": variables
                    }));
                    if let Some(t) = current_token.as_ref() {
                        req = req.bearer_auth(t);
                    }

                    let resp = req.send()?;
                    let status = resp.status();
                    let body: Value = resp.json()?;

                    if is_unauthenticated(&body) {
                        // Attempt 1 → try silent session refresh
                        if attempt == 1 {
                            if let Some(dir) = session_dir.as_ref().filter(|d| d.exists()) {
                                match refresh_token_via_session(dir, 180) {
                                    Ok(refreshed) => {
                                        let _ = save_token(token_file, &refreshed);
                                        current_token = Some(refreshed);
                                        continue;
                                    }
                                    Err(_) => {
                                        // Session refresh failed; fall through to auto-login
                                    }
                                }
                            }
                            // No session dir or refresh failed — try auto-login if allowed
                            if !should_attempt_auto_login(*auto_login) {
                                anyhow::bail!(
                                    "unauthenticated (token missing/expired). Re-run `copilot auth login` (or `copilot auth set-token`)."
                                );
                            }
                        }

                        // Attempt 2 → auto-login via interactive browser
                        if attempt <= 2 && should_attempt_auto_login(*auto_login) {
                            eprintln!();
                            eprintln!("Token expired — launching browser to re-authenticate...");
                            match auto_login_via_browser(
                                session_dir.clone(),
                                token_file,
                                300,
                            ) {
                                Ok(refreshed) => {
                                    current_token = Some(refreshed);
                                    eprintln!("Token refreshed successfully.");
                                    eprintln!();
                                    continue;
                                }
                                Err(e) => {
                                    anyhow::bail!(
                                        "auto-login failed: {e}\n\nRe-run `copilot auth login` manually."
                                    );
                                }
                            }
                        }

                        anyhow::bail!(
                            "unauthenticated (token missing/expired). Re-run `copilot auth login` (or `copilot auth set-token`)."
                        );
                    }

                    if let Some(msg) = format_graphql_error(&body) {
                        anyhow::bail!("{msg}");
                    }

                    if !status.is_success() {
                        anyhow::bail!("graphql http error {status}");
                    }
                    return Ok(body);
                }

                unreachable!("loop returns or errors")
            }
        }
    }
}

fn http_client_from_env() -> anyhow::Result<reqwest::blocking::Client> {
    let timeout_secs: u64 = std::env::var("COPILOT_HTTP_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60);
    let connect_timeout_secs: u64 = std::env::var("COPILOT_HTTP_CONNECT_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);

    Ok(reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .connect_timeout(Duration::from_secs(connect_timeout_secs))
        .build()?)
}

fn is_unauthenticated(body: &Value) -> bool {
    body.get("errors")
        .and_then(|e| e.as_array())
        .and_then(|arr| arr.first())
        .and_then(|e| e.get("extensions"))
        .and_then(|ext| ext.get("code"))
        .and_then(|c| c.as_str())
        == Some("UNAUTHENTICATED")
}

fn format_graphql_error(body: &Value) -> Option<String> {
    let errors = body.get("errors")?.as_array()?;
    let first = errors.first()?;
    let message = first.get("message").and_then(|m| m.as_str()).unwrap_or("");
    let code = first
        .get("extensions")
        .and_then(|e| e.get("code"))
        .and_then(|c| c.as_str());

    if message.is_empty() && code.is_none() {
        return None;
    }

    let mut out = String::new();
    out.push_str("graphql error");
    if let Some(c) = code {
        out.push_str(&format!(" ({c})"));
    }
    if !message.is_empty() {
        out.push_str(&format!(": {message}"));
    }
    Some(out)
}

/// Returns true if auto-login should be attempted. In production this requires
/// a desktop environment where a browser window can open and the user can see
/// status messages. We check stderr (not stdin) because the browser helper
/// opens a GUI window — it does not need interactive stdin.
///
/// During tests, the `COPILOT_TEST_AUTO_LOGIN_TOKEN` env var bypasses the
/// terminal check so the full retry path can be exercised.
fn should_attempt_auto_login(auto_login: bool) -> bool {
    if !auto_login {
        return false;
    }
    // Test hook: allow the auto-login path to be exercised without a real terminal
    if std::env::var("COPILOT_TEST_AUTO_LOGIN_TOKEN")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .is_some()
    {
        return true;
    }
    // Explicit opt-in via env var — useful when running from an IDE, agent, or
    // piped context where the TTY check fails but a browser can still open.
    if std::env::var("COPILOT_AUTO_LOGIN")
        .ok()
        .filter(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .is_some()
    {
        return true;
    }
    // Check stderr rather than stdin: the browser helper opens a GUI window and
    // doesn't need stdin. What matters is whether a human can see our status
    // messages. This also allows auto-login when stdin is piped but the user is
    // watching (e.g. running from an IDE/agent terminal).
    std::io::IsTerminal::is_terminal(&std::io::stderr())
}

fn refresh_token_via_session(session_dir: &Path, timeout_seconds: u64) -> anyhow::Result<String> {
    // Test hook: allow deterministic refresh without running the browser helper.
    // (Used by unit tests that simulate an expired token + refresh + retry.)
    if let Ok(t) = std::env::var("COPILOT_TEST_REFRESH_TOKEN")
        && !t.trim().is_empty()
    {
        return Ok(t.trim().to_string());
    }

    let Some(helper) = crate::config::token_helper_path() else {
        anyhow::bail!(
            "token refresh helper not found (install python3 + playwright, or re-run `copilot auth set-token`)"
        );
    };
    let out = std::process::Command::new("python3")
        .arg(helper)
        .args(["--mode", "session"])
        .args(["--user-data-dir", session_dir.to_string_lossy().as_ref()])
        .args(["--timeout-seconds", &timeout_seconds.to_string()])
        .output()?;

    if !out.status.success() {
        anyhow::bail!("token refresh helper failed");
    }
    let token = String::from_utf8(out.stdout)?.trim().to_string();
    if token.is_empty() {
        anyhow::bail!("token refresh helper returned empty token");
    }
    Ok(token)
}

/// Launch a headful browser for the user to log in interactively.
/// The session is persisted so future calls can use silent refresh.
/// The new token is saved to `token_file`.
fn auto_login_via_browser(
    session_dir: Option<PathBuf>,
    token_file: &Path,
    timeout_seconds: u64,
) -> anyhow::Result<String> {
    // Test hook: return a deterministic token and persist it, just like the real path would.
    if let Ok(t) = std::env::var("COPILOT_TEST_AUTO_LOGIN_TOKEN")
        && !t.trim().is_empty()
    {
        let token = t.trim().to_string();
        save_token(token_file, &token)?;
        return Ok(token);
    }

    let Some(helper) = crate::config::token_helper_path() else {
        anyhow::bail!(
            "browser helper not found. Install python3 + playwright, then run `copilot auth login`."
        );
    };

    // Ensure session dir exists so the login persists for future silent refreshes.
    let session = session_dir.unwrap_or_else(crate::config::session_path);
    crate::config::ensure_private_dir(&session)?;

    let out = std::process::Command::new("python3")
        .arg(&helper)
        .args(["--mode", "interactive", "--headful"])
        .args(["--user-data-dir", session.to_string_lossy().as_ref()])
        .args(["--timeout-seconds", &timeout_seconds.to_string()])
        .stdin(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .output()?;

    if !out.status.success() {
        anyhow::bail!("browser login failed (helper exited with {})", out.status);
    }

    let token = String::from_utf8(out.stdout)?.trim().to_string();
    if token.is_empty() {
        anyhow::bail!("browser login produced no token — was the login completed?");
    }

    save_token(token_file, &token)?;
    Ok(token)
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PageInfo {
    #[serde(rename = "endCursor")]
    pub end_cursor: Option<String>,
    #[serde(rename = "hasNextPage")]
    pub has_next_page: Option<bool>,
    #[serde(rename = "hasPreviousPage")]
    pub has_previous_page: Option<bool>,
    #[serde(rename = "startCursor")]
    pub start_cursor: Option<String>,
}

#[derive(Debug)]
pub struct TransactionsPage {
    pub transactions: Vec<Transaction>,
    pub page_info: PageInfo,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Tag {
    pub id: TagId,
    pub name: Option<String>,
    #[serde(rename = "colorName")]
    pub color_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Transaction {
    pub id: TransactionId,
    pub date: Option<String>,
    pub name: Option<String>,
    pub amount: Option<Value>,
    #[serde(rename = "itemId")]
    pub item_id: Option<ItemId>,
    #[serde(rename = "type")]
    pub txn_type: Option<TransactionType>,
    #[serde(rename = "isReviewed")]
    pub is_reviewed: Option<bool>,
    #[serde(rename = "categoryId")]
    pub category_id: Option<CategoryId>,
    #[serde(rename = "accountId")]
    pub account_id: Option<AccountId>,
    #[serde(rename = "recurringId")]
    pub recurring_id: Option<RecurringId>,
    #[serde(rename = "userNotes")]
    pub user_notes: Option<String>,
    pub tags: Option<Vec<Tag>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TransactionIdRef {
    #[serde(rename = "accountId")]
    pub account_id: AccountId,
    pub id: TransactionId,
    #[serde(rename = "itemId")]
    pub item_id: ItemId,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BulkEditFailed {
    pub error: Option<String>,
    #[serde(rename = "errorCode")]
    pub error_code: Option<String>,
}

#[derive(Debug)]
pub struct BulkEditTransactionsResult {
    pub updated: Vec<Transaction>,
    pub failed: Vec<BulkEditFailed>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "__typename")]
pub enum Icon {
    EmojiUnicode {
        unicode: Option<String>,
    },
    Genmoji {
        id: Option<String>,
        src: Option<String>,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Category {
    pub id: CategoryId,
    pub name: Option<String>,
    #[serde(rename = "isRolloverDisabled")]
    pub is_rollover_disabled: Option<bool>,
    #[serde(rename = "canBeDeleted")]
    pub can_be_deleted: Option<bool>,
    #[serde(rename = "isExcluded")]
    pub is_excluded: Option<bool>,
    #[serde(rename = "templateId")]
    pub template_id: Option<String>,
    #[serde(rename = "colorName")]
    pub color_name: Option<String>,
    pub icon: Option<Icon>,
    #[serde(rename = "childCategories")]
    pub child_categories: Option<Vec<Category>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Recurring {
    pub id: RecurringId,
    pub name: Option<String>,
    pub frequency: Option<RecurringFrequency>,
    #[serde(rename = "categoryId")]
    pub category_id: Option<CategoryId>,
}

#[derive(Debug, Clone)]
pub struct BudgetMonth {
    pub month: String,
    pub amount: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Account {
    pub id: AccountId,
    pub name: Option<String>,
    #[serde(rename = "type")]
    pub account_type: Option<String>,
    #[serde(rename = "subType")]
    pub sub_type: Option<String>,
    pub mask: Option<String>,
    pub balance: Option<Value>,
    pub limit: Option<Value>,
    #[serde(rename = "itemId")]
    pub item_id: Option<ItemId>,
    #[serde(rename = "institutionId")]
    pub institution_id: Option<InstitutionId>,
    #[serde(rename = "isManual")]
    pub is_manual: Option<bool>,
    #[serde(rename = "isUserHidden")]
    pub is_user_hidden: Option<bool>,
    #[serde(rename = "isUserClosed")]
    pub is_user_closed: Option<bool>,
    pub color: Option<String>,
    #[serde(rename = "hasLiveBalance")]
    pub has_live_balance: Option<bool>,
    #[serde(rename = "liveBalance")]
    pub live_balance: Option<Value>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct NetworthEntry {
    pub assets: Option<Value>,
    pub date: Option<String>,
    pub debt: Option<Value>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct MonthlySpendEntry {
    pub id: Option<String>,
    pub date: Option<String>,
    #[serde(rename = "totalAmount")]
    pub total_amount: Option<Value>,
    #[serde(rename = "comparisonAmount")]
    pub comparison_amount: Option<Value>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SpendData {
    pub current: Option<SpendMonthly>,
    pub histories: Option<Vec<SpendMonthly>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SpendMonthly {
    pub id: Option<String>,
    pub month: Option<String>,
    pub amount: Option<Value>,
    #[serde(rename = "comparisonAmount")]
    pub comparison_amount: Option<Value>,
    #[serde(rename = "unpaidRecurringAmount")]
    pub unpaid_recurring_amount: Option<Value>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct UpcomingRecurring {
    pub id: RecurringId,
    pub name: Option<String>,
    pub frequency: Option<RecurringFrequency>,
    #[serde(rename = "categoryId")]
    pub category_id: Option<CategoryId>,
    pub state: Option<String>,
    pub emoji: Option<String>,
    pub icon: Option<Icon>,
    #[serde(rename = "nextPaymentDate")]
    pub next_payment_date: Option<String>,
    #[serde(rename = "nextPaymentAmount")]
    pub next_payment_amount: Option<Value>,
    pub rule: Option<RecurringRule>,
    pub payments: Option<Vec<RecurringPayment>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RecurringRule {
    #[serde(rename = "nameContains")]
    pub name_contains: Option<String>,
    #[serde(rename = "minAmount")]
    pub min_amount: Option<Value>,
    #[serde(rename = "maxAmount")]
    pub max_amount: Option<Value>,
    pub days: Option<Vec<i32>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RecurringPayment {
    pub amount: Option<Value>,
    #[serde(rename = "isPaid")]
    pub is_paid: Option<bool>,
    pub date: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TransactionSummary {
    #[serde(rename = "transactionsCount")]
    pub transactions_count: Option<i64>,
    #[serde(rename = "totalNetIncome")]
    pub total_net_income: Option<Value>,
    #[serde(rename = "totalIncome")]
    pub total_income: Option<Value>,
    #[serde(rename = "totalSpent")]
    pub total_spent: Option<Value>,
}
