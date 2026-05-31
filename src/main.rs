// ------------------------------------------------------------------------- / prasanth rangan //
// sqllineage
// a simple cli tool to analyze your sql and generate lineage


// ------------------------------------------------------------------------- / imports

use sqlparser::{
    ast::{Statement, Query, SetExpr, Select, TableWithJoins, TableFactor, FromTable},
    dialect::TeradataDialect,
    parser::Parser,
};

use anyhow::Result;
use regex::Regex;
use std::{
    collections::{HashMap, HashSet},
    env, fmt, fs,
    sync::OnceLock,
};


// ------------------------------------------------------------------------- / datatypes

enum StatementType {
    Select,
    Insert,
    Update,
    Delete,
    View,
    Other,
}

struct LineageNode {
    label: String,
    children: Vec<LineageNode>,
}


// ------------------------------------------------------------------------- / implementations

impl LineageNode {
    fn new(label: impl Into<String>) -> Self {
        LineageNode {
            label: label.into(),
            children: Vec::new(),
        }
    }
}

impl fmt::Display for LineageNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}", self.label)?;
        print_children(f, &self.children, &mut vec![])
    }
}

impl fmt::Display for StatementType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StatementType::Select => write!(f, "Select"),
            StatementType::Insert => write!(f, "Insert"),
            StatementType::Update => write!(f, "Update"),
            StatementType::Delete => write!(f, "Delete"),
            StatementType::View   => write!(f, "View"),
            StatementType::Other  => write!(f, "Other"),
        }
    }
}


// ------------------------------------------------------------------------- / preprocessing functions

fn statement_type(statement: &Statement) -> StatementType {
    match statement {
        Statement::Query(_)        => StatementType::Select,
        Statement::Insert(_)       => StatementType::Insert,
        Statement::Update(_)       => StatementType::Update,
        Statement::Delete(_)       => StatementType::Delete,
        Statement::CreateView {..} => StatementType::View,
        _                          => StatementType::Other,
    }
}

fn regex_replacements() -> &'static [(Regex, &'static str)] {
    static REGEXES: OnceLock<Vec<(Regex, &str)>> = OnceLock::new();
    REGEXES.get_or_init(|| {
        vec![
            (Regex::new(r"(?i)\bSEL\b").unwrap(), "SELECT"),
            (Regex::new(r"(?i)\bINS\b").unwrap(), "INSERT"),
            (Regex::new(r"(?i)\bUPD\b").unwrap(), "UPDATE"),
            (Regex::new(r"(?i)\bDEL\b").unwrap(), "DELETE"),
            (Regex::new(r"(?i)^\s*REPLACE\s+VIEW\s+").unwrap(), "CREATE VIEW "),
        ]
    })
}

fn normalize_teradata(sql_in: &str) -> String {
    let mut sql_td = sql_in.to_string();
    for (pattern, replace) in regex_replacements() {
        sql_td = pattern.replace_all(&sql_td, *replace).to_string();
    }

    let cleanup_re = Regex::new(r"(?is)(.*?)\bAS\s+\blocking\b\s+.*?\bfor\b\s+.*?\bselect\b(.*)").unwrap();
    sql_td = cleanup_re.replace_all(&sql_td, |caps: &regex::Captures| {
        let before_as = &caps[1];
        let after_select = &caps[2];
        format!("{} AS SELECT{}", before_as, after_select)
    }).to_string();

    sql_td
}


// ------------------------------------------------------------------------- / lineage (flat extraction)

fn extract_table(twj: &TableWithJoins) -> Option<String> {
    match &twj.relation {
        TableFactor::Table { name, .. } => Some(name.to_string()),
        _ => None,
    }
}

fn extract_delete(from: &FromTable) -> Option<String> {
    match from {
        FromTable::WithFromKeyword(twj_list) => {
            twj_list.first().and_then(|twj| {
                match &twj.relation {
                    TableFactor::Table { name, .. } => Some(name.to_string()),
                    _ => None,
                }
            })
        }
        _ => None,
    }
}

fn extract_statement(statement: &Statement) -> (Vec<String>, Option<String>) {
    match statement {
        Statement::Query(query) => {
            let sources = extract_query(query);
            (sources, None)
        }
        Statement::Insert(insert) => {
            let target = Some(insert.table.to_string());
            let sources = match &insert.source {
                Some(query) => extract_query(query),
                None => vec![],
            };
            (sources, target)
        }
        Statement::Update(update) => {
            let target = extract_table(&update.table);
            let mut sources = vec![target.clone().unwrap_or_default()];
            if let Some(from_kind) = &update.from {
                if let sqlparser::ast::UpdateTableFromKind::AfterSet(twj_list) = from_kind {
                    for twj in twj_list {
                        sources.extend(extract_table_with_joins(twj));
                    }
                }
            }
            (sources, target)
        }
        Statement::Delete(delete) => {
            let target = extract_delete(&delete.from);
            let mut sources = vec![target.clone().unwrap_or_default()];
            if let Some(using_list) = &delete.using {
                for twj in using_list {
                    sources.extend(extract_table_with_joins(twj));
                }
            }
            (sources, target)
        }
        Statement::CreateView(create_view) => {
            let target = Some(create_view.name.to_string());
            let sources = extract_query(&create_view.query);
            (sources, target)
        }
        _ => (vec![], None),
    }
}

fn extract_query(query: &Query) -> Vec<String> {
    let mut tables = Vec::new();
    if let Some(with) = &query.with {
        for cte in &with.cte_tables {
            tables.extend(extract_query(&cte.query));
        }
    }
    tables.extend(extract_set_expr(query.body.as_ref()));
    tables
}

fn extract_set_expr(expr: &SetExpr) -> Vec<String> {
    match expr {
        SetExpr::Select(select) => extract_select(select),
        SetExpr::Query(query) => extract_query(query.as_ref()),
        SetExpr::SetOperation { left, right, .. } => {
            let mut tables = extract_set_expr(left.as_ref());
            tables.extend(extract_set_expr(right.as_ref()));
            tables
        }
        _ => vec![],
    }
}

fn extract_select(select: &Select) -> Vec<String> {
    let mut tables = Vec::new();
    for twj in &select.from {
        tables.extend(extract_table_with_joins(twj));
    }
    tables
}

fn extract_table_with_joins(twj: &TableWithJoins) -> Vec<String> {
    let mut tables = Vec::new();
    extract_table_factor(&twj.relation, &mut tables);
    for join in &twj.joins {
        extract_table_factor(&join.relation, &mut tables);
    }
    tables
}

fn extract_table_factor(factor: &TableFactor, tables: &mut Vec<String>) {
    match factor {
        TableFactor::Table { name, .. } => {
            tables.push(name.to_string());
        }
        TableFactor::Derived { subquery, .. } => {
            tables.extend(extract_query(subquery));
        }
        _ => {}
    }
}


// ------------------------------------------------------------------------- / lineage (tree)

fn print_children(
    f: &mut fmt::Formatter<'_>,
    children: &[LineageNode],
    prefix: &mut Vec<bool>,
) -> fmt::Result {
    let count = children.len();
    for (i, child) in children.iter().enumerate() {
        let is_last = i == count - 1;
        let connector = if is_last { "└──" } else { "├──" };

        let indent: String = prefix
            .iter()
            .map(|&continue_branch| {
                if continue_branch { "│   " } else { "    " }
            })
            .collect();

        writeln!(f, "{}{} {}", indent, connector, child.label)?;

        prefix.push(!is_last);
        print_children(f, &child.children, prefix)?;
        prefix.pop();
    }
    Ok(())
}

fn build_cte_map(query: &Query) -> HashMap<String, &Query> {
    let mut map = HashMap::new();
    if let Some(with) = &query.with {
        for cte in &with.cte_tables {
            map.insert(cte.alias.name.to_string(), cte.query.as_ref());
        }
    }
    map
}

fn build_statement_lineage_node(statement: &Statement) -> Option<LineageNode> {
    let mut visited_ctes = HashSet::new();
    build_statement_lineage_node_with_visited(statement, &mut visited_ctes)
}

fn build_statement_lineage_node_with_visited(
    statement: &Statement,
    visited_ctes: &mut HashSet<String>,
) -> Option<LineageNode> {
    match statement {
        Statement::Query(query) => {
            let cte_map = build_cte_map(query);
            let mut root = LineageNode::new("SELECT");
            root.children.push(build_set_expr_tree(query.body.as_ref(), &cte_map, visited_ctes));
            Some(root)
        }
        Statement::Insert(insert) => {
            let cte_map = insert.source.as_ref().map(|s| build_cte_map(s.as_ref())).unwrap_or_default();
            let mut root = LineageNode::new(format!("INSERT INTO {}", insert.table));
            if let Some(source) = &insert.source {
                root.children.push(build_set_expr_tree(source.body.as_ref(), &cte_map, visited_ctes));
            }
            Some(root)
        }
        Statement::Update(update) => {
            let target = extract_table(&update.table).unwrap_or_default();
            let mut root = LineageNode::new(format!("UPDATE {}", target));
            root.children.push(LineageNode::new("(self)"));
            if let Some(from_kind) = &update.from {
                if let sqlparser::ast::UpdateTableFromKind::AfterSet(twj_list) = from_kind {
                    for twj in twj_list {
                        for table in extract_table_with_joins(twj) {
                            root.children.push(LineageNode::new(table));
                        }
                    }
                }
            }
            Some(root)
        }
        Statement::Delete(delete) => {
            let target = extract_delete(&delete.from).unwrap_or_default();
            let mut root = LineageNode::new(format!("DELETE FROM {}", target));
            root.children.push(LineageNode::new("(self)"));
            if let Some(using_list) = &delete.using {
                for twj in using_list {
                    for table in extract_table_with_joins(twj) {
                        root.children.push(LineageNode::new(table));
                    }
                }
            }
            Some(root)
        }
        Statement::CreateView(create_view) => {
            let cte_map = HashMap::new();
            let mut visited = HashSet::new();
            let mut root = LineageNode::new(format!("CREATE VIEW {}", create_view.name));
            let body_node = build_set_expr_tree(create_view.query.body.as_ref(), &cte_map, &mut visited);
            root.children.push(body_node);
            Some(root)
        }
        _ => None,
    }
}

fn build_set_expr_tree(
    expr: &SetExpr,
    cte_map: &HashMap<String, &Query>,
    visited_ctes: &mut HashSet<String>,
) -> LineageNode {
    match expr {
        SetExpr::Select(select) => {
            let mut from_node = LineageNode::new("FROM");
            let mut factors: Vec<&TableFactor> = Vec::new();
            for twj in &select.from {
                factors.push(&twj.relation);
                for join in &twj.joins {
                    factors.push(&join.relation);
                }
            }
            from_node.children = build_factors_as_siblings(&factors, cte_map, visited_ctes);
            from_node
        }
        SetExpr::SetOperation { op, left, right, .. } => {
            let op_name = match op {
                sqlparser::ast::SetOperator::Union => "UNION ALL",
                sqlparser::ast::SetOperator::Except => "EXCEPT",
                sqlparser::ast::SetOperator::Intersect => "INTERSECT",
                _ => "SETOP",
            };
            let mut node = LineageNode::new(op_name);
            node.children.push(build_set_expr_tree(left.as_ref(), cte_map, visited_ctes));
            node.children.push(build_set_expr_tree(right.as_ref(), cte_map, visited_ctes));
            node
        }
        SetExpr::Query(query) => build_set_expr_tree(query.body.as_ref(), cte_map, visited_ctes),
        _ => LineageNode::new("(unknown)"),
    }
}

fn build_set_expr_compact(
    expr: &SetExpr,
    cte_map: &HashMap<String, &Query>,
    visited_ctes: &mut HashSet<String>,
) -> Vec<LineageNode> {
    match expr {
        SetExpr::Select(select) => {
            let mut factors: Vec<&TableFactor> = Vec::new();
            for twj in &select.from {
                factors.push(&twj.relation);
                for join in &twj.joins {
                    factors.push(&join.relation);
                }
            }
            build_factors_as_siblings(&factors, cte_map, visited_ctes)
        }
        SetExpr::SetOperation { op, left, right, .. } => {
            let op_name = match op {
                sqlparser::ast::SetOperator::Union => "UNION ALL",
                sqlparser::ast::SetOperator::Except => "EXCEPT",
                sqlparser::ast::SetOperator::Intersect => "INTERSECT",
                _ => "SETOP",
            };
            let mut node = LineageNode::new(op_name);
            node.children = vec![
                build_set_expr_tree(left.as_ref(), cte_map, visited_ctes),
                build_set_expr_tree(right.as_ref(), cte_map, visited_ctes),
            ];
            vec![node]
        }
        SetExpr::Query(query) => build_set_expr_compact(query.body.as_ref(), cte_map, visited_ctes),
        _ => vec![],
    }
}

fn build_factors_as_siblings(
    factors: &[&TableFactor],
    cte_map: &HashMap<String, &Query>,
    visited_ctes: &mut HashSet<String>,
) -> Vec<LineageNode> {
    factors
        .iter()
        .map(|factor| build_table_factor_node(factor, cte_map, visited_ctes))
        .collect()
}

fn build_table_factor_node(
    factor: &TableFactor,
    cte_map: &HashMap<String, &Query>,
    visited_ctes: &mut HashSet<String>,
) -> LineageNode {
    match factor {
        TableFactor::Table { name, alias, .. } => {
            let table_name = name.to_string();
            let label = if let Some(a) = alias {
                format!("{} {}", table_name, a.name)
            } else {
                table_name.clone()
            };

            if cte_map.contains_key(&table_name) {
                if visited_ctes.contains(&table_name) {
                    LineageNode::new(format!("{} (Recursive)", label))
                } else {
                    visited_ctes.insert(table_name.clone());
                    let cte_query = cte_map[&table_name];
                    let children = build_set_expr_compact(cte_query.body.as_ref(), cte_map, visited_ctes);
                    visited_ctes.remove(&table_name);
                    let mut node = LineageNode::new(format!("{} (CTE)", label));
                    node.children = children;
                    node
                }
            } else {
                LineageNode::new(label)
            }
        }
        TableFactor::Derived { subquery, alias, .. } => {
            let alias_name = alias
                .as_ref()
                .map(|a| a.name.to_string())
                .unwrap_or_else(|| "subquery".to_string());
            let mut node = LineageNode::new(format!("{} (SQRY)", alias_name));
            node.children = build_set_expr_compact(subquery.body.as_ref(), cte_map, visited_ctes);
            node
        }
        _ => LineageNode::new("(unknown)"),
    }
}


// ------------------------------------------------------------------------- / main

fn main() -> Result<()> {
    let input_args: Vec<String> = env::args().collect();
    let mut input_file = None;
    let mut show_tree = false;
    let mut show_flat = false;
    let mut output_file = None;
    let mut args_iter = input_args.iter().skip(1).peekable();

    while let Some(arg) = args_iter.next() {
        match arg.as_str() {
            "--input" => {
                if let Some(next) = args_iter.peek() {
                    if next.starts_with("--") {
                        anyhow::bail!("--input requires a filename, but got another flag: {}", next);
                    }
                }
                input_file = Some(args_iter.next().ok_or_else(|| anyhow::anyhow!("Missing filename for --input"))?.clone());
            }
            "--output" => {
                if let Some(next) = args_iter.peek() {
                    if next.starts_with("--") {
                        anyhow::bail!("--output requires a filename, but got another flag: {}", next);
                    }
                }
                output_file = Some(args_iter.next().ok_or_else(|| anyhow::anyhow!("Missing filename for --output"))?.clone());
            }
            "--tree" => show_tree = true,
            "--flat" => show_flat = true,
            _ => anyhow::bail!("Unknown flag: {}", arg),
        }
    }

    if !show_tree && !show_flat {
        show_tree = true;
        show_flat = true;
    }

    let input_file = input_file.ok_or_else(|| anyhow::anyhow!("--input is required"))?;
    let sql_raw = fs::read_to_string(input_file)?;
    let ast = Parser::parse_sql(&TeradataDialect {}, &normalize_teradata(&sql_raw))?;
    let mut output_lines: Vec<String> = Vec::new();

    for (index, statement) in ast.iter().enumerate() {
        output_lines.push(String::new());
        output_lines.push(format!("-- {}. {} --", index + 1, statement_type(statement)));
        output_lines.push(String::new());

        if show_flat {
            let (sources, target) = extract_statement(statement);
            if sources.is_empty() {
                output_lines.push("Sources: None".to_string());
            } else {
                output_lines.push(format!("Sources: {:?}", sources));
            }
            output_lines.push(format!("Target:  {}", target.as_deref().unwrap_or("None")));
            output_lines.push(String::new());
        }

        if show_tree {
            if let Some(root) = build_statement_lineage_node(statement) {
                output_lines.push(format!("{}", root));
            } else {
                output_lines.push(format!("{} (no diagram)", statement_type(statement)));
            }
            output_lines.push(String::new());
        }
    }

    for line in &output_lines {
        println!("{}", line);
    }

    if let Some(path) = output_file {
        let content = output_lines.join("\n");
        fs::write(&path, content)?;
        eprintln!("Lineage report written to {}", path);
    }

    Ok(())
}

