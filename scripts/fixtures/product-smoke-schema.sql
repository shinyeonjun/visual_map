CREATE TABLE users (
  id INTEGER PRIMARY KEY,
  email TEXT NOT NULL UNIQUE
);

CREATE TABLE orders (
  id INTEGER PRIMARY KEY,
  user_id INTEGER NOT NULL REFERENCES users(id),
  status TEXT NOT NULL
);

CREATE VIEW active_orders AS
  SELECT id, user_id, status
  FROM orders
  WHERE status = 'active';

CREATE TRIGGER trg_orders_status
  AFTER UPDATE OF status ON orders
  BEGIN
    SELECT NEW.status;
  END;
