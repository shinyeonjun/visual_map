from fastapi import FastAPI

app = FastAPI()


def find_order_by_id(order_id: int):
    query = "SELECT id, user_id, status FROM orders WHERE id = ?"
    return {"query": query, "values": [order_id]}


def load_order(order_id: int):
    return find_order_by_id(order_id)


@app.get("/orders/{order_id}")
def get_order(order_id: int):
    return load_order(order_id)


def persist_order_status(order_id: int, status: str):
    query = "UPDATE orders SET status = ? WHERE id = ?"
    return {"query": query, "values": [status, order_id]}


@app.patch("/orders/{order_id}/status")
def change_order_status(order_id: int, status: str):
    return persist_order_status(order_id, status)
