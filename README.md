# sqLLineage

A simple CLI tool to analyze your SQL and generate lineage.

- **Flat lineage** breaks down into individual statements, shows source tables and target for each statement.
- **Tree lineage** builds ASCII tree to visualize the lineages in a tree-like format, showing dependencies between tables for each statement.
- Supports joins, subqueries, CTEs, recursive CTEs, unions, and intersects (more statement types to be added).
- Supports **Teradata** SQL dialect for parsing (in progress; more dialects to be added).
- Generate outputs to stdout or a file (different structures and formats to be added).


## Installation

Clone the repository and build with Cargo:

```bash
git clone https://github.com/prasanthrangan/sqllineage.git
cd sqllineage
cargo install --path .
```


## Usage

```bash
sqllineage --input <file.sql> [--tree] [--flat] [--output <file>]

# examples:
sqllineage --input /path/to/query.sql --tree
sqllineage --input /path/to/query.sql --flat
sqllineage --input /path/to/query.sql --output /path/to/query.out

```


## Options

| Flag        | Description |
|-------------|-------------|
| `--input`   | Path to the SQL file (required). |
| `--tree`    | Show lineage as an ASCII tree diagram. |
| `--flat`    | Show flat source/target lists for each statement. |
| `--output`  | Write the report to a file (also prints to stdout). |


## Sample

```bash
󰣇 2B/Dev/rust ❯ cat sample.sql

-- 1. Recursive CTE to generate employee hierarchy
WITH RECURSIVE emp_tree AS (
    SELECT id, name, manager_id, 1 AS level
    FROM employees
    WHERE manager_id IS NULL
    UNION ALL
    SELECT e.id, e.name, e.manager_id, et.level + 1
    FROM employees e
    JOIN emp_tree et ON e.manager_id = et.id
)
SELECT et.id, et.name, et.level, d.dept_name
FROM emp_tree et
LEFT JOIN departments d ON et.id = d.manager_id
WHERE et.level <= 3;

-- 2. UPDATE with a subquery and FROM clause
UPDATE products p
SET price = p.price * 1.1
FROM (
    SELECT product_id, discount_rate
    FROM promotions
    WHERE end_date > CURRENT_DATE
) AS prom
WHERE p.id = prom.product_id;

-- 3. SELECT with UNION and CTE
WITH high_value_orders AS (
    SELECT order_id, customer_id, total_amount
    FROM orders
    WHERE total_amount > 1000
),
high_value_customers AS (
    SELECT customer_id, name
    FROM customers
    WHERE loyalty_tier = 'gold'
)
SELECT ho.order_id, hc.name, ho.total_amount
FROM high_value_orders ho
JOIN high_value_customers hc ON ho.customer_id = hc.customer_id
UNION
SELECT order_id, 'Legacy', total_amount
FROM archived_orders
WHERE total_amount > 1000;

-- 4. DELETE with subquery and EXISTS
DELETE FROM logs
WHERE user_id IN (
    SELECT id FROM inactive_users
    WHERE last_login < '2023-01-01'
)
AND log_date < '2023-01-01';

```
```bash
󰣇 2B/Dev/rust ❯ sqllineage --input sample.sql

Sources: ["employees", "employees", "emp_tree", "emp_tree", "departments"]
Target:  None

SELECT
└── FROM
    ├── emp_tree et (CTE)
    │   └── UNION ALL
    │       ├── FROM
    │       │   └── employees
    │       └── FROM
    │           ├── employees e
    │           └── emp_tree et (Recursive)
    └── departments d



-- 2. Update --

Sources: ["products", "promotions"]
Target:  products

UPDATE products
├── (self)
└── promotions



-- 3. Select --

Sources: ["orders", "customers", "high_value_orders", "high_value_customers", "archived_orders"]
Target:  None

SELECT
└── UNION ALL
    ├── FROM
    │   ├── high_value_orders ho (CTE)
    │   │   └── orders
    │   └── high_value_customers hc (CTE)
    │       └── customers
    └── FROM
        └── archived_orders



-- 4. Delete --

Sources: ["logs"]
Target:  logs

DELETE FROM logs
└── (self)

```
