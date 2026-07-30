from fastapi import FastAPI
from services import load_orders

app = FastAPI()


@app.get("/orders")
def orders_endpoint():
    return load_orders()
