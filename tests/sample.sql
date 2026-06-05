-- CTE with aggregation and self-join --
WITH monthly_sales AS (
    SEL
        o.customer_id,
        DATE_TRUNC('month', o.order_date) AS order_month,
        SUM(oi.quantity * oi.unit_price) AS total_spent
    FROM orders o
    INNER JOIN order_items oi ON o.order_id = oi.order_id
    GROUP BY o.customer_id, DATE_TRUNC('month', o.order_date)
),
customer_ranking AS (
    SEL
        ms.customer_id,
        ms.order_month,
        ms.total_spent,
        RANK() OVER (PARTITION BY ms.order_month ORDER BY ms.total_spent DESC) AS rank_in_month
    FROM monthly_sales ms
)
SEL
    c.customer_name,
    cr.order_month,
    cr.total_spent,
    cr.rank_in_month
FROM customer_ranking cr
JOIN customers c ON cr.customer_id = c.customer_id
WHERE cr.rank_in_month <= 3;


-- Subquery with UNION ALL --
SEL
    combined.order_id,
    combined.status,
    combined.channel
FROM (
    SEL order_id, status, 'online' AS channel FROM online_orders
    UNION ALL
    SEL order_id, status, 'store' AS channel FROM store_orders
) combined
WHERE combined.status = 'completed';


-- INSERT from a SELECT
INSERT INTO high_value_orders (order_id, customer_id, total_amount)
SEL
    o.order_id,
    o.customer_id,
    SUM(oi.quantity * oi.unit_price) AS total_amount
FROM orders o
JOIN order_items oi ON o.order_id = oi.order_id
JOIN products p ON oi.product_id = p.product_id
WHERE p.category = 'Electronics'
GROUP BY o.order_id, o.customer_id
HAVING SUM(oi.quantity * oi.unit_price) > 1000;


-- UPDATE with FROM --
UPD order_items oi
SET discount = 10
FROM orders o
WHERE oi.order_id = o.order_id
AND o.order_date < '2023-01-01';


-- Scalar TIME --
SEL CURRENT_TIMESTAMP;


-- Simple DELETE --
DELETE FROM order_items
USING orders
WHERE order_items.order_id = orders.order_id AND orders.status = 'cancelled';


-- Recursive CTE --
WITH RECURSIVE org_chart AS (
    SEL employee_id, manager_id, 1 AS level
    FROM employees
    WHERE manager_id IS NULL
    UNION ALL
    SEL e.employee_id, e.manager_id, oc.level + 1
    FROM employees e
    INNER JOIN org_chart oc ON e.manager_id = oc.employee_id
)
SEL * FROM org_chart;


-- CTEs referencing each other --
WITH cte1 AS (
    SEL * FROM table_a
),
cte2 AS (
    SEL * FROM cte1 JOIN table_b ON cte1.id = table_b.ref_id
),
cte3 AS (
    SEL cte2.*, table_c.value
    FROM cte2 LEFT JOIN table_c ON cte2.id = table_c.fk
)
SEL * FROM cte3
WHERE value > 100;


-- Multi-level subqueries with correlated EXISTS and NOT EXISTS --
SEL
    u.user_id,
    u.name,
    (SEL COUNT(*) FROM orders o WHERE o.user_id = u.user_id AND o.status = 'completed') AS completed_orders
FROM users u
WHERE EXISTS (
    SEL 1 FROM orders o
    WHERE o.user_id = u.user_id
    AND o.order_date >= CURRENT_DATE - INTERVAL '30' DAY
)
AND NOT EXISTS (
    SEL 1 FROM returns r
    WHERE r.user_id = u.user_id
    AND r.return_date > CURRENT_DATE - INTERVAL '90' DAY
);


-- join with CROSS JOIN and LATERAL --
SEL
    p.product_name,
    inv.warehouse_id,
    inv.quantity
FROM products p
CROSS JOIN warehouses w
LEFT JOIN LATERAL (
    SEL quantity FROM inventory
    WHERE product_id = p.product_id AND warehouse_id = w.warehouse_id
) inv ON true;


-- SET operations: INTERSECT and EXCEPT --
SEL customer_id FROM customers WHERE region = 'East'
INTERSECT
SEL customer_id FROM orders WHERE order_date >= '2025-01-01'
EXCEPT
SEL customer_id FROM customers WHERE status = 'inactive';


-- Multiple UNION ALL branches with subqueries --
SEL 'type1', SUM(amount) FROM (
    SEL amount FROM transactions WHERE type = 'A'
    UNION ALL
    SEL amount FROM transactions WHERE type = 'B'
) t1
UNION ALL
SEL 'type2', COUNT(*) FROM transactions WHERE type = 'C'
UNION ALL
SEL 'type3', AVG(amount) FROM transactions WHERE type = 'D';


-- INSERT with a multi-CTE source --
INSERT INTO monthly_summary (region, total_sales, avg_sale, customer_count)
WITH regional_sales AS (
    SEL region, SUM(amount) AS total, AVG(amount) AS avg_amt
    FROM orders
    GROUP BY region
),
regional_customers AS (
    SEL region, COUNT(DISTINCT customer_id) AS cust_cnt
    FROM orders
    GROUP BY region
)
SEL rs.region, rs.total, rs.avg_amt, rc.cust_cnt
FROM regional_sales rs
JOIN regional_customers rc ON rs.region = rc.region;


-- UPDATE with a subquery in SET --
UPD products p
SET price = price * 1.1
WHERE p.category IN (
    SEL category FROM categories WHERE priority > 5
)
AND p.product_id IN (
    SEL product_id FROM order_items GROUP BY product_id HAVING COUNT(*) > 10
);


-- DELETE with a correlated subquery --
DEL FROM order_items oi
WHERE oi.order_id IN (
    SEL order_id FROM orders
    WHERE order_date < CURRENT_DATE - INTERVAL '2' YEAR
    AND status = 'archived'
)
AND oi.product_id NOT IN (
    SEL product_id FROM products WHERE discontinued = false
);

