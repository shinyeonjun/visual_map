package typefixture;

final class Payload {}

final class ResultValue {}

class BaseService {
    ResultValue execute(Payload input) {
        return new ResultValue();
    }
}

interface Contract {
    ResultValue execute(Payload input);
}

interface ParentContract {}

interface ChildContract extends ParentContract {}

final class Service extends BaseService implements Contract {
    private Payload current;

    Service(Payload current) {
        this.current = current;
    }

    @Override
    public ResultValue execute(Payload input) {
        Payload transientValue = input;
        current = transientValue;
        return new ResultValue();
    }
}
