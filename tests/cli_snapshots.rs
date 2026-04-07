use assert_cmd::Command;

fn run(args: &[&str]) -> String {
    let tmp_home = tempfile::tempdir().unwrap();
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("copilot"));
    cmd.env("HOME", tmp_home.path());
    cmd.env_remove("COPILOT_TOKEN");
    cmd.env_remove("COPILOT_TOKEN_FILE");
    cmd.env("COPILOT_FIXTURES_DIR", "tests/fixtures/graphql");
    cmd.args(args);
    let out = cmd.assert().success().get_output().stdout.clone();
    String::from_utf8(out).unwrap()
}

#[test]
fn transactions_list_table_snapshot() {
    insta::assert_snapshot!(run(&["transactions", "list"]));
}

#[test]
fn transactions_list_json_snapshot() {
    insta::assert_snapshot!(run(&["--output", "json", "transactions", "list"]));
}

#[test]
fn transactions_list_table_page_info_snapshot() {
    insta::assert_snapshot!(run(&["transactions", "list", "--page-info"]));
}

#[test]
fn transactions_list_json_page_info_snapshot() {
    insta::assert_snapshot!(run(&[
        "--output",
        "json",
        "transactions",
        "list",
        "--page-info"
    ]));
}

#[test]
fn transactions_list_table_filter_tag_snapshot() {
    insta::assert_snapshot!(run(&["transactions", "list", "--tag", "Shopping"]));
}

#[test]
fn transactions_list_table_filter_category_snapshot() {
    insta::assert_snapshot!(run(&["transactions", "list", "--category-id", "cat_other"]));
}

#[test]
fn transactions_search_table_snapshot() {
    insta::assert_snapshot!(run(&["transactions", "search", "amazon"]));
}

#[test]
fn transactions_search_json_snapshot() {
    insta::assert_snapshot!(run(&[
        "--output",
        "json",
        "transactions",
        "search",
        "amazon"
    ]));
}

#[test]
fn transactions_show_table_snapshot() {
    insta::assert_snapshot!(run(&["transactions", "show", "txn_1"]));
}

#[test]
fn transactions_show_json_snapshot() {
    insta::assert_snapshot!(run(&["--output", "json", "transactions", "show", "txn_1"]));
}

#[test]
fn transactions_list_table_fields_and_sort_snapshot() {
    insta::assert_snapshot!(run(&[
        "transactions",
        "list",
        "--fields",
        "date,name,amount,reviewed,category,tags,type",
        "--sort",
        "date-desc",
    ]));
}

#[test]
fn transactions_list_table_filter_reviewed_snapshot() {
    insta::assert_snapshot!(run(&["transactions", "list", "--reviewed"]));
}

#[test]
fn transactions_list_table_filter_unreviewed_snapshot() {
    insta::assert_snapshot!(run(&["transactions", "list", "--unreviewed"]));
}

#[test]
fn transactions_list_table_filter_date_snapshot() {
    insta::assert_snapshot!(run(&["transactions", "list", "--date", "12-15-2025"]));
}

#[test]
fn transactions_set_category_by_name_table_snapshot() {
    insta::assert_snapshot!(run(&[
        "--yes",
        "transactions",
        "set-category",
        "txn_1",
        "--category",
        "Other",
    ]));
}

#[test]
fn transactions_set_notes_table_snapshot() {
    insta::assert_snapshot!(run(&[
        "--yes",
        "transactions",
        "set-notes",
        "txn_1",
        "--notes",
        "hello world",
    ]));
}

#[test]
fn transactions_clear_notes_table_snapshot() {
    insta::assert_snapshot!(run(&[
        "--yes",
        "transactions",
        "set-notes",
        "txn_1",
        "--clear",
    ]));
}

#[test]
fn transactions_set_tags_add_table_snapshot() {
    insta::assert_snapshot!(run(&[
        "--yes",
        "transactions",
        "set-tags",
        "txn_2",
        "--mode",
        "add",
        "--tag-id",
        "tag_shopping",
    ]));
}

#[test]
fn transactions_assign_recurring_table_snapshot() {
    insta::assert_snapshot!(run(&[
        "--yes",
        "transactions",
        "assign-recurring",
        "txn_1",
        "--recurring-id",
        "rec_1",
    ]));
}

#[test]
fn transactions_edit_type_table_snapshot() {
    insta::assert_snapshot!(run(&[
        "--yes",
        "transactions",
        "edit",
        "txn_1",
        "--type",
        "internal-transfer",
    ]));
}
#[test]
fn auth_status_table_snapshot() {
    insta::assert_snapshot!(run(&["auth", "status"]));
}

#[test]
fn auth_status_json_snapshot() {
    insta::assert_snapshot!(run(&["--output", "json", "auth", "status"]));
}

#[test]
fn categories_list_table_snapshot() {
    insta::assert_snapshot!(run(&["categories", "list"]));
}

#[test]
fn categories_list_json_snapshot() {
    insta::assert_snapshot!(run(&["--output", "json", "categories", "list"]));
}

#[test]
fn recurrings_list_table_snapshot() {
    insta::assert_snapshot!(run(&["recurrings", "list"]));
}

#[test]
fn recurrings_list_json_snapshot() {
    insta::assert_snapshot!(run(&["--output", "json", "recurrings", "list"]));
}

#[test]
fn categories_show_table_snapshot() {
    insta::assert_snapshot!(run(&["categories", "show", "cat_other"]));
}

#[test]
fn categories_show_json_snapshot() {
    insta::assert_snapshot!(run(&[
        "--output",
        "json",
        "categories",
        "show",
        "cat_other"
    ]));
}

#[test]
fn categories_create_table_snapshot() {
    insta::assert_snapshot!(run(&[
        "--yes",
        "categories",
        "create",
        "New Category",
        "--emoji",
        "😀",
        "--color-name",
        "BLUE1",
    ]));
}

#[test]
fn recurrings_show_table_snapshot() {
    insta::assert_snapshot!(run(&["recurrings", "show", "rec_1"]));
}

#[test]
fn recurrings_list_filtered_snapshot() {
    insta::assert_snapshot!(run(&["recurrings", "list", "--category-id", "cat_housing"]));
}

#[test]
fn recurrings_create_table_snapshot() {
    insta::assert_snapshot!(run(&[
        "--yes",
        "recurrings",
        "create",
        "txn_1",
        "--frequency",
        "monthly",
    ]));
}

#[test]
fn recurrings_edit_table_snapshot() {
    insta::assert_snapshot!(run(&[
        "--yes",
        "recurrings",
        "edit",
        "rec_1",
        "--name-contains",
        "rent",
        "--min-amount",
        "10",
        "--max-amount",
        "5000",
        "--recalculate-only-for-future",
    ]));
}

#[test]
fn budgets_month_table_snapshot() {
    insta::assert_snapshot!(run(&["budgets", "month"]));
}

#[test]
fn budgets_month_json_snapshot() {
    insta::assert_snapshot!(run(&["--output", "json", "budgets", "month"]));
}

// ---- Accounts ----

#[test]
fn accounts_list_table_snapshot() {
    insta::assert_snapshot!(run(&["accounts", "list"]));
}

#[test]
fn accounts_list_json_snapshot() {
    insta::assert_snapshot!(run(&["--output", "json", "accounts", "list"]));
}

#[test]
fn accounts_list_show_hidden_snapshot() {
    insta::assert_snapshot!(run(&["accounts", "list", "--show-hidden"]));
}

#[test]
fn accounts_list_filter_type_snapshot() {
    insta::assert_snapshot!(run(&["accounts", "list", "--account-type", "credit"]));
}

#[test]
fn accounts_list_filter_name_snapshot() {
    insta::assert_snapshot!(run(&["accounts", "list", "--name-contains", "chase"]));
}

#[test]
fn accounts_show_table_snapshot() {
    insta::assert_snapshot!(run(&["accounts", "show", "acct_1"]));
}

#[test]
fn accounts_show_json_snapshot() {
    insta::assert_snapshot!(run(&["--output", "json", "accounts", "show", "acct_1"]));
}

#[test]
fn accounts_list_csv_snapshot() {
    insta::assert_snapshot!(run(&["--output", "csv", "accounts", "list"]));
}

// ---- Net worth ----

#[test]
fn networth_current_table_snapshot() {
    insta::assert_snapshot!(run(&["networth", "current"]));
}

#[test]
fn networth_current_json_snapshot() {
    insta::assert_snapshot!(run(&["--output", "json", "networth", "current"]));
}

#[test]
fn networth_history_table_snapshot() {
    insta::assert_snapshot!(run(&["networth", "history"]));
}

#[test]
fn networth_history_json_snapshot() {
    insta::assert_snapshot!(run(&["--output", "json", "networth", "history"]));
}

// ---- Spending ----

#[test]
fn spending_monthly_table_snapshot() {
    insta::assert_snapshot!(run(&["spending", "monthly"]));
}

#[test]
fn spending_monthly_json_snapshot() {
    insta::assert_snapshot!(run(&["--output", "json", "spending", "monthly"]));
}

#[test]
fn spending_summary_table_snapshot() {
    insta::assert_snapshot!(run(&["spending", "summary"]));
}

#[test]
fn spending_summary_json_snapshot() {
    insta::assert_snapshot!(run(&["--output", "json", "spending", "summary"]));
}

#[test]
fn spending_history_table_snapshot() {
    insta::assert_snapshot!(run(&["spending", "history"]));
}

#[test]
fn spending_history_json_snapshot() {
    insta::assert_snapshot!(run(&["--output", "json", "spending", "history"]));
}

// ---- Upcoming recurrings ----

#[test]
fn recurrings_upcoming_table_snapshot() {
    insta::assert_snapshot!(run(&["recurrings", "upcoming"]));
}

#[test]
fn recurrings_upcoming_json_snapshot() {
    insta::assert_snapshot!(run(&["--output", "json", "recurrings", "upcoming"]));
}

// ---- Categories edit ----

#[test]
fn categories_edit_table_snapshot() {
    insta::assert_snapshot!(run(&[
        "--yes",
        "categories",
        "edit",
        "cat_other",
        "--name",
        "Renamed Category",
        "--color-name",
        "RED1",
    ]));
}

// ---- Transaction filters: date range, amount, account ----

#[test]
fn transactions_list_table_filter_from_snapshot() {
    // Both transactions have date 2025-12-15, so --from 2025-12-15 matches both
    insta::assert_snapshot!(run(&["transactions", "list", "--from", "2025-12-15"]));
}

#[test]
fn transactions_list_table_filter_to_snapshot() {
    insta::assert_snapshot!(run(&["transactions", "list", "--to", "2025-12-15"]));
}

#[test]
fn transactions_list_table_filter_from_to_range_snapshot() {
    insta::assert_snapshot!(run(&[
        "transactions",
        "list",
        "--from",
        "2025-12-01",
        "--to",
        "2025-12-31",
    ]));
}

#[test]
fn transactions_list_table_filter_from_excludes_snapshot() {
    // future date: no results
    insta::assert_snapshot!(run(&["transactions", "list", "--from", "2026-01-01"]));
}

#[test]
fn transactions_list_table_filter_min_amount_snapshot() {
    // txn_1 is -100, txn_2 is -57.48. Min 80 should only match txn_1
    insta::assert_snapshot!(run(&["transactions", "list", "--min-amount", "80"]));
}

#[test]
fn transactions_list_table_filter_max_amount_snapshot() {
    // max 60 should only match txn_2 (57.48)
    insta::assert_snapshot!(run(&["transactions", "list", "--max-amount", "60"]));
}

#[test]
fn transactions_list_table_filter_amount_range_snapshot() {
    // 50..80 should only match txn_2 (57.48)
    insta::assert_snapshot!(run(&[
        "transactions",
        "list",
        "--min-amount",
        "50",
        "--max-amount",
        "80",
    ]));
}

#[test]
fn transactions_list_table_filter_account_snapshot() {
    insta::assert_snapshot!(run(&["transactions", "list", "--account", "chase"]));
}

#[test]
fn transactions_list_table_filter_account_id_snapshot() {
    insta::assert_snapshot!(run(&["transactions", "list", "--account-id", "acct_2"]));
}

// ---- Totals ----

#[test]
fn transactions_list_table_totals_snapshot() {
    insta::assert_snapshot!(run(&["transactions", "list", "--totals"]));
}

// ---- CSV output ----

#[test]
fn transactions_list_csv_snapshot() {
    insta::assert_snapshot!(run(&["--output", "csv", "transactions", "list"]));
}

#[test]
fn transactions_list_csv_with_fields_snapshot() {
    insta::assert_snapshot!(run(&[
        "--output",
        "csv",
        "transactions",
        "list",
        "--fields",
        "date,name,amount,account,id",
    ]));
}

// ---- New fields: account, notes ----

#[test]
fn transactions_list_table_fields_account_notes_snapshot() {
    insta::assert_snapshot!(run(&[
        "transactions",
        "list",
        "--fields",
        "date,name,amount,account,notes,id",
    ]));
}

// ---- Enhanced search (notes, category, tags) ----

#[test]
fn transactions_search_by_category_name_snapshot() {
    // "other" should match txn_1 via category name "Other"
    insta::assert_snapshot!(run(&["transactions", "search", "other"]));
}

#[test]
fn transactions_search_by_tag_name_snapshot() {
    // "shopping" should match txn_2 via tag name "Shopping"
    insta::assert_snapshot!(run(&["transactions", "search", "shopping"]));
}

// ---- Duplicate detection ----

#[test]
fn transactions_duplicates_snapshot() {
    // With only 2 different-amount transactions in fixture, no duplicates expected
    insta::assert_snapshot!(run(&["transactions", "duplicates"]));
}

// ---- Shell completions ----

#[test]
fn completions_bash_works() {
    // Just verify it produces output without error
    let output = run(&["completions", "bash"]);
    assert!(output.contains("copilot"));
}

#[test]
fn completions_zsh_works() {
    let output = run(&["completions", "zsh"]);
    assert!(output.contains("copilot"));
}

#[test]
fn completions_fish_works() {
    let output = run(&["completions", "fish"]);
    assert!(output.contains("copilot"));
}
