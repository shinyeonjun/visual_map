export class Payload {}

export class ResultValue {}

export class BaseService {
  execute(_input: Payload): ResultValue {
    return new ResultValue();
  }
}

export interface Contract {
  execute(input: Payload): ResultValue;
}

export interface ParentContract {}

export interface ChildContract extends ParentContract {}

export class Service extends BaseService implements Contract {
  private current: Payload;

  constructor(current: Payload) {
    super();
    this.current = current;
  }

  execute(input: Payload): ResultValue {
    const transient: Payload = input;
    this.current = transient;
    return new ResultValue();
  }
}

export class Store<T extends Contract> {
  constructor(readonly value: T) {}
}
