type Database = {
  query<T>(sql: string, values: unknown[]): Promise<T[]>;
  execute(sql: string, values: unknown[]): Promise<void>;
};

export async function loadOrder(db: Database, id: number) {
  return db.query("SELECT id, status FROM orders WHERE id = ?", [id]);
}

export async function updateOrderStatus(db: Database, id: number, status: string) {
  await db.execute("UPDATE orders SET status = ? WHERE id = ?", [status, id]);
}
