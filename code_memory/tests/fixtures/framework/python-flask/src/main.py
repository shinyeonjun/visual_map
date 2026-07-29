from flask import Blueprint, Flask
from handlers import health

app = Flask(__name__)
app.add_url_rule("/health", view_func=health)
