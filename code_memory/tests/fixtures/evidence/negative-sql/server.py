import logging

logger = logging.getLogger(__name__)

# SELECT id FROM orders WHERE id = ?
COMMENT_SQL = "SELECT id FROM orders WHERE id = ?"
logger.info("SELECT id FROM orders WHERE id = ?")
logger.query("SELECT id FROM orders WHERE id = ?")
table = "orders"
dynamic_sql = "SELECT id FROM " + table


def unrelated():
    return "not a database operation"
