use comfy_table::Cell;
use serde::Serialize;

use crate::client::CopilotClient;

use super::render::{KeyValueRow, TableRow, render_output};
use super::{Cli, NetworthCmd, value_to_money_string};

pub(super) fn run_networth(
    cli: &Cli,
    client: &CopilotClient,
    cmd: NetworthCmd,
) -> anyhow::Result<()> {
    match cmd {
        NetworthCmd::Current => {
            let entry = client.get_networth_live_balance()?;
            let assets = value_to_money_string(entry.assets);
            let debt = value_to_money_string(entry.debt);

            let assets_f = parse_money_f64(&assets);
            let debt_f = parse_money_f64(&debt);
            let net = assets_f - debt_f;
            let net_str = if net < 0.0 {
                format!("-${:.2}", net.abs())
            } else {
                format!("${:.2}", net)
            };

            render_output(
                cli,
                vec![
                    KeyValueRow {
                        key: "date".into(),
                        value: entry.date.unwrap_or_else(|| "now".into()),
                    },
                    KeyValueRow {
                        key: "assets".into(),
                        value: assets,
                    },
                    KeyValueRow {
                        key: "debt".into(),
                        value: debt,
                    },
                    KeyValueRow {
                        key: "net_worth".into(),
                        value: net_str,
                    },
                ],
            )
        }
        NetworthCmd::History(args) => {
            let entries = client.get_networth(args.time_frame.as_deref())?;
            let rows: Vec<NetworthRow> = entries
                .into_iter()
                .map(|e| {
                    let assets_s = value_to_money_string(e.assets);
                    let debt_s = value_to_money_string(e.debt);
                    let assets_f = parse_money_f64(&assets_s);
                    let debt_f = parse_money_f64(&debt_s);
                    let net = assets_f - debt_f;
                    let net_str = if net < 0.0 {
                        format!("-${:.2}", net.abs())
                    } else {
                        format!("${:.2}", net)
                    };
                    NetworthRow {
                        date: e.date.unwrap_or_default(),
                        assets: assets_s,
                        debt: debt_s,
                        net_worth: net_str,
                    }
                })
                .collect();
            render_output(cli, rows)
        }
    }
}

fn parse_money_f64(s: &str) -> f64 {
    let cleaned = s
        .trim()
        .replace('$', "")
        .replace(',', "");
    cleaned.parse::<f64>().unwrap_or(0.0)
}

#[derive(Debug, Clone, Serialize)]
struct NetworthRow {
    date: String,
    assets: String,
    debt: String,
    net_worth: String,
}

impl TableRow for NetworthRow {
    const HEADERS: &'static [&'static str] = &["date", "assets", "debt", "net_worth"];

    fn cells(&self) -> Vec<Cell> {
        vec![
            Cell::new(&self.date),
            Cell::new(&self.assets),
            Cell::new(&self.debt),
            Cell::new(&self.net_worth),
        ]
    }
}
