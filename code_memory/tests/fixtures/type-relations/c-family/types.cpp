class Payload {};

class ResultValue {};

class BaseService {
public:
    virtual ResultValue execute(Payload input) {
        return ResultValue{};
    }
};

class Contract {
public:
    virtual ResultValue execute(Payload input) = 0;
};

class Service final : public BaseService, public Contract {
private:
    Payload current;

public:
    explicit Service(Payload input) : current(input) {}

    ResultValue execute(Payload input) override {
        Payload transient = input;
        current = transient;
        return ResultValue{};
    }
};

int main() {
    Payload local;
    Service service(local);
    service.execute(local);
    return 0;
}
