class Payload {}

class ResultValue {}

class BaseService {
  ResultValue execute(Payload input) => ResultValue();
}

mixin Audit {}

abstract class Contract {
  ResultValue execute(Payload input);
}

abstract class ParentContract {}

abstract class ChildContract implements ParentContract {}

class Service extends BaseService with Audit implements Contract {
  Payload current;

  Service(this.current);

  @override
  ResultValue execute(Payload input) {
    final Payload transient = input;
    current = transient;
    return ResultValue();
  }
}
