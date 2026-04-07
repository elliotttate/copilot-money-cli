use comfy_table::Cell;
use serde::Serialize;

use crate::client::CopilotClient;
use crate::types::AccountId;

use super::render::{KeyValueRow, TableRow, render_output, shorten_id_for_table};
use super::{AccountsCmd, Cli, value_to_money_string};

pub(super) fn run_accounts(
    cli: &Cli,
    client: &CopilotClient,
    cmd: AccountsCmd,
) -> anyhow::Result<()> {
    match cmd {
        AccountsCmd::List(args) => {
            let mut items = client.list_accounts()?;

            if !args.show_hidden {
                items.retain(|a| !a.is_user_hidden.unwrap_or(false));
            }
            if !args.show_closed {
                items.retain(|a| !a.is_user_closed.unwrap_or(false));
            }

            if let Some(ref q) = args.name_contains {
                let q = q.to_lowercase();
                items.retain(|a| {
                    a.name
                        .as_deref()
                        .map(|n| n.to_lowercase().contains(&q))
                        .unwrap_or(false)
                });
            }

            if let Some(ref t) = args.account_type {
                let t = t.to_lowercase();
                items.retain(|a| {
                    a.account_type
                        .as_deref()
                        .map(|at| at.to_lowercase() == t)
                        .unwrap_or(false)
                });
            }

            let rows: Vec<AccountRow> = items
                .into_iter()
                .map(|a| AccountRow {
                    id: a.id,
                    name: a.name.unwrap_or_default(),
                    account_type: a.account_type.unwrap_or_default(),
                    sub_type: a.sub_type.unwrap_or_default(),
                    mask: a.mask.unwrap_or_default(),
                    balance: value_to_money_string(a.balance),
                })
                .collect();
            render_output(cli, rows)
        }
        AccountsCmd::Show { id } => {
            let items = client.list_accounts()?;
            let found = items.into_iter().find(|a| a.id == id);
            match found {
                Some(a) => render_output(
                    cli,
                    vec![
                        KeyValueRow {
                            key: "id".into(),
                            value: a.id.to_string(),
                        },
                        KeyValueRow {
                            key: "name".into(),
                            value: a.name.unwrap_or_default(),
                        },
                        KeyValueRow {
                            key: "type".into(),
                            value: a.account_type.unwrap_or_default(),
                        },
                        KeyValueRow {
                            key: "sub_type".into(),
                            value: a.sub_type.unwrap_or_default(),
                        },
                        KeyValueRow {
                            key: "mask".into(),
                            value: a.mask.unwrap_or_default(),
                        },
                        KeyValueRow {
                            key: "balance".into(),
                            value: value_to_money_string(a.balance),
                        },
                        KeyValueRow {
                            key: "limit".into(),
                            value: value_to_money_string(a.limit),
                        },
                        KeyValueRow {
                            key: "color".into(),
                            value: a.color.unwrap_or_default(),
                        },
                        KeyValueRow {
                            key: "is_manual".into(),
                            value: a.is_manual.unwrap_or(false).to_string(),
                        },
                        KeyValueRow {
                            key: "is_hidden".into(),
                            value: a.is_user_hidden.unwrap_or(false).to_string(),
                        },
                        KeyValueRow {
                            key: "is_closed".into(),
                            value: a.is_user_closed.unwrap_or(false).to_string(),
                        },
                    ],
                ),
                None => anyhow::bail!("account not found"),
            }
        }
    }
}

pub(super) fn resolve_account_id(
    client: &CopilotClient,
    account_id: Option<&AccountId>,
    account_name: Option<&str>,
) -> anyhow::Result<Option<AccountId>> {
    if let Some(id) = account_id {
        return Ok(Some(id.clone()));
    }
    let Some(name) = account_name else {
        return Ok(None);
    };
    let want = name.trim().to_lowercase();
    if want.is_empty() {
        anyhow::bail!("empty --account");
    }
    let accounts = client.list_accounts()?;
    let matches: Vec<_> = accounts
        .into_iter()
        .filter(|a| {
            a.name
                .as_deref()
                .map(|n| n.to_lowercase().contains(&want))
                .unwrap_or(false)
        })
        .collect();
    match matches.as_slice() {
        [] => anyhow::bail!("no account matching {:?}", name),
        [one] => Ok(Some(one.id.clone())),
        many => anyhow::bail!(
            "account name {:?} is ambiguous ({} matches); use --account-id instead",
            name,
            many.len()
        ),
    }
}

pub(super) fn account_name_map(
    client: &CopilotClient,
) -> anyhow::Result<std::collections::HashMap<AccountId, String>> {
    let accounts = client.list_accounts()?;
    let mut out = std::collections::HashMap::new();
    for a in accounts {
        out.insert(a.id, a.name.unwrap_or_default());
    }
    Ok(out)
}

#[derive(Debug, Clone, Serialize)]
struct AccountRow {
    id: AccountId,
    name: String,
    account_type: String,
    sub_type: String,
    mask: String,
    balance: String,
}

impl TableRow for AccountRow {
    const HEADERS: &'static [&'static str] = &["id", "name", "type", "sub_type", "mask", "balance"];

    fn cells(&self) -> Vec<Cell> {
        vec![
            Cell::new(shorten_id_for_table(self.id.as_str())),
            Cell::new(&self.name),
            Cell::new(&self.account_type),
            Cell::new(&self.sub_type),
            Cell::new(&self.mask),
            Cell::new(&self.balance),
        ]
    }
}
