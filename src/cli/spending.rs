use comfy_table::Cell;
use serde::Serialize;

use crate::client::CopilotClient;

use super::render::{KeyValueRow, TableRow, render_output};
use super::{Cli, SpendingCmd, value_to_money_string};

pub(super) fn run_spending(
    cli: &Cli,
    client: &CopilotClient,
    cmd: SpendingCmd,
) -> anyhow::Result<()> {
    match cmd {
        SpendingCmd::Monthly => {
            let items = client.list_monthly_spend()?;
            let rows: Vec<MonthlySpendRow> = items
                .into_iter()
                .map(|e| MonthlySpendRow {
                    date: e.date.unwrap_or_default(),
                    total: value_to_money_string(e.total_amount),
                    comparison: value_to_money_string(e.comparison_amount),
                })
                .collect();
            render_output(cli, rows)
        }
        SpendingCmd::Summary => {
            let summary = client.get_transaction_summary(None)?;
            render_output(
                cli,
                vec![
                    KeyValueRow {
                        key: "transactions".into(),
                        value: summary
                            .transactions_count
                            .map(|n| n.to_string())
                            .unwrap_or_default(),
                    },
                    KeyValueRow {
                        key: "total_income".into(),
                        value: value_to_money_string(summary.total_income),
                    },
                    KeyValueRow {
                        key: "total_spent".into(),
                        value: value_to_money_string(summary.total_spent),
                    },
                    KeyValueRow {
                        key: "net_income".into(),
                        value: value_to_money_string(summary.total_net_income),
                    },
                ],
            )
        }
        SpendingCmd::History => {
            let data = client.list_spends(true)?;
            let mut rows: Vec<SpendHistoryRow> = Vec::new();
            if let Some(histories) = data.histories {
                for h in histories {
                    rows.push(SpendHistoryRow {
                        month: h.month.unwrap_or_default(),
                        amount: value_to_money_string(h.amount),
                        comparison: value_to_money_string(h.comparison_amount),
                        unpaid_recurring: value_to_money_string(h.unpaid_recurring_amount),
                    });
                }
            }
            render_output(cli, rows)
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct MonthlySpendRow {
    date: String,
    total: String,
    comparison: String,
}

impl TableRow for MonthlySpendRow {
    const HEADERS: &'static [&'static str] = &["date", "total", "comparison"];

    fn cells(&self) -> Vec<Cell> {
        vec![
            Cell::new(&self.date),
            Cell::new(&self.total),
            Cell::new(&self.comparison),
        ]
    }
}

#[derive(Debug, Clone, Serialize)]
struct SpendHistoryRow {
    month: String,
    amount: String,
    comparison: String,
    unpaid_recurring: String,
}

impl TableRow for SpendHistoryRow {
    const HEADERS: &'static [&'static str] = &["month", "amount", "comparison", "unpaid_recurring"];

    fn cells(&self) -> Vec<Cell> {
        vec![
            Cell::new(&self.month),
            Cell::new(&self.amount),
            Cell::new(&self.comparison),
            Cell::new(&self.unpaid_recurring),
        ]
    }
}
