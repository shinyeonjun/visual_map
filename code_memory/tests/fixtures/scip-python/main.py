from models import Box, User


def add(left: int, right: int) -> int:
    return left + right


def main() -> int:
    user = Box(User("user-1")).get()
    return add(1, 2) + len(user.user_id)
