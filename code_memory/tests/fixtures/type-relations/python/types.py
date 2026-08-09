class Payload:
    pass


class ResultValue:
    pass


class BaseService:
    def execute(self, item: Payload) -> ResultValue:
        return ResultValue()


class Service(BaseService):
    current: Payload

    def execute(self, item: Payload) -> ResultValue:
        transient: Payload = item
        self.current = transient
        return ResultValue()
