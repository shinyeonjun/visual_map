def load_orders():
    sql = "SELECT id, customer_id FROM orders WHERE customer_id = ?"
    return sql
