use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::client::{ClientMode, CopilotClient, Transaction, TransactionIdRef};
use crate::config::{load_token, session_path, token_path};
use crate::types::{CategoryId, TagId, TransactionId};

// ---------------------------------------------------------------------------
// JSON-RPC 2.0 types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    #[allow(dead_code)]
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Option<Value>,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i64,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

impl JsonRpcResponse {
    fn success(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: Some(result),
            error: None,
        }
    }

    fn error(id: Value, code: i64, message: String) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message,
                data: None,
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// MCP tool definitions
// ---------------------------------------------------------------------------

fn tool_definitions() -> Value {
    json!({
        "tools": [
            {
                "name": "list_transactions",
                "description": "List transactions from Copilot Money with optional filters. Returns transaction data including date, name, amount, category, tags, and review status.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "limit": {
                            "type": "integer",
                            "description": "Maximum number of transactions to return (default 25)",
                            "default": 25
                        },
                        "reviewed": {
                            "type": "boolean",
                            "description": "If true, only return reviewed transactions"
                        },
                        "unreviewed": {
                            "type": "boolean",
                            "description": "If true, only return unreviewed transactions"
                        },
                        "category": {
                            "type": "string",
                            "description": "Filter by category name (case-insensitive exact match)"
                        },
                        "date": {
                            "type": "string",
                            "description": "Filter by date (YYYY-MM-DD format)"
                        },
                        "name_contains": {
                            "type": "string",
                            "description": "Filter by merchant/name substring (case-insensitive)"
                        },
                        "tag": {
                            "type": "string",
                            "description": "Filter by tag name"
                        }
                    }
                }
            },
            {
                "name": "search_transactions",
                "description": "Search transactions by merchant/name text. Returns matching transactions.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Search text to match against transaction merchant/name"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum number of results (default 50)",
                            "default": 50
                        }
                    },
                    "required": ["query"]
                }
            },
            {
                "name": "show_transaction",
                "description": "Get full details for a specific transaction by ID.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "id": {
                            "type": "string",
                            "description": "Transaction ID"
                        }
                    },
                    "required": ["id"]
                }
            },
            {
                "name": "review_transactions",
                "description": "Mark one or more transactions as reviewed.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "ids": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Transaction IDs to mark as reviewed"
                        }
                    },
                    "required": ["ids"]
                }
            },
            {
                "name": "unreview_transactions",
                "description": "Mark one or more transactions as unreviewed.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "ids": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Transaction IDs to mark as unreviewed"
                        }
                    },
                    "required": ["ids"]
                }
            },
            {
                "name": "set_transaction_category",
                "description": "Set the category for one or more transactions by category name or ID.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "ids": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Transaction IDs to update"
                        },
                        "category": {
                            "type": "string",
                            "description": "Category name (case-insensitive)"
                        },
                        "category_id": {
                            "type": "string",
                            "description": "Category ID (use instead of name for precision)"
                        }
                    },
                    "required": ["ids"]
                }
            },
            {
                "name": "set_transaction_notes",
                "description": "Set or clear user notes on one or more transactions.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "ids": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Transaction IDs to update"
                        },
                        "notes": {
                            "type": "string",
                            "description": "Notes text to set (omit to clear)"
                        }
                    },
                    "required": ["ids"]
                }
            },
            {
                "name": "set_transaction_tags",
                "description": "Set, add, or remove tags on one or more transactions.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "ids": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Transaction IDs to update"
                        },
                        "tag_ids": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Tag IDs to set/add/remove"
                        },
                        "mode": {
                            "type": "string",
                            "enum": ["set", "add", "remove"],
                            "description": "How to apply tags: 'set' replaces all, 'add' appends, 'remove' removes (default: set)",
                            "default": "set"
                        }
                    },
                    "required": ["ids", "tag_ids"]
                }
            },
            {
                "name": "list_categories",
                "description": "List all spending categories from Copilot Money, including child/nested categories.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "include_children": {
                            "type": "boolean",
                            "description": "Include nested child categories (default true)",
                            "default": true
                        }
                    }
                }
            },
            {
                "name": "list_tags",
                "description": "List all tags from Copilot Money.",
                "inputSchema": {
                    "type": "object",
                    "properties": {}
                }
            },
            {
                "name": "create_tag",
                "description": "Create a new tag in Copilot Money.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "Tag name"
                        },
                        "color_name": {
                            "type": "string",
                            "description": "Optional color name"
                        }
                    },
                    "required": ["name"]
                }
            },
            {
                "name": "delete_tag",
                "description": "Delete a tag from Copilot Money.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "id": {
                            "type": "string",
                            "description": "Tag ID to delete"
                        }
                    },
                    "required": ["id"]
                }
            },
            {
                "name": "list_recurrings",
                "description": "List all recurring expense/income rules from Copilot Money.",
                "inputSchema": {
                    "type": "object",
                    "properties": {}
                }
            },
            {
                "name": "budget_month",
                "description": "Show monthly budget history from Copilot Money.",
                "inputSchema": {
                    "type": "object",
                    "properties": {}
                }
            },
            {
                "name": "list_accounts",
                "description": "List connected financial accounts from Copilot Money (banks, credit cards, investments, etc).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "include_hidden": {
                            "type": "boolean",
                            "description": "Include hidden accounts (default false)",
                            "default": false
                        }
                    }
                }
            },
            {
                "name": "networth",
                "description": "Get current net worth (assets, debt, total) from Copilot Money.",
                "inputSchema": {
                    "type": "object",
                    "properties": {}
                }
            },
            {
                "name": "spending_summary",
                "description": "Get a spending summary with total income, total spent, and net income.",
                "inputSchema": {
                    "type": "object",
                    "properties": {}
                }
            },
            {
                "name": "monthly_spending",
                "description": "Get monthly spending totals with month-over-month comparison.",
                "inputSchema": {
                    "type": "object",
                    "properties": {}
                }
            },
            {
                "name": "upcoming_recurrings",
                "description": "List upcoming unpaid recurring payments and bills.",
                "inputSchema": {
                    "type": "object",
                    "properties": {}
                }
            }
        ]
    })
}

// ---------------------------------------------------------------------------
// MCP server
// ---------------------------------------------------------------------------

pub struct McpServer {
    client: CopilotClient,
}

impl McpServer {
    fn new(client: CopilotClient) -> Self {
        Self { client }
    }

    fn handle_request(&self, req: &JsonRpcRequest) -> Option<JsonRpcResponse> {
        let id = req.id.clone().unwrap_or(Value::Null);

        // Notifications (no id) don't get a response
        req.id.as_ref()?;

        let result = match req.method.as_str() {
            "initialize" => self.handle_initialize(),
            "tools/list" => self.handle_tools_list(),
            "tools/call" => self.handle_tools_call(req.params.as_ref()),
            "ping" => Ok(json!({})),
            _ => Err((-32601, format!("method not found: {}", req.method))),
        };

        Some(match result {
            Ok(val) => JsonRpcResponse::success(id, val),
            Err((code, msg)) => JsonRpcResponse::error(id, code, msg),
        })
    }

    fn handle_initialize(&self) -> Result<Value, (i64, String)> {
        Ok(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": "copilot-money",
                "version": env!("CARGO_PKG_VERSION")
            }
        }))
    }

    fn handle_tools_list(&self) -> Result<Value, (i64, String)> {
        Ok(tool_definitions())
    }

    fn handle_tools_call(&self, params: Option<&Value>) -> Result<Value, (i64, String)> {
        let params = params.ok_or((-32602, "missing params".into()))?;
        let name = params
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or((-32602, "missing tool name".into()))?;
        let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

        let result = match name {
            "list_transactions" => self.tool_list_transactions(&arguments),
            "search_transactions" => self.tool_search_transactions(&arguments),
            "show_transaction" => self.tool_show_transaction(&arguments),
            "review_transactions" => self.tool_review_transactions(&arguments),
            "unreview_transactions" => self.tool_unreview_transactions(&arguments),
            "set_transaction_category" => self.tool_set_transaction_category(&arguments),
            "set_transaction_notes" => self.tool_set_transaction_notes(&arguments),
            "set_transaction_tags" => self.tool_set_transaction_tags(&arguments),
            "list_categories" => self.tool_list_categories(&arguments),
            "list_tags" => self.tool_list_tags(),
            "create_tag" => self.tool_create_tag(&arguments),
            "delete_tag" => self.tool_delete_tag(&arguments),
            "list_recurrings" => self.tool_list_recurrings(),
            "budget_month" => self.tool_budget_month(),
            "list_accounts" => self.tool_list_accounts(&arguments),
            "networth" => self.tool_networth(),
            "spending_summary" => self.tool_spending_summary(),
            "monthly_spending" => self.tool_monthly_spending(),
            "upcoming_recurrings" => self.tool_upcoming_recurrings(),
            _ => Err(format!("unknown tool: {name}")),
        };

        match result {
            Ok(content) => Ok(json!({
                "content": [{
                    "type": "text",
                    "text": serde_json::to_string_pretty(&content).unwrap_or_default()
                }]
            })),
            Err(msg) => Ok(json!({
                "content": [{
                    "type": "text",
                    "text": msg
                }],
                "isError": true
            })),
        }
    }

    // -- Tool implementations -----------------------------------------------

    fn tool_list_transactions(&self, args: &Value) -> Result<Value, String> {
        let limit = args
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(25) as usize;

        let reviewed = args.get("reviewed").and_then(|v| v.as_bool()).unwrap_or(false);
        let unreviewed = args.get("unreviewed").and_then(|v| v.as_bool()).unwrap_or(false);

        let filter = if reviewed {
            Some(json!({ "isReviewed": true }))
        } else if unreviewed {
            Some(json!({ "isReviewed": false }))
        } else {
            None
        };

        let page = self
            .client
            .list_transactions_page(limit, None, filter, None)
            .map_err(|e| e.to_string())?;

        let cat_map = self.category_name_map();
        let mut txns = page.transactions;

        // Client-side filters
        if let Some(name_contains) = args.get("name_contains").and_then(|v| v.as_str()) {
            let q = name_contains.to_lowercase();
            txns.retain(|t| {
                t.name
                    .as_deref()
                    .map(|n| n.to_lowercase().contains(&q))
                    .unwrap_or(false)
            });
        }

        if let Some(date) = args.get("date").and_then(|v| v.as_str()) {
            txns.retain(|t| t.date.as_deref() == Some(date));
        }

        if let Some(category) = args.get("category").and_then(|v| v.as_str()) {
            let want = category.to_lowercase();
            txns.retain(|t| {
                t.category_id
                    .as_ref()
                    .and_then(|cid| cat_map.get(cid))
                    .map(|name| name.to_lowercase() == want)
                    .unwrap_or(false)
            });
        }

        if let Some(tag_name) = args.get("tag").and_then(|v| v.as_str()) {
            let want = tag_name.to_lowercase();
            txns.retain(|t| {
                t.tags
                    .as_ref()
                    .map(|tags| {
                        tags.iter().any(|tag| {
                            tag.name
                                .as_deref()
                                .map(|n| n.to_lowercase() == want)
                                .unwrap_or(false)
                        })
                    })
                    .unwrap_or(false)
            });
        }

        Ok(json!(self.enrich_transactions(&txns, &cat_map)))
    }

    fn tool_search_transactions(&self, args: &Value) -> Result<Value, String> {
        let query = args
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or("missing required parameter: query")?;
        let limit = args
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(50) as usize;

        let page = self
            .client
            .list_transactions_page(limit, None, None, None)
            .map_err(|e| e.to_string())?;

        let q = query.to_lowercase();
        let filtered: Vec<_> = page
            .transactions
            .into_iter()
            .filter(|t| {
                t.name
                    .as_deref()
                    .map(|n| n.to_lowercase().contains(&q))
                    .unwrap_or(false)
            })
            .collect();

        let cat_map = self.category_name_map();
        Ok(json!(self.enrich_transactions(&filtered, &cat_map)))
    }

    fn tool_show_transaction(&self, args: &Value) -> Result<Value, String> {
        let id = args
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or("missing required parameter: id")?;

        let txns = self
            .client
            .list_transactions(200)
            .map_err(|e| e.to_string())?;

        let target_id: TransactionId = id.into();
        let txn = txns
            .into_iter()
            .find(|t| t.id == target_id)
            .ok_or_else(|| format!("transaction {id} not found"))?;

        let cat_map = self.category_name_map();
        let enriched = self.enrich_transactions(&[txn], &cat_map);
        Ok(enriched.into_iter().next().unwrap_or(json!(null)))
    }

    fn tool_review_transactions(&self, args: &Value) -> Result<Value, String> {
        let ids = self.parse_transaction_ids(args)?;
        let id_refs = self.resolve_transaction_refs(&ids)?;

        let result = self
            .client
            .bulk_edit_transactions_reviewed(id_refs, true)
            .map_err(|e| e.to_string())?;

        Ok(json!({
            "updated": result.updated.len(),
            "failed": result.failed.len()
        }))
    }

    fn tool_unreview_transactions(&self, args: &Value) -> Result<Value, String> {
        let ids = self.parse_transaction_ids(args)?;
        let id_refs = self.resolve_transaction_refs(&ids)?;

        let result = self
            .client
            .bulk_edit_transactions_reviewed(id_refs, false)
            .map_err(|e| e.to_string())?;

        Ok(json!({
            "updated": result.updated.len(),
            "failed": result.failed.len()
        }))
    }

    fn tool_set_transaction_category(&self, args: &Value) -> Result<Value, String> {
        let ids = self.parse_transaction_ids(args)?;

        let category_id = if let Some(cid) = args.get("category_id").and_then(|v| v.as_str()) {
            CategoryId::from(cid)
        } else if let Some(name) = args.get("category").and_then(|v| v.as_str()) {
            self.resolve_category_by_name(name)?
        } else {
            return Err("must provide either 'category' or 'category_id'".into());
        };

        let txns = self
            .client
            .list_transactions(500)
            .map_err(|e| e.to_string())?;

        let mut updated = 0;
        for target_id in &ids {
            let txn = txns
                .iter()
                .find(|t| t.id == *target_id)
                .ok_or_else(|| format!("transaction {} not found", target_id))?;
            let item_id = txn
                .item_id
                .as_ref()
                .ok_or("transaction missing itemId")?;
            let account_id = txn
                .account_id
                .as_ref()
                .ok_or("transaction missing accountId")?;

            self.client
                .edit_transaction(
                    item_id,
                    account_id,
                    target_id,
                    json!({ "categoryId": category_id.as_str() }),
                )
                .map_err(|e| e.to_string())?;
            updated += 1;
        }

        Ok(json!({ "updated": updated }))
    }

    fn tool_set_transaction_notes(&self, args: &Value) -> Result<Value, String> {
        let ids = self.parse_transaction_ids(args)?;
        let notes = args.get("notes").and_then(|v| v.as_str());

        let input = match notes {
            Some(n) => json!({ "userNotes": n }),
            None => json!({ "userNotes": null }),
        };

        let txns = self
            .client
            .list_transactions(500)
            .map_err(|e| e.to_string())?;

        let mut updated = 0;
        for target_id in &ids {
            let txn = txns
                .iter()
                .find(|t| t.id == *target_id)
                .ok_or_else(|| format!("transaction {} not found", target_id))?;
            let item_id = txn
                .item_id
                .as_ref()
                .ok_or("transaction missing itemId")?;
            let account_id = txn
                .account_id
                .as_ref()
                .ok_or("transaction missing accountId")?;

            self.client
                .edit_transaction(item_id, account_id, target_id, input.clone())
                .map_err(|e| e.to_string())?;
            updated += 1;
        }

        Ok(json!({ "updated": updated }))
    }

    fn tool_set_transaction_tags(&self, args: &Value) -> Result<Value, String> {
        let ids = self.parse_transaction_ids(args)?;
        let tag_ids: Vec<TagId> = args
            .get("tag_ids")
            .and_then(|v| v.as_array())
            .ok_or("missing required parameter: tag_ids")?
            .iter()
            .filter_map(|v| v.as_str().map(TagId::from))
            .collect();

        let mode = args
            .get("mode")
            .and_then(|v| v.as_str())
            .unwrap_or("set");

        let txns = self
            .client
            .list_transactions(500)
            .map_err(|e| e.to_string())?;

        let mut updated = 0;
        for target_id in &ids {
            let txn = txns
                .iter()
                .find(|t| t.id == *target_id)
                .ok_or_else(|| format!("transaction {} not found", target_id))?;
            let item_id = txn
                .item_id
                .as_ref()
                .ok_or("transaction missing itemId")?;
            let account_id = txn
                .account_id
                .as_ref()
                .ok_or("transaction missing accountId")?;

            let final_tag_ids: Vec<String> = match mode {
                "add" => {
                    let mut existing: Vec<String> = txn
                        .tags
                        .as_ref()
                        .map(|ts| ts.iter().map(|t| t.id.as_str().to_string()).collect())
                        .unwrap_or_default();
                    for tid in &tag_ids {
                        let s = tid.as_str().to_string();
                        if !existing.contains(&s) {
                            existing.push(s);
                        }
                    }
                    existing
                }
                "remove" => {
                    let remove_set: Vec<&str> = tag_ids.iter().map(|t| t.as_str()).collect();
                    txn.tags
                        .as_ref()
                        .map(|ts| {
                            ts.iter()
                                .filter(|t| !remove_set.contains(&t.id.as_str()))
                                .map(|t| t.id.as_str().to_string())
                                .collect()
                        })
                        .unwrap_or_default()
                }
                _ => tag_ids.iter().map(|t| t.as_str().to_string()).collect(),
            };

            self.client
                .edit_transaction(
                    item_id,
                    account_id,
                    target_id,
                    json!({ "tagIds": final_tag_ids }),
                )
                .map_err(|e| e.to_string())?;
            updated += 1;
        }

        Ok(json!({ "updated": updated }))
    }

    fn tool_list_categories(&self, args: &Value) -> Result<Value, String> {
        let categories = self
            .client
            .list_categories(false, false, false)
            .map_err(|e| e.to_string())?;

        let include_children = args
            .get("include_children")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        fn serialize_cat(cat: &crate::client::Category, include_children: bool) -> Value {
            let mut obj = json!({
                "id": cat.id.as_str(),
                "name": cat.name,
            });
            if let Some(color) = &cat.color_name {
                obj["color_name"] = json!(color);
            }
            if let Some(crate::client::Icon::EmojiUnicode {
                unicode: Some(u),
            }) = &cat.icon
            {
                obj["emoji"] = json!(u);
            }
            if include_children
                && let Some(children) = &cat.child_categories
            {
                let kids: Vec<Value> = children
                    .iter()
                    .map(|c| serialize_cat(c, true))
                    .collect();
                if !kids.is_empty() {
                    obj["children"] = json!(kids);
                }
            }
            obj
        }

        let result: Vec<Value> = categories
            .iter()
            .map(|c| serialize_cat(c, include_children))
            .collect();
        Ok(json!(result))
    }

    fn tool_list_tags(&self) -> Result<Value, String> {
        let tags = self.client.list_tags().map_err(|e| e.to_string())?;
        let result: Vec<Value> = tags
            .iter()
            .map(|t| {
                json!({
                    "id": t.id.as_str(),
                    "name": t.name,
                    "color_name": t.color_name,
                })
            })
            .collect();
        Ok(json!(result))
    }

    fn tool_create_tag(&self, args: &Value) -> Result<Value, String> {
        let name = args
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or("missing required parameter: name")?;
        let color_name = args.get("color_name").and_then(|v| v.as_str());

        let tag = self
            .client
            .create_tag(name, color_name)
            .map_err(|e| e.to_string())?;

        Ok(json!({
            "id": tag.id.as_str(),
            "name": tag.name,
            "color_name": tag.color_name,
        }))
    }

    fn tool_delete_tag(&self, args: &Value) -> Result<Value, String> {
        let id = args
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or("missing required parameter: id")?;
        let tag_id: TagId = id.into();

        let ok = self
            .client
            .delete_tag(&tag_id)
            .map_err(|e| e.to_string())?;

        Ok(json!({ "deleted": ok }))
    }

    fn tool_list_recurrings(&self) -> Result<Value, String> {
        let recurrings = self
            .client
            .list_recurrings()
            .map_err(|e| e.to_string())?;

        let result: Vec<Value> = recurrings
            .iter()
            .map(|r| {
                json!({
                    "id": r.id.as_str(),
                    "name": r.name,
                    "frequency": r.frequency.map(|f| f.to_string()),
                    "category_id": r.category_id.as_ref().map(|c| c.as_str().to_string()),
                })
            })
            .collect();
        Ok(json!(result))
    }

    fn tool_budget_month(&self) -> Result<Value, String> {
        let months = self
            .client
            .list_budget_months()
            .map_err(|e| e.to_string())?;

        let result: Vec<Value> = months
            .iter()
            .map(|m| {
                json!({
                    "month": m.month,
                    "amount": m.amount,
                })
            })
            .collect();
        Ok(json!(result))
    }

    fn tool_list_accounts(&self, args: &Value) -> Result<Value, String> {
        let include_hidden = args
            .get("include_hidden")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let mut accounts = self
            .client
            .list_accounts()
            .map_err(|e| e.to_string())?;

        if !include_hidden {
            accounts.retain(|a| !a.is_user_hidden.unwrap_or(false) && !a.is_user_closed.unwrap_or(false));
        }

        let result: Vec<Value> = accounts
            .iter()
            .map(|a| {
                json!({
                    "id": a.id.as_str(),
                    "name": a.name,
                    "type": a.account_type,
                    "sub_type": a.sub_type,
                    "mask": a.mask,
                    "balance": a.balance,
                    "is_manual": a.is_manual,
                    "color": a.color,
                })
            })
            .collect();
        Ok(json!(result))
    }

    fn tool_networth(&self) -> Result<Value, String> {
        let entry = self
            .client
            .get_networth_live_balance()
            .map_err(|e| e.to_string())?;
        Ok(json!({
            "date": entry.date,
            "assets": entry.assets,
            "debt": entry.debt,
        }))
    }

    fn tool_spending_summary(&self) -> Result<Value, String> {
        let summary = self
            .client
            .get_transaction_summary(None)
            .map_err(|e| e.to_string())?;
        Ok(json!({
            "transactions_count": summary.transactions_count,
            "total_income": summary.total_income,
            "total_spent": summary.total_spent,
            "total_net_income": summary.total_net_income,
        }))
    }

    fn tool_monthly_spending(&self) -> Result<Value, String> {
        let items = self
            .client
            .list_monthly_spend()
            .map_err(|e| e.to_string())?;
        let result: Vec<Value> = items
            .iter()
            .map(|m| {
                json!({
                    "date": m.date,
                    "total_amount": m.total_amount,
                    "comparison_amount": m.comparison_amount,
                })
            })
            .collect();
        Ok(json!(result))
    }

    fn tool_upcoming_recurrings(&self) -> Result<Value, String> {
        let items = self
            .client
            .list_upcoming_recurrings()
            .map_err(|e| e.to_string())?;
        let result: Vec<Value> = items
            .iter()
            .map(|r| {
                json!({
                    "id": r.id.as_str(),
                    "name": r.name,
                    "frequency": r.frequency.map(|f| f.to_string()),
                    "next_payment_date": r.next_payment_date,
                    "next_payment_amount": r.next_payment_amount,
                    "state": r.state,
                })
            })
            .collect();
        Ok(json!(result))
    }

    // -- Helpers ------------------------------------------------------------

    fn category_name_map(&self) -> HashMap<CategoryId, String> {
        let categories = self.client.list_categories(false, false, false).unwrap_or_default();
        let mut out = HashMap::new();
        fn walk(out: &mut HashMap<CategoryId, String>, cats: &[crate::client::Category]) {
            for c in cats {
                out.insert(c.id.clone(), c.name.clone().unwrap_or_default());
                if let Some(children) = c.child_categories.as_ref() {
                    walk(out, children);
                }
            }
        }
        walk(&mut out, &categories);
        out
    }

    fn enrich_transactions(
        &self,
        txns: &[Transaction],
        cat_map: &HashMap<CategoryId, String>,
    ) -> Vec<Value> {
        txns.iter()
            .map(|t| {
                let cat_name = t
                    .category_id
                    .as_ref()
                    .and_then(|cid| cat_map.get(cid))
                    .cloned();
                let tags: Vec<String> = t
                    .tags
                    .as_ref()
                    .map(|ts| {
                        ts.iter()
                            .filter_map(|tag| tag.name.clone())
                            .collect()
                    })
                    .unwrap_or_default();

                json!({
                    "id": t.id.as_str(),
                    "date": t.date,
                    "name": t.name,
                    "amount": t.amount,
                    "is_reviewed": t.is_reviewed,
                    "category_id": t.category_id.as_ref().map(|c| c.as_str().to_string()),
                    "category_name": cat_name,
                    "tags": tags,
                    "type": t.txn_type.map(|tt| tt.to_string()),
                    "user_notes": t.user_notes,
                    "item_id": t.item_id.as_ref().map(|i| i.as_str().to_string()),
                    "account_id": t.account_id.as_ref().map(|a| a.as_str().to_string()),
                    "recurring_id": t.recurring_id.as_ref().map(|r| r.as_str().to_string()),
                })
            })
            .collect()
    }

    fn parse_transaction_ids(&self, args: &Value) -> Result<Vec<TransactionId>, String> {
        let ids = args
            .get("ids")
            .and_then(|v| v.as_array())
            .ok_or("missing required parameter: ids")?;

        let result: Vec<TransactionId> = ids
            .iter()
            .filter_map(|v| v.as_str().map(TransactionId::from))
            .collect();

        if result.is_empty() {
            return Err("ids must be a non-empty array of strings".into());
        }
        Ok(result)
    }

    fn resolve_transaction_refs(
        &self,
        ids: &[TransactionId],
    ) -> Result<Vec<TransactionIdRef>, String> {
        let txns = self
            .client
            .list_transactions(500)
            .map_err(|e| e.to_string())?;

        let mut refs = Vec::new();
        for target_id in ids {
            let txn = txns
                .iter()
                .find(|t| t.id == *target_id)
                .ok_or_else(|| format!("transaction {} not found", target_id))?;
            refs.push(TransactionIdRef {
                account_id: txn
                    .account_id
                    .clone()
                    .ok_or("transaction missing accountId")?,
                id: txn.id.clone(),
                item_id: txn.item_id.clone().ok_or("transaction missing itemId")?,
            });
        }
        Ok(refs)
    }

    fn resolve_category_by_name(&self, name: &str) -> Result<CategoryId, String> {
        let want = name.trim().to_lowercase();
        if want.is_empty() {
            return Err("empty category name".into());
        }
        let categories = self
            .client
            .list_categories(false, false, false)
            .map_err(|e| e.to_string())?;

        let mut matches = Vec::new();
        fn walk(
            out: &mut Vec<(CategoryId, String)>,
            cats: &[crate::client::Category],
        ) {
            for c in cats {
                out.push((c.id.clone(), c.name.clone().unwrap_or_default()));
                if let Some(children) = c.child_categories.as_ref() {
                    walk(out, children);
                }
            }
        }
        let mut all = Vec::new();
        walk(&mut all, &categories);

        for (id, cat_name) in &all {
            if cat_name.to_lowercase() == want {
                matches.push(id.clone());
            }
        }

        match matches.len() {
            0 => Err(format!("no category named {:?}", name)),
            1 => Ok(matches.into_iter().next().unwrap()),
            n => Err(format!(
                "category name {:?} is ambiguous ({n} matches); use category_id instead",
                name
            )),
        }
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

pub fn run_mcp(
    token: Option<String>,
    token_file: Option<PathBuf>,
    session_dir: Option<PathBuf>,
    base_url: String,
    fixtures_dir: Option<PathBuf>,
) -> anyhow::Result<()> {
    let token_file_path = token_file.unwrap_or_else(token_path);
    let resolved_token = token.or_else(|| load_token(&token_file_path).ok());

    let mode = match fixtures_dir {
        Some(dir) => ClientMode::Fixtures(dir),
        None => ClientMode::Http {
            base_url,
            token: resolved_token,
            token_file: token_file_path,
            session_dir: session_dir.or_else(|| {
                let sp = session_path();
                sp.exists().then_some(sp)
            }),
            auto_login: false, // MCP is non-interactive; never launch a browser
        },
    };

    let client = CopilotClient::new(mode);
    let server = McpServer::new(client);

    let stdin = io::stdin().lock();
    let mut stdout = io::stdout().lock();

    for line in stdin.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        let req: JsonRpcRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let resp = JsonRpcResponse::error(
                    Value::Null,
                    -32700,
                    format!("parse error: {e}"),
                );
                let out = serde_json::to_string(&resp)?;
                writeln!(stdout, "{out}")?;
                stdout.flush()?;
                continue;
            }
        };

        if let Some(resp) = server.handle_request(&req) {
            let out = serde_json::to_string(&resp)?;
            writeln!(stdout, "{out}")?;
            stdout.flush()?;
        }
    }

    Ok(())
}
