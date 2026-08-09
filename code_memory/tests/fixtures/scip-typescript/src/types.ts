export interface Entity {
  id: string;
}

export class User implements Entity {
  constructor(public id: string) {}
}

export class Box<T extends Entity> {
  constructor(private readonly value: T) {}

  get(): T {
    return this.value;
  }
}

export class Handler {
  readonly execute = (value: number): number => value;
}
